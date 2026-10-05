#!/usr/bin/env python3
"""Rewrite citations of retired requirement identifiers.

A requirement identifier is never renumbered and never reused, so a rewrite
that retires one leaves every citation of it pointing at nothing. This tool
takes a mapping of retired identifier to replacement — or to DROP, where the
requirement is gone and nothing replaces it — and applies it across the
corpus, the source, and the tests.

It reads the corpus with the same regexes `spec_check.py` reads it with, so
what it treats as a citation is what the checker treats as a citation. An
identifier inside inline backticks or a fenced block is a mention rather than
a citation (SPC-FR-TQVN) and is never rewritten.

Usage:
    retire.py --map MAPPING [--root DIR] [--include GLOB]... [--apply]

Without --apply it writes nothing and reports what it would change.
"""

import argparse
import os
import re
import sys

# The citation form spec_check.py matches (SPC-FR-KUSH).
CITE_RE = re.compile(r"\b([A-Z]{3})-(FR|TS)-(\d+|[A-Z]{4})\b")
# SPC-FR-TQVN: an identifier inside inline backticks is mentioned, not cited.
INLINE_CODE_RE = re.compile(r"`[^`]*`")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
# In a source file an identifier inside a string literal is DATA — a test
# fixture, an assertion message, a log field — and rewriting one changes what
# the program does rather than what it cites.
STRING_LITERAL_RE = re.compile(r"\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*'")
MARKDOWN_SUFFIXES = (".md",)

DROP = "DROP"

# The three shapes a dropped citation actually occurs in, repaired in order.
# 1. a whole parenthetical that held only citations
PAREN_ONLY_RE = re.compile(r"\s*\((?:per\s+)?(?:`[^`]*`\s*)?\)")
# 2. a list separator left dangling inside a parenthetical
DANGLING_SEP_RE = re.compile(r"\(\s*(?:,|;)\s*")
DOUBLE_SEP_RE = re.compile(r",\s*,")
TRAILING_SEP_RE = re.compile(r"(?:,|;)\s*\)")
# 3. a "per `file.md` " clause whose identifier is gone
PER_CLAUSE_RE = re.compile(r"\s*\(?per\s+`[^`]*`\s*\)?")
SPACE_RE = re.compile(r"[ \t]{2,}")
SPACE_PUNCT_RE = re.compile(r"\s+([,.;:])")

TEXT_SUFFIXES = (".md", ".rs", ".ts", ".tsx", ".py", ".toml", ".json")


def load_mapping(path):
    """One `OLD NEW` or `OLD DROP` pair per line; `#` starts a comment."""
    mapping = {}
    with open(path, encoding="utf-8") as handle:
        for number, raw in enumerate(handle, 1):
            line = raw.split("#", 1)[0].strip()
            if not line:
                continue
            parts = line.split()
            if len(parts) != 2:
                sys.exit(f"{path}:{number}: expected 'OLD NEW', got {line!r}")
            old, new = parts
            if not CITE_RE.fullmatch(old):
                sys.exit(f"{path}:{number}: {old} is not an identifier")
            if new != DROP and not CITE_RE.fullmatch(new):
                sys.exit(f"{path}:{number}: {new} is not an identifier or DROP")
            if old == new:
                sys.exit(f"{path}:{number}: {old} maps to itself")
            mapping[old] = new
    return mapping


def protected_spans(line, in_fence, is_markdown):
    """The regions of one line a citation must not be rewritten inside."""
    if in_fence:
        return [(0, len(line))]
    spans = [m.span() for m in INLINE_CODE_RE.finditer(line)]
    if not is_markdown:
        spans += [m.span() for m in STRING_LITERAL_RE.finditer(line)]
    return spans


def repair(text):
    """Tidy the punctuation a dropped citation left behind."""
    for _ in range(3):
        text = PAREN_ONLY_RE.sub("", text)
        text = DANGLING_SEP_RE.sub("(", text)
        text = DOUBLE_SEP_RE.sub(",", text)
        text = TRAILING_SEP_RE.sub(")", text)
    text = text.replace("()", "")
    # Collapse runs of spaces INSIDE the line only. The leading run is this
    # line's indentation, and collapsing it would reflow the file's shape as a
    # side effect of removing an identifier from a sentence.
    indent = text[: len(text) - len(text.lstrip(" \t"))]
    body = text[len(indent) :]
    body = SPACE_RE.sub(" ", body)
    body = SPACE_PUNCT_RE.sub(r"\1", body)
    return indent + body


def rewrite_line(line, mapping, in_fence, is_markdown=True):
    """Return the rewritten line and how many citations changed."""
    spans = protected_spans(line, in_fence, is_markdown)

    def guarded(match):
        start = match.start()
        for lo, hi in spans:
            if lo <= start < hi:
                return match.group(0)
        return mapping.get(match.group(0), match.group(0))

    replaced = CITE_RE.sub(guarded, line)
    if replaced == line:
        return line, 0

    dropped = False
    for old, new in mapping.items():
        if new == DROP and old in line:
            dropped = True

    def remover(match):
        start = match.start()
        for lo, hi in spans:
            if lo <= start < hi:
                return match.group(0)
        if mapping.get(match.group(0)) == DROP:
            return ""
        return mapping.get(match.group(0), match.group(0))

    out = CITE_RE.sub(remover, line)
    if dropped:
        out = repair(out)
    changed = sum(
        1
        for m in CITE_RE.finditer(line)
        if m.group(0) in mapping
        and not any(lo <= m.start() < hi for lo, hi in spans)
    )
    return out, changed


def walk(root, includes):
    for base, dirs, files in os.walk(root):
        dirs[:] = [
            d
            for d in dirs
            if d not in {".git", "node_modules", "target", "dist", ".venv"}
        ]
        for name in files:
            if not name.endswith(TEXT_SUFFIXES):
                continue
            path = os.path.join(base, name)
            rel = os.path.relpath(path, root)
            if includes and not any(rel.startswith(i) for i in includes):
                continue
            yield path, rel


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--map", required=True)
    parser.add_argument("--root", default=".")
    parser.add_argument("--include", action="append", default=[])
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()

    mapping = load_mapping(args.map)
    total_files = 0
    total_citations = 0

    for path, rel in walk(args.root, args.include):
        try:
            original = open(path, encoding="utf-8").read()
        except (UnicodeDecodeError, OSError):
            continue
        if not any(old in original for old in mapping):
            continue

        out_lines = []
        in_fence = False
        changed_here = 0
        is_markdown = rel.endswith(MARKDOWN_SUFFIXES)
        for line in original.split("\n"):
            if is_markdown and FENCE_RE.match(line):
                in_fence = not in_fence
                out_lines.append(line)
                continue
            new_line, changed = rewrite_line(line, mapping, in_fence, is_markdown)
            changed_here += changed
            out_lines.append(new_line)

        if not changed_here:
            continue
        total_files += 1
        total_citations += changed_here
        print(f"{rel}: {changed_here}")
        if args.apply:
            open(path, "w", encoding="utf-8").write("\n".join(out_lines))

    verb = "rewrote" if args.apply else "would rewrite"
    print(f"\n{verb} {total_citations} citations in {total_files} files")
    if not args.apply:
        print("run again with --apply to write")


if __name__ == "__main__":
    main()
