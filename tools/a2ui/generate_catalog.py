#!/usr/bin/env python3
"""Build resources/a2ui/discussion-catalog.json (AUI-FR-CSOS).

The catalog is the official A2UI v1.0 basic catalog of the pinned
@a2ui/web_core package, with its common types inlined, and the extension
components of the application. Run it from the repository root after a change
of the pinned version or of an extension:

    python3 tools/a2ui/generate_catalog.py
"""
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCHEMAS = ROOT / "node_modules/@a2ui/web_core/src/v1_0/schemas"
OUT = ROOT / "resources/a2ui/discussion-catalog.json"
EXTENSIONS = ROOT / "tools/a2ui/extensions.json"
CATALOG_ID = "synthesis:a2ui/v1"


def local_refs(value):
    """Point every reference at the one document: common types and the
    basic catalog's own definitions both live under `$defs`."""
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            if key == "$ref" and isinstance(item, str):
                item = re.sub(r"^common_types\.json#", "#", item)
                item = re.sub(r"^catalog\.json#", "#", item)
            out[key] = local_refs(item)
        return out
    if isinstance(value, list):
        return [local_refs(v) for v in value]
    return value


def main():
    basic = json.loads((SCHEMAS / "catalogs/basic/catalog.json").read_text())
    common = json.loads((SCHEMAS / "common_types.json").read_text())
    extensions = json.loads(EXTENSIONS.read_text())
    defs = {}
    defs.update(local_refs(common["$defs"]))
    defs.update(local_refs(basic["$defs"]))
    defs.update(extensions["$defs"])
    components = local_refs(basic["components"])
    # AUI-FR-MAMC: no obscured TextField.
    field = components["TextField"]
    for part in field.get("allOf", [field]):
        variant = part.get("properties", {}).get("variant")
        if variant and "enum" in variant:
            variant["enum"] = [v for v in variant["enum"] if v != "obscured"]
    for name, schema in extensions["components"].items():
        components[name] = schema
        defs["anyComponent"]["oneOf"].append({"$ref": f"#/components/{name}"})
    catalog = {
        "$schema": basic["$schema"],
        "$id": CATALOG_ID,
        "catalogId": CATALOG_ID,
        "protocolVersion": basic.get("protocolVersion", "1.0"),
        "title": "Synthesis discussion catalog",
        "basedOn": basic["catalogId"],
        "instructions": extensions["instructions"],
        "components": components,
        "functions": local_refs(basic["functions"]),
        "$defs": defs,
    }
    OUT.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(components)} components, {len(catalog['functions'])} functions")


if __name__ == "__main__":
    main()
