/* Browser-side stand-in for the Tauri IPC bridge, for UI/visual auditing only. */

import { toUnifiedEvent, unifiedInvoke } from "./mock-unified-discussions";
import {
  NOT_HANDLED,
  SHADOW_ISSUE,
  githubPollingInvoke,
} from "./mock-github-polling";
import { documentsInvoke } from "./mock-documents";
import { gitPanelInvoke } from "./mock-git-panel";

const now = "2026-07-28T11:24:00Z";

/**
 * A state the seeded project cannot be driven into from the UI, asked for on the
 * URL instead — `?noComments`, `?notARepo`, `?noChanges`.
 *
 * The empty states of SNV-FR-60 are the reason this exists: a read-only panel
 * (Comments) and a panel reporting on the repository (Changes) have no action
 * that empties them, so without a flag the states are unreachable in the harness
 * and can only be checked in jsdom.
 */
function harnessFlag(name: string): boolean {
  return new URLSearchParams(location.search).has(name);
}

/** The value of a `?name=value` flag, or null when it carries none. */
function harnessValue(name: string): string | null {
  return new URLSearchParams(location.search).get(name);
}

/* --- Session logging (LGC-logging.md / LOG-logs.md) --------------------- */

type MockLogRecord = {
  sequence: number;
  ts: string;
  level: "DEBUG" | "INFO" | "WARN" | "ERROR";
  domains: ("frontend" | "ai" | "backend" | "remote")[];
  message: string;
  fields: Record<string, unknown>;
};

const LOG_LEVEL_ORDER = ["DEBUG", "INFO", "WARN", "ERROR"];

/**
 * A seeded session buffer, so the Logs panel has something to render, filter,
 * and page without the app having to be driven into emitting first.
 *
 * `?noLogs` empties it — the empty-buffer state of LOG-FR-18 is otherwise
 * unreachable here, since the harness seeds records at load.
 */
let logSequence = 0;
function seedLogs(): MockLogRecord[] {
  if (harnessFlag("noLogs")) return [];
  const base = Date.parse("2026-07-28T11:24:00Z");
  const seeds: Omit<MockLogRecord, "sequence" | "ts">[] = [
    { level: "INFO", domains: ["backend"], message: "content root changed", fields: { root: "~/dev/acme-platform" } },
    { level: "DEBUG", domains: ["backend"], message: "scan enumerated 812 files", fields: { files: 812, ms: 143 } },
    { level: "DEBUG", domains: ["frontend"], message: "shell mounted", fields: { panel: "library" } },
    { level: "INFO", domains: ["frontend"], message: "opened LIB-library.md", fields: { path: "specifications/ui/LIB-library.md" } },
    { level: "WARN", domains: ["ai", "remote"], message: "model request retried (attempt 2)", fields: { provider: "openrouter", attempt: 2 } },
    { level: "ERROR", domains: ["backend"], message: "scan aborted: permission denied", fields: { path: "vendor/", errno: 13 } },
    { level: "INFO", domains: ["remote"], message: "fetched 3 branches", fields: { remote: "origin" } },
    { level: "DEBUG", domains: ["ai"], message: "agent turn dispatched", fields: { agent: "arch", tokens: 1420 } },
    { level: "WARN", domains: ["frontend"], message: "comment rail could not re-anchor a thread", fields: { thread: "th-7e3" } },
    { level: "ERROR", domains: ["ai", "remote"], message: "provider returned 429", fields: { provider: "openrouter", retryAfterMs: 2000 } },
  ];
  return seeds.map((s, i) => ({
    ...s,
    sequence: logSequence++,
    ts: new Date(base + i * 1137).toISOString(),
  }));
}
let logBuffer: MockLogRecord[] = seedLogs();
let logGeneration = 0;
let logDropped = 0;

/** The same predicate the Rust buffer applies (LGC-FR-09 / LGC-FR-10). */
function logMatches(r: MockLogRecord, f: any): boolean {
  if (LOG_LEVEL_ORDER.indexOf(r.level) < LOG_LEVEL_ORDER.indexOf(f.minLevel ?? "DEBUG")) {
    return false;
  }
  const domains: string[] = f.domains ?? [];
  if (domains.length > 0 && !r.domains.some((d) => domains.includes(d))) return false;
  const q: string = f.query ?? "";
  if (!q) return true;
  const haystacks = [r.message, ...Object.keys(r.fields), ...Object.values(r.fields).map(String)];
  if (f.queryIsRegex) {
    // LGC-FR-11: an uncompilable pattern is the typed error, not a crash.
    let re: RegExp;
    try {
      re = new RegExp(q);
    } catch {
      throw "invalid query";
    }
    return haystacks.some((h) => re.test(h));
  }
  return haystacks.some((h) => h.toLowerCase().includes(q.toLowerCase()));
}


function tree() {
  const seeded = seededTree();
  // NTA-FR-13 / ASC-FR-10: a file created this session is in the tree the next
  // scan serves, which is how the Project panel comes to have a row to reveal
  // and select. Grafted on rather than seeded, so the fixture stays a pure
  // description of the demo project.
  for (const [path, node] of Object.entries(createdTypedFiles)) {
    const cut = path.lastIndexOf("/");
    const parent = cut === -1 ? seeded : findIn(seeded, path.slice(0, cut));
    if (!parent) continue;
    parent.children = parent.children ?? [];
    if (!parent.children.some((c: any) => c.path === path))
      parent.children.push({ ...node });
  }
  // LCM-FR-02: a rename made this session is in the tree the next scan serves,
  // so the Project panel shows the new name where the old one was. Applied in
  // the order the renames were made, because a second rename of the same node
  // names it by the path the first one gave it.
  for (const { path, newName } of renames) applyRename(seeded, path, newName);
  return seeded;
}

/**
 * Retarget the node at `path` onto a new basename, in place. Every path below it
 * follows: a descendant path is its ancestor path plus its own tail, so the
 * whole subtree moves with one prefix substitution.
 */
function applyRename(root: any, path: string, newName: string) {
  const node = findIn(root, path);
  if (!node) return;
  const cut = path.lastIndexOf("/");
  const next = cut === -1 ? newName : `${path.slice(0, cut)}/${newName}`;
  const retarget = (n: any) => {
    n.path = next + String(n.path).slice(path.length);
    n.id = n.path;
    for (const c of n.children ?? []) retarget(c);
  };
  retarget(node);
  node.name = newName;
}

/** Walk `root` for the node at `path`. */
function findIn(root: any, path: string): any | undefined {
  if (root.path === path) return root;
  for (const c of root.children ?? []) {
    const hit = findIn(c, path);
    if (hit) return hit;
  }
  return undefined;
}

/** The demo project exactly as it is checked out, before this session's edits. */
function seededTree(): any {
  return {
    id: "",
    name: "acme",
    path: "",
    nodeKind: "folder",
    hasArtifacts: true,
    children: [
      {
        id: "specifications",
        name: "specifications",
        path: "specifications",
        nodeKind: "folder",
        hasArtifacts: true,
        children: [
          {
            id: "specifications/ui",
            name: "ui",
            path: "specifications/ui",
            nodeKind: "folder",
            hasArtifacts: true,
            children: [
              {
                id: "specifications/ui/LIB-library.md",
                name: "LIB-library.md",
                path: "specifications/ui/LIB-library.md",
                nodeKind: "file",
                artifactType: "spec",
                typeSource: "inferred",
              },
              {
                id: "specifications/ui/EDT-editor.md",
                name: "EDT-editor.md",
                path: "specifications/ui/EDT-editor.md",
                nodeKind: "file",
                artifactType: "spec",
                typeSource: "assigned",
              },
              // EDT-FR-67 / EDT-FR-68: a file written in every construct the
              // rich surface is meant to model, so "does it render as itself"
              // is answerable in a browser rather than only in jsdom. See
              // `MD_BODY_FIDELITY`.
              {
                id: "specifications/ui/FID-fidelity.md",
                name: "FID-fidelity.md",
                path: "specifications/ui/FID-fidelity.md",
                nodeKind: "file",
                artifactType: "spec",
                typeSource: "inferred",
              },
            ],
          },
          {
            id: "specifications/core",
            name: "core",
            path: "specifications/core",
            nodeKind: "folder",
            hasArtifacts: true,
            children: [
              {
                id: "specifications/core/PST-project-storage.md",
                name: "PST-project-storage.md",
                path: "specifications/core/PST-project-storage.md",
                nodeKind: "file",
                artifactType: "spec",
                typeSource: "inherited",
              },
            ],
          },
        ],
      },
      {
        id: "skills",
        name: "skills",
        path: "skills",
        nodeKind: "folder",
        hasArtifacts: true,
        children: [
          {
            id: "skills/analyst",
            name: "analyst",
            path: "skills/analyst",
            nodeKind: "folder",
            hasArtifacts: true,
            children: [
              {
                id: "skills/analyst/SKILL.md",
                name: "SKILL.md",
                displayName: "analyst",
                path: "skills/analyst/SKILL.md",
                nodeKind: "file",
                artifactType: "skill",
                typeSource: "inferred",
              },
            ],
          },
        ],
      },
      {
        id: "flows",
        name: "flows",
        path: "flows",
        nodeKind: "folder",
        hasArtifacts: true,
        children: [
          {
            id: "flows/release.flow.md",
            name: "release.flow.md",
            path: "flows/release.flow.md",
            nodeKind: "file",
            artifactType: "flow",
            typeSource: "inferred",
          },
        ],
      },
      {
        id: "prompts",
        name: "prompts",
        path: "prompts",
        nodeKind: "folder",
        hasArtifacts: true,
        children: [
          {
            id: "prompts/review.md",
            name: "review.md",
            path: "prompts/review.md",
            nodeKind: "file",
            artifactType: "prompt",
            typeSource: "inferred",
          },
          {
            id: "prompts/triage.md",
            name: "triage.md",
            path: "prompts/triage.md",
            nodeKind: "file",
            artifactType: "prompt",
            typeSource: "inferred",
          },
        ],
      },
      {
        id: "src",
        name: "src",
        path: "src",
        nodeKind: "folder",
        hasArtifacts: false,
        children: [
          { id: "src/main.rs", name: "main.rs", path: "src/main.rs", nodeKind: "file" },
          { id: "src/lib.rs", name: "lib.rs", path: "src/lib.rs", nodeKind: "file" },
          // EDT-FR-74/EDT-FR-78: one file per shape the source surface has to
          // take — a second language, and a file no grammar resolves for, which
          // stays plain while remaining editable.
          {
            id: "src/state",
            name: "state",
            path: "src/state",
            nodeKind: "folder",
            hasArtifacts: false,
            children: [
              {
                id: "src/state/draftSessions.ts",
                name: "draftSessions.ts",
                path: "src/state/draftSessions.ts",
                nodeKind: "file",
              },
            ],
          },
          { id: "src/NOTES.txt", name: "NOTES.txt", path: "src/NOTES.txt", nodeKind: "file" },
        ],
      },
      { id: "README.md", name: "README.md", path: "README.md", nodeKind: "file" },
    ],
  };
}

/**
 * Files created through `create_typed_file` during this session, by path.
 *
 * The seeded tree is a pure function, so a creation has nowhere else to land —
 * and without somewhere to land, a second creation of the same name would not
 * collide and the Project panel would never show the new file (NTA-FR-13,
 * NTA-FR-16).
 */
const createdTypedFiles: Record<string, any> = {};

/**
 * Renames made through `rename_path` this session, oldest first.
 *
 * The seeded tree is a pure function, so a rename has nowhere else to land — and
 * without somewhere to land the Project panel keeps serving the old name, which
 * reads as a rename that did nothing rather than as the harness gap it is.
 */
const renames: { path: string; newName: string }[] = [];

/** Locate a node in the seeded tree by its path, for the commands that need to
 * answer questions about the filesystem rather than just re-serve the tree. */
function findTreeNode(path: string): any | undefined {
  const walk = (n: any): any | undefined => {
    if (n.path === path) return n;
    for (const c of n.children ?? []) {
      const hit = walk(c);
      if (hit) return hit;
    }
    return undefined;
  };
  return walk(tree());
}

const FLOW_BODY = JSON.stringify(
  {
    version: 1,
    name: "Release review",
    description: "Draft the notes, review them, then cut the tag.",
    // FLO-FR-37: a loop holding two steps that decide between themselves, with
    // the graph reaching it as one element (FLO-FR-39).
    loops: [
      {
        id: "l1",
        name: "Review cycle",
        // FLO-FR-37: what the loop is run under, and where its author says
        // what ends it. A prompt, because that is what the picker offers
        // (FLO-FR-10) — a reference no picker could have produced would make
        // the fixture unreachable from the surface it is meant to exercise.
        artifactIds: ["prompts/review.md"],
        maxPasses: 5,
        position: { x: 420, y: 60 },
        size: { width: 460, height: 280 },
      },
    ],
    nodes: [
      {
        id: "n1",
        name: "Draft release notes",
        position: { x: 120, y: 120 },
        artifactIds: ["specifications/ui/LIB-library.md"],
        prompt: "Read the **changelog** and list every `FR` that moved.",
      },
      {
        id: "n2",
        name: "Review with the team",
        parentId: "l1",
        position: { x: 30, y: 90 },
        prompt: "Check every FR that moved this cycle.",
      },
      {
        id: "n3",
        name: "Answer the findings",
        parentId: "l1",
        position: { x: 240, y: 90 },
      },
      { id: "n4", name: "Cut the tag", position: { x: 420, y: 420 } },
    ],
    edges: [
      { id: "e1", from: "n1", to: "l1", label: "needs review" },
      { id: "e2", from: "l1", to: "n4", label: "done" },
      { id: "e3", from: "n2", to: "n3" },
      { id: "e4", from: "n3", to: "n2", label: "again" },
    ],
  },
  null,
  2,
);

/** One entry of the report `validate_flow_document` returns (FGV-FR-04). */
interface HarnessViolation {
  code: string;
  message: string;
  elementId?: string;
  edgeId?: string;
}

/** The harness's stand-in for the Rust validator. See the command's comment. */
function validateFlow(body: string): {
  valid: boolean;
  violations: HarnessViolation[];
} {
  // FGV-FR-05: an empty body is the empty graph.
  if (body.trim() === "") return { valid: true, violations: [] };
  let raw: Record<string, unknown>;
  try {
    raw = JSON.parse(body) as Record<string, unknown>;
  } catch (e) {
    // FGV-FR-06: reported alone — nothing structural applies to a body that
    // could not be read.
    return {
      valid: false,
      violations: [{ code: "not_json", message: `the file is not valid JSON: ${e}` }],
    };
  }
  if (raw?.version !== 1) {
    return {
      valid: false,
      violations: [
        { code: "unknown_version", message: "expected a Flow document of version 1" },
      ],
    };
  }
  const loops = (raw.loops ?? []) as Array<Record<string, unknown>>;
  const nodes = (raw.nodes ?? []) as Array<Record<string, unknown>>;
  const edges = (raw.edges ?? []) as Array<Record<string, unknown>>;
  const parent = new Map<string, string | undefined>();
  for (const e of [...loops, ...nodes]) {
    parent.set(String(e.id), e.parentId === undefined ? undefined : String(e.parentId));
  }
  const violations: HarnessViolation[] = [];
  for (const edge of edges) {
    const id = String(edge.id);
    const from = String(edge.from);
    const to = String(edge.to);
    if (!parent.has(from) || !parent.has(to)) {
      violations.push({
        code: "dangling_edge_endpoint",
        message: `edge \`${id}\` names an element this document does not hold`,
        edgeId: id,
      });
      continue;
    }
    // FGV-FR-12: both endpoints share a container.
    if (parent.get(from) !== parent.get(to)) {
      violations.push({
        code: "cross_container_edge",
        message: `edge \`${id}\` joins \`${from}\` and \`${to}\`, which are in different containers`,
        edgeId: id,
      });
    }
  }
  return { valid: violations.length === 0, violations };
}

/**
 * EDT-FR-73/EDT-FR-74: a source file the harness serves as source rather than as
 * Markdown, so the Editor's source surface has something with a language in it
 * to colour. Keyed by extension — the same fact the extension map reads.
 */
const SOURCE_BODIES: Record<string, string> = {
  rs: `use std::collections::HashMap;

/// Resolve a project path against the active worktree.
pub fn resolve(root: &str, path: &str) -> Option<String> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    seen.insert(path, 1);
    if path.starts_with("..") || path.contains('\\0') {
        return None; // an escape is refused rather than clamped
    }
    Some(format!("{root}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_an_escape() {
        assert_eq!(resolve("/tmp/p", "../etc/passwd"), None);
        assert_eq!(resolve("/tmp/p", "a.md"), Some("/tmp/p/a.md".into()));
    }
}
`,
  ts: `import { invoke } from "@tauri-apps/api/core";

export interface Artifact {
  id: string;
  name: string;
  kind: "markdown" | "flow" | "text";
}

/** Load one artifact's contents, or throw the backend's typed error. */
export async function load(id: string): Promise<string> {
  const res = await invoke<{ body: string }>("load_artifact_contents_by_id", {
    id,
  });
  // A body of zero length is a real answer, not a missing one.
  return res.body ?? "";
}
`,
  json: `{
  "name": "synthesis",
  "private": true,
  "version": "0.1.0",
  "scripts": { "dev": "vite", "build": "tsc && vite build" },
  "dependencies": { "react": "^19.1.0", "highlight.js": "^11.12.0" }
}
`,
  txt: `Plain prose with nothing a grammar would recognise in it at all. This file is
here so the surface can be seen rendering text that stays uncoloured, which is
as much a part of the feature as the colouring is.
`,
};

const MD_BODY = `# Library panel

The Library is the project's artifact browser. It renders the scanned tree and
lets the author open, classify, and act on every artifact in the project.

## Requirements

- **LIB-FR-01** — the panel lists artifacts grouped by folder.
- **LIB-FR-02** — the tree is derived from the filesystem scan.
- **LIB-FR-12** — an "All files" lens shows unclassified files too.

### Notes

Some \`inline code\` and a [link](https://example.com) plus **bold** and *italic*.

\`\`\`ts
const lens = "all_artifacts";
\`\`\`

> A block quote about how the panel behaves when the tree is empty.

1. First ordered item
2. Second ordered item

| Column | Meaning |
| ------ | ------- |
| lens   | filter  |
| text   | search  |

## Behaviour at length

The body below exists so the document is longer than any tab is tall. The page
framing of EDT-FR-63 makes the page the scroll container, and a document that
fits its viewport can never show whether the page's leading and trailing edges
hold their place while the text moves between them.

### Scanning

The scanner walks the worktree once at open and again on every rescan, so the
tree the panel renders is a projection of the filesystem rather than a cache the
author has to remember to refresh.

- The walk skips \`.git\`, \`node_modules\`, and \`.synthesis/drafts\`.
- Classification is by folder assignment first, then by inferred type.
- A file that resolves to no type is still listed under the All files lens.

### Lenses

Each lens is a filter over the same tree rather than a different tree, so the
folder structure an author has learned does not rearrange itself when the lens
changes.

1. All artifacts — every classified file.
2. A single type — Skill, Agent, Prompt, Spec, Flow, Instructions, Scenario.
3. All files — the whole worktree, classified or not.

### Filtering

The filter field matches on the file's name and on its folder path, and it keeps
the ancestors of every match so a hit deep in the tree is reachable rather than
merely counted.

> An empty result reads as an empty state naming the filter, not as an empty
> tree, because the difference between "nothing matches" and "nothing is here"
> is the difference between a typo and a broken scan.

### Ordering

Folders sort before files, and both sort case-insensitively, so the order is the
one a reader of the filesystem expects.

\`\`\`ts
const ordered = [...folders, ...files].sort(byName);
\`\`\`

### Actions

Every action the panel offers is also reachable from the context menu, and the
menu is the whole of the panel's action surface — the rows themselves carry no
buttons that appear on hover, because a row that grows chrome under the pointer
is a row whose contents move while it is being read.

### Persistence

The lens, the filter text, and the expansion state of every folder belong to the
project rather than to the window, so closing the project and reopening it
returns the tree to the shape it was left in.

### Empty projects

A project holding no artifact at all renders an empty state that names the scan
rather than the tree, since the useful next step is a rescan or a new artifact
rather than a filter.

## Closing notes

This trailing section is the bottom of the document. Scrolled fully down, the
page's trailing edge should sit above the tab's foot with field showing beneath
it, and the last line of prose should clear the page's own padding.
`;

/**
 * A second artifact body, carrying a leading YAML frontmatter block so the
 * Editor's frontmatter region (EDT-FR-18 / EDT-FR-21) actually renders. Served
 * for `EDT-editor.md`; `LIB-library.md` keeps the frontmatter-less body so both
 * shapes are reachable in the demo project.
 */
const MD_BODY_FM = `---
title: Editor
spec: EDT
status: draft # not yet reviewed
tags:
  - ui
  - editing
---

# Editor

The Editor is where an artifact is written. It offers a WYSIWYG surface and a
raw-text surface over the same buffer.

## Requirements

- **EDT-FR-17** — the mode toggle swaps the two surfaces.
- **EDT-FR-21** — the frontmatter region renders YAML by role.

\`\`\`ts
const mode: EditorMode = "wysiwyg";
\`\`\`
`;

/**
 * EDT-FR-59: a Skill artifact whose frontmatter carries a top-level scalar
 * `description`, which is what makes the region's `<count>/1024` reading render
 * at all — the demo project's other frontmatter block (`MD_BODY_FM`) carries no
 * such key, so without this body the count line, in both the expanded region's
 * bottom row and the collapsed summary bar, is unreachable in the harness.
 *
 * The description is deliberately far longer than the region's measure, so
 * EDT-FR-18's soft-wrap (no horizontal scrollbar, no overflow past the page's
 * trailing edge) is answerable here too. The body below it is long enough to
 * scroll, so EDT-FR-20's scroll-driven collapse is reachable. Served for
 * `skills/analyst/SKILL.md`.
 */
const MD_BODY_SKILL = `---
name: analyst
description: Author and update specifications for this project. Use whenever the user wants to capture, modify, or formalize a feature, surface, requirement, user story, or behaviour into a spec file, and when the user describes a new surface in enough detail that the natural next step is to write it down rather than to build it.
allowed-tools: Read, Write, Edit, Grep, Glob
---

# Analyst

The analyst turns intent into a specification. It never implements: it produces
spec artifacts and a handoff for the engineer.

## When to use

- The user asks to "spec this out", "write a spec for X", or "update the picker
  spec".
- The user describes a surface or a behaviour in detail without naming a spec.

## What it produces

1. A spec file under \`specifications/\`, numbered in the existing scheme.
2. Functional requirements, each one testable on its own.
3. Test scenarios written Given/When/Then.
4. A handoff block naming every spec the engineer must read.

## What it never does

It writes no code, runs no build, and touches nothing under \`src/\`. A spec that
cannot be implemented is a spec that needs another pass, not a patch applied to
the implementation to make the words true.

## Numbering

Requirements are appended rather than renumbered, so a reference written down in
a commit message a year ago still points at the same sentence today. A
requirement that is withdrawn is marked withdrawn and keeps its number.

## Review

Every spec is read back against the surfaces it touches before it is handed off,
because a requirement that contradicts a neighbouring spec costs more to
discover in code than in prose.

## Closing notes

This trailing section exists so the document scrolls: with the body scrolled
down, the frontmatter region must collapse to its single-line summary bar
(EDT-FR-20) and that bar must still carry the description's reading against the
1024-character budget (EDT-FR-59).
`;

/**
 * EDT-FR-67 / EDT-FR-68: a body written in every construct the WYSIWYG surface
 * is required to model, plus the ones it is required to *preserve* without
 * modelling. Served for `FID-fidelity.md`.
 *
 * The demo project's other two bodies exercise the frontmatter region and the
 * scroll length of a long document; neither carries an image, a task list, a
 * table wider than the page's measure, prose shaped like an HTML tag, or an
 * HTML comment — so none of them can answer whether the rich surface renders
 * those as themselves rather than as markup or as nothing at all.
 *
 * Two images, because they take different paths through the parser: one whose
 * `src` is an ordinary URL and one whose `src` is a base64 data URI. The URL
 * points at a file the harness's own Vite root already serves, so the pixels
 * actually arrive and "does the image render" is answerable without a backend.
 */
const MD_BODY_FIDELITY = `# Fidelity fixture

A paragraph carrying **strong**, *emphasis*, ~~strikethrough~~, \`inline code\`,
and a [link](https://example.com). This line ends in a hard break  
and continues on the next one.

Prose shaped like a tag must survive as prose: <artifact> and
<discussion_history> and <a pre-clear sequence>. So must the comparison 5 < 6
and its mirror 7 > 3.

<!-- an HTML comment: inert content, never markup, never deleted -->

An inline image sits in its sentence: ![the app icon](/src-tauri/icons/32x32.png) and the sentence carries on past it.

A second image, written as a base64 data URI: ![checkerboard](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAHgAAAA8CAIAAAAiz+n/AAAAj0lEQVR42u3asQ0AMAgDMA7jkl7MWV07MiGkWsqcwWOUqMx+8lQ/mt8EDtCgQYMGDRoHaNCgQYMGjQM0aNCgt0Kzm2kGDRo0aNCgQeMADRo0aNCgcYAGDRr0Wmh2hn/QoEGDBo0DNGjQoEGDxgEaNGjQoL1J2Rn+QYMGDRo0DtCgQYMGDRoHaNCgQYP+uPkCDxPP/Vd0+zsAAAAASUVORK5CYII=) and the sentence carries on past it.

> A blockquote, so the quoted passage reads as quoted.
> Its second line belongs to the same quote.

## Lists

- Bullet one
- Bullet two
  - A nested bullet
    - And one deeper still
- Bullet three

1. Ordered one
2. Ordered two
   1. A nested ordered item

- [ ] An unchecked task
- [x] A completed task
- [ ] A third task, still open

## A table wider than the page

| Requirement | Surface | Construct | Renders as | Round trip | Owner | Status | Since | Notes |
| ----------- | ------- | --------- | ---------- | ---------- | ----- | ------ | ----- | ----- |
| EDT-FR-68 | WYSIWYG | table | a real table | spelling only | Editor | done | v1 | the widest row in the file |
| EDT-FR-68 | WYSIWYG | task list | checkboxes | spelling only | Editor | done | v1 | tight, like the bullet list it is written as |
| EDT-FR-67 | WYSIWYG | literal text | prose | byte-for-byte | Editor | done | v1 | no entity ever reaches the file |

---

## A fence with lines wider than the page

\`\`\`ts
const verdict = extensions.filter((e) => e.name === "table").map((e) => e.options).reduce((acc, o) => ({ ...acc, ...o }), {});
\`\`\`

A construct the surface does not model must still be here.[^1]

[^1]: A footnote definition — inert content rather than markup, and never
dropped.
`;

const worktrees = [
  {
    path: "/Users/demo/dev/acme",
    name: "acme",
    branch: "main",
    headShortHash: "4f2a10c",
    isDetached: false,
    isActive: true,
    isPrimary: true,
    isMissing: false,
  },
  {
    path: "/Users/demo/dev/acme-feature",
    name: "acme-feature",
    branch: "feature/new-artifact-window",
    headShortHash: "98dea14",
    isDetached: false,
    isActive: false,
    isPrimary: false,
    isMissing: false,
  },
];

const worktreeContext = {
  repositoryRoot: "/Users/demo/dev/acme",
  activeWorktreePath: "/Users/demo/dev/acme",
  worktrees,
  branches: [
    { name: "develop", kind: "local", headShortHash: "bbb2222", upstream: "origin/develop" },
    { name: "origin/main", kind: "remote", headShortHash: "4f2a10c" },
  ],
};

const changeEntries = [
  {
    id: "specifications/ui/LIB-library.md",
    path: "specifications/ui/LIB-library.md",
    name: "LIB-library.md",
    changeStatus: "modified",
    addedLines: 24,
    removedLines: 6,
    isBinary: false,
    artifactType: "spec",
    typeSource: "inferred",
  },
  {
    id: "specifications/ui/DRP-drafts-panel.md",
    path: "specifications/ui/DRP-drafts-panel.md",
    name: "DRP-drafts-panel.md",
    changeStatus: "added",
    addedLines: 180,
    removedLines: 0,
    isBinary: false,
    artifactType: "spec",
    typeSource: "inferred",
  },
  {
    id: "src/components/NewArtifactModal.tsx",
    path: "src/components/NewArtifactModal.tsx",
    name: "NewArtifactModal.tsx",
    changeStatus: "deleted",
    addedLines: 0,
    removedLines: 212,
    isBinary: false,
  },
  {
    id: "resources/icon.png",
    path: "resources/icon.png",
    name: "icon.png",
    changeStatus: "modified",
    addedLines: null,
    removedLines: null,
    isBinary: true,
  },
  {
    id: "src/state/draftSessions.ts",
    path: "src/state/draftSessions.ts",
    name: "draftSessions.ts",
    changeStatus: "untracked",
    addedLines: 96,
    removedLines: 0,
    isBinary: false,
  },
  {
    id: "src/components/NewArtifactWorkspace.tsx",
    path: "src/components/NewArtifactWorkspace.tsx",
    name: "NewArtifactWorkspace.tsx",
    changeStatus: "renamed",
    previousPath: "src/components/ArtifactWorkspace.tsx",
    addedLines: 12,
    removedLines: 3,
    isBinary: false,
  },
  // CHG-FR-09 / CHG-FR-28: the **Unrevisioned** group needs more than a single
  // file to exercise its own folder nesting and its group-level check cascade —
  // several untracked entries across separate folder spines, one of them
  // classified so the group survives the default **All artifacts** lens, and one
  // deep enough (and long enough) to test the extra indent level's overflow.
  {
    id: "specifications/ui/CHG-changes.md",
    path: "specifications/ui/CHG-changes.md",
    name: "CHG-changes.md",
    changeStatus: "untracked",
    addedLines: 188,
    removedLines: 0,
    isBinary: false,
    artifactType: "spec",
    typeSource: "inferred",
  },
  {
    id: "src/components/ChangesGroupHeader.tsx",
    path: "src/components/ChangesGroupHeader.tsx",
    name: "ChangesGroupHeader.tsx",
    changeStatus: "untracked",
    addedLines: 64,
    removedLines: 0,
    isBinary: false,
  },
  {
    id: ".claude/skills/engineer/references/ui-verification-checklist.md",
    path: ".claude/skills/engineer/references/ui-verification-checklist.md",
    name: "ui-verification-checklist.md",
    changeStatus: "untracked",
    addedLines: 41,
    removedLines: 0,
    isBinary: false,
  },
  // CHG-FR-65: the inline failure report is only reachable if the change set
  // actually holds a path `rollback_paths` refuses. The switch above fails any
  // path containing `fails-rollback`, so the fixture carries one — a tracked
  // modification, so the failure is a restore that could not be written rather
  // than an untracked file that could not be removed.
  {
    id: "src/state/fails-rollback.ts",
    path: "src/state/fails-rollback.ts",
    name: "fails-rollback.ts",
    changeStatus: "modified",
    addedLines: 8,
    removedLines: 2,
    isBinary: false,
  },
  // CHG-FR-59: the confirmation's path list is bounded and scrolls inside its
  // own box. Below ~8 paths it never reaches that bound, so the fixture carries
  // enough entries — one of them deliberately deep — for a browser pass to see
  // the list scroll rather than the dialog grow.
  {
    id: "src/components/RollbackConfirm.tsx",
    path: "src/components/RollbackConfirm.tsx",
    name: "RollbackConfirm.tsx",
    changeStatus: "untracked",
    addedLines: 72,
    removedLines: 0,
    isBinary: false,
  },
  {
    id: "src/hooks/useShellSession.ts",
    path: "src/hooks/useShellSession.ts",
    name: "useShellSession.ts",
    changeStatus: "modified",
    addedLines: 31,
    removedLines: 4,
    isBinary: false,
  },
  {
    id: "src/styles/components.css",
    path: "src/styles/components.css",
    name: "components.css",
    changeStatus: "modified",
    addedLines: 46,
    removedLines: 1,
    isBinary: false,
  },
  {
    id: "src-tauri/src/git.rs",
    path: "src-tauri/src/git.rs",
    name: "git.rs",
    changeStatus: "modified",
    addedLines: 118,
    removedLines: 9,
    isBinary: false,
  },
  {
    id: ".claude/skills/engineer/references/rollback-verification-notes-and-measurements.md",
    path: ".claude/skills/engineer/references/rollback-verification-notes-and-measurements.md",
    name: "rollback-verification-notes-and-measurements.md",
    changeStatus: "untracked",
    addedLines: 12,
    removedLines: 0,
    isBinary: false,
  },
];

const human = {
  kind: "human",
  login: "raver119",
  displayName: "Demo Author",
  email: "author@example.com",
};
/**
 * CMT-FR-77: an agent participant carries the *snapshot* of the title it
 * answered under, so a comment states a role even after the registry's has
 * changed. Three shapes, because all three render differently: an ordinary
 * title, a title long enough that the one-line clip has to do something, and a
 * participant with no title at all (the pre-title records of AGR-FR-24), which
 * must render no line and no placeholder.
 */
const agent = {
  kind: "agent",
  agentId: "claude",
  handle: "claude",
  model: "opus-5",
  title: "UI/UX designer",
};
const agentLongTitle = {
  kind: "agent",
  agentId: "agt-scribe",
  handle: "scribe",
  model: "opus-5",
  title:
    "Documentation editor and long-form technical prose specialist for editorial surfaces",
};
const agentNoTitle = {
  kind: "agent",
  agentId: "agt-long",
  handle: "specification-reviewer-extraordinaire",
  model: "opus-5",
  title: "",
};

/**
 * CMS-FR-42 / CMS-FR-48: the attachment store behind `read_comment_attachment`.
 *
 * A 160x100 checkerboard PNG, small enough to inline and big enough that a
 * thumbnail bounded to the card's measure (CMT-FR-48) is actually visible. Keyed
 * by digest exactly as the backend keys it, so a card that asks for a digest the
 * store does not hold gets the refusal CMT-FR-49 renders as a broken chip.
 */
const ATTACHMENT_BLOBS: Record<string, { mediaType: string; filename: string; data: string }> = {
  "a1b2c3d4e5f60718293a4b5c6d7e8f900112233445566778899aabbccddeeff0": {
    mediaType: "image/png",
    filename: "kickoff-flow.png",
    data:
      "iVBORw0KGgoAAAANSUhEUgAAAKAAAABkCAIAAACO1KzYAAABDklEQVR42u3cMQ0AMAhFQeRUEzoRUSXVUQ8MDOSS7j95t5LGyWq/d7P97M7shtCAhQYsNGDAgO0CtgvYLmDAQgMWGrDQgO0CtgvYLmDAQgMWGrDQgAGPAou1excwYKEBCw0YMGC7gO0CtgsYsNCAhQYsNGC7gO0CtgsYsNCAhQYsNGDAs8BiuaoUGrDQgAEDtgvYLmC7gAELDVhowEIDtgvYLmC7gAELDVhowEIDBgzYrn+y7LqqtAsYsNCAhQYsNGC7gO0CtgsYsNCAhQYsNGC7gO0CtgsYsNCAhQYstH+yILmqtAvYLmC7gAELDVhowEIDtgvYLmC7gAELDVhowEIDBgzYLmC7gAELvXr3A0ddV2zq81nCAAAAAElFTkSuQmCC",
  },
  "0000000000000000000000000000000000000000000000000000000000000001": {
    mediaType: "application/pdf",
    filename: "library-lens-persistence-decision-record-2026-07-26-final-v3.pdf",
    data: "JVBERi0xLjQKJcOkw7zDtsOfCjElJUVPRgo=",
  },
};

const PNG_DIGEST = Object.keys(ATTACHMENT_BLOBS)[0];

/** Comments appended this session, so a posted comment stays in its card. */
const appendedComments: Record<string, any[]> = {};
let appendSeq = 0;

/**
 * CMS-FR-43: an `inline` attachment input becomes a stored `blob`, registered
 * under a fresh digest so `read_comment_attachment` can serve the bytes back.
 * Shared by every append, anchored or discussion, so a picture posted into a
 * discussion round-trips exactly as one posted into a thread does.
 */
function storeAttachments(inputs: any[]): any[] {
  return (inputs ?? []).map((input: any) => {
    if (input.kind === "url") return { ...input };
    const digest = `posted-${++appendSeq}`;
    ATTACHMENT_BLOBS[digest] = {
      mediaType: input.mediaType,
      filename: input.filename,
      data: input.data,
    };
    return {
      kind: "blob",
      digest,
      mediaType: input.mediaType,
      filename: input.filename,
      bytes: Math.floor((input.data?.length ?? 0) * 0.75),
    };
  });
}

/**
 * CMS-FR-42: what a seeded comment attaches. One blob the store holds, one
 * non-image blob so the card has a named chip beside its thumbnail (CMT-FR-48),
 * one link with a label, and one blob whose digest the store does NOT hold, so
 * the "could not be loaded" chip of CMT-FR-49 is reachable without unplugging
 * anything. The long filename is deliberate — a chip has to truncate rather than
 * widen the rail.
 */
const seededAttachments = [
  { kind: "blob", digest: PNG_DIGEST, mediaType: "image/png", filename: "kickoff-flow.png", bytes: 327 },
  {
    kind: "blob",
    digest: "0000000000000000000000000000000000000000000000000000000000000001",
    mediaType: "application/pdf",
    filename: "library-lens-persistence-decision-record-2026-07-26-final-v3.pdf",
    bytes: 91422,
  },
  { kind: "url", url: "https://example.invalid/spec-v2.png", mediaType: "image/png", label: "spec-v2.png" },
];

/**
 * What each seeded anchored thread of `LIB-library.md` differs from the base
 * fixture by. Held here rather than inline in `list_comment_threads` so that
 * everything else rebuilding one — an append, an event payload — rebuilds the
 * SAME thread: a `t-3` that arrives unlocked because the override was left
 * behind reads as CMT-FR-15 being broken and is only the harness forgetting.
 */
const ANCHORED_OVERRIDES: Record<string, Record<string, unknown>> = {
  "t-1": {},
  "t-2": { resolved: true, anchor: { start: 200, end: 240, quote: "All files" } },
  "t-3": { locked: true, anchor: { start: 300, end: 330, quote: "lens" } },
};

function thread(id: string, over: Record<string, unknown> = {}) {
  return {
    id,
    // CMS-FR-36 / CMS-FR-53: every thread now says where its log lives and what
    // it is about. An artifact's anchored thread is the shape the rail has
    // always rendered; a discussion (below) is the same record with `kind`
    // flipped, no `artifactId`, and a null anchor.
    scope: "artifact",
    kind: "anchored",
    artifactId: "specifications/ui/LIB-library.md",
    anchor: { start: 40, end: 96, quote: "renders the scanned tree" },
    comments: [
      {
        id: `${id}-c1`,
        author: human,
        // AGT-FR-29 / AGT-FR-28: the opener carries a live `@all`, a live
        // nickname, and two `@`s that resolve to nobody (an email address and an
        // unenrolled name), so every surface that renders a body — the rail and
        // the Comments panel row — shows a bold tag beside plain prose without a
        // comment having to be posted first.
        body:
          "Should the lens persist per project or per worktree? @all — @arch, " +
          "mail me@example.com, not @nobody.",
        quotes: [],
        // CMT-FR-48 / CMP-FR-25: the opening comment carries the pictures, so
        // the panel row has an attachment count to state.
        attachments: seededAttachments,
        createdAt: "2026-07-26T09:12:00Z",
      },
      {
        id: `${id}-c2`,
        author: agent,
        body: "Per project — `LibraryPanelState` is project-local.\n\nSee **PSS-FR-18**.",
        quotes: [{ commentId: `${id}-c1`, excerpt: "per project or per worktree" }],
        attachments: [],
        createdAt: "2026-07-26T09:20:00Z",
      },
      /**
       * A long tail, on `t-1` alone.
       *
       * A two-comment thread fits every surface that renders it, which makes a
       * whole class of behaviour unobservable in this harness: whether a
       * conversation opens at its latest message, whether it stays at the foot
       * as one arrives, whether the composer really is outside the scroller.
       * Confined to `t-1` so the rail's stacking and the panel's row heights are
       * measured against the short threads they always were.
       */
      ...(id === "t-1"
        ? Array.from({ length: 14 }, (_, i) => ({
            id: `${id}-c${i + 3}`,
            // CMT-FR-77: the agent replies alternate between the three
            // participant shapes so one thread shows a title line, a clipped
            // long one, and none at all.
            author:
              i % 2 === 0
                ? human
                : i === 1
                  ? agentLongTitle
                  : i === 3
                    ? agentNoTitle
                    : agent,
            body:
              i % 2 === 0
                ? `Follow-up ${i + 1}: what happens when the lens is changed from a second window?`
                : `Answer ${i + 1}: the panel re-reads on the event rather than polling — see **LIB-FR-13**.`,
            quotes: [],
            attachments: [],
            createdAt: `2026-07-26T09:${String(21 + i).padStart(2, "0")}:00Z`,
          }))
        : []),
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-07-26T09:12:00Z",
    updatedAt: "2026-07-26T09:20:00Z",
    // The seeded thread's own overrides first, then this caller's — so
    // rebuilding `t-3` anywhere rebuilds it LOCKED unless the caller is the one
    // unlocking it.
    ...(ANCHORED_OVERRIDES[id] ?? {}),
    ...over,
  };
}

/**
 * CMS-FR-53 / CMS-FR-57: a discussion — a thread about a draft as a whole.
 *
 * Deliberately *not* built on `thread()`: a discussion carries no `artifactId`
 * and a null anchor, and spreading the anchored fixture and then deleting them
 * would let a stray anchor leak back in and make CMT-FR-55 (no quote line on the
 * card) look satisfied when it is not.
 */
function discussion(id: string, draftId: string, over: Record<string, unknown> = {}) {
  return {
    id,
    scope: "draft",
    kind: "discussion",
    draftId,
    anchor: null,
    comments: [
      // The comment `prop-0` names as its own (`commentId: "disc-1-c0"`). Without
      // it the already-accepted proposal has no route into the review modal at
      // all — DCR-FR-02 opens it only from a comment's control, the tab's pending
      // indication (which a decided proposal does not raise) or a notification —
      // so DCR-FR-17's decided-for-reading foot was unreachable in the browser.
      {
        id: `${id}-c0`,
        author: agent,
        body: "Folded the two stray bullets into the paragraph above them.",
        quotes: [],
        attachments: [
          {
            kind: "proposal",
            proposalId: "prop-0",
            draftId,
            // DRS-FR-11: the draft's one prompt, which is what a proposal is over.
            path: "Worktree cleanup command.md",
          },
        ],
        createdAt: "2026-07-28T09:50:00Z",
      },
      {
        id: `${id}-c1`,
        author: human,
        body: "Does this draft need a `--force` escape hatch, or is the confirm prompt enough?",
        quotes: [],
        attachments: [],
        createdAt: "2026-07-28T10:02:00Z",
      },
      {
        id: `${id}-c2`,
        author: agent,
        body: "The prompt is enough for interactive use — a flag only matters once this runs in CI.",
        quotes: [{ commentId: `${id}-c1`, excerpt: "is the confirm prompt enough" }],
        attachments: [],
        createdAt: "2026-07-28T10:06:00Z",
      },
      // CMS-FR-60 / PDC-FR-12: the comment a proposal is announced by — the
      // agent's rationale as prose, and one `proposal` attachment the rail
      // renders as a control (CMT-FR-48).
      {
        id: `${id}-c3`,
        author: agent,
        body: "The opening spends four sentences on scope before it says what the command does. I have led with the command.",
        quotes: [],
        attachments: [
          {
            kind: "proposal",
            proposalId: "prop-1",
            draftId,
            path: "Worktree cleanup command.md",
          },
        ],
        createdAt: "2026-07-28T10:14:00Z",
      },
      // CMT-FR-71: `?goneProposal` adds a comment referencing a proposal no
      // reading of this draft will ever return — the log line read from outside
      // the draft it belonged to, or a draft since deleted. It is the only way
      // to reach the control's **missing** rendering, since nothing the author
      // can do in the window removes a proposal record while leaving the comment
      // that announced it behind.
      ...(harnessFlag("goneProposal")
        ? [
            {
              id: `${id}-c4`,
              author: agent,
              body: "Tightened the closing paragraph.",
              quotes: [],
              attachments: [
                {
                  kind: "proposal",
                  proposalId: "prop-gone",
                  draftId,
                  path: "Worktree cleanup command.md",
                },
              ],
              createdAt: "2026-07-28T10:20:00Z",
            },
          ]
        : []),
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-07-28T09:50:00Z",
    updatedAt: "2026-07-28T10:14:00Z",
    ...over,
  };
}

/**
 * CMS-FR-58: the discussions of each draft, in the order they were opened.
 *
 * Mutable and per-session, because unlike the anchored fixtures these are
 * actually written to in the browser: opening one, replying, resolving. A static
 * fixture would make NAW-FR-32 (the posted discussion appears at the head of the
 * margin) and CMT-FR-56 (a resolved one moves to the foot) untestable.
 */
/**
 * ACT-FR-02 / CMS-FR-53: a discussion over a project file rather than a draft.
 *
 * The same record with the draft's scope swapped for the artifact's, which is
 * what the Editor's raw-Markdown mode, the Flow tab and the Diff tab read
 * through the floating panel (ACT-FR-20).
 */
function artifactDiscussion(id: string, artifactId: string, over: Record<string, unknown> = {}) {
  const { draftId: _draftId, ...rest } = discussion(id, "", over) as Record<string, unknown>;
  return { ...rest, scope: "artifact", artifactId, ...over };
}

/**
 * CMS-FR-62 / NTC-FR-19: the one discussion a note carries.
 *
 * The wire shape the backend's `get_or_create_note_discussion` returns —
 * `scope: "note"`, `kind: "discussion"`, `noteId` set, **no** `artifactId` and
 * no `draftId`, `anchor: null`. Built from the draft discussion's comment set so
 * a reopened note conversation has something to render, with both locators
 * stripped: an `artifactId` leaking in would make the overlay believe it has an
 * artifact owner to attach to, which is exactly what CVP-FR-59 forbids.
 */
function noteDiscussion(id: string, noteId: string, over: Record<string, unknown> = {}) {
  const {
    draftId: _draftId,
    artifactId: _artifactId,
    ...rest
  } = discussion(id, "", over) as Record<string, unknown>;
  return {
    ...rest,
    scope: "note",
    kind: "discussion",
    noteId,
    anchor: null,
    // The draft fixture's comments carry proposal attachments, which belong to a
    // draft and not to a note. Two plain messages instead.
    comments: [
      {
        id: `${id}-c1`,
        author: human,
        body: "Should this land before the editor rework, or after it?",
        quotes: [],
        attachments: [],
        createdAt: "2026-08-02T10:02:00Z",
      },
      {
        id: `${id}-c2`,
        author: agent,
        body: "After — the rework moves the surface this depends on.",
        quotes: [{ commentId: `${id}-c1`, excerpt: "before the editor rework" }],
        attachments: [],
        createdAt: "2026-08-02T10:06:00Z",
      },
    ],
    createdAt: "2026-08-02T10:02:00Z",
    updatedAt: "2026-08-02T10:06:00Z",
    ...over,
  };
}

/**
 * ACT-FR-22: the discussions of one *target* — a draft or a project file —
 * keyed exactly as `discussionTargetKey` keys them in `src/types.ts`, because
 * `list_discussion_threads` and `open_discussion_thread` now take a target
 * rather than a `draftId`.
 *
 * Seeded so both routes into the panel are reachable without writing anything:
 * a Flow with one discussion, and a spec whose Editor/Diff tabs read the same
 * conversation (ACT-FR-02, ACT-FR-16, ACT-FR-19).
 */
/**
 * DCP contract surface: the changes agents have proposed to a draft.
 *
 * `d-1` carries one **pending** proposal, so the New Artifact tab's marker
 * (NAW-FR-35), the comment rail's control (CMT-FR-48) and the review modal
 * (DCR-FR-02) are all reachable without an agent turn — and one already
 * **accepted**, so the decided-for-reading state (DCR-FR-17) is too.
 */
const proposalsByDraft: Record<string, any[]> = {
  "d-1": [
    {
      id: "prop-1",
      draftId: "d-1",
      // DRS-FR-11: a draft is one prompt, so every proposal targets that prompt.
      path: "Worktree cleanup command.md",
      agent,
      rationale:
        "The opening spends four sentences on scope before it says what the command does. I have led with the command.",
      threadId: "disc-1",
      commentId: "disc-1-c3",
      state: "pending",
      candidateEdited: false,
      legacy: false,
      hunkCount: 4,
      counts: { pending: 4, accepted: 0, rejected: 0, discussing: 0 },
      ledger: [
        { id: "h-1", kind: "replace", state: "pending", edited: false, revision: 0 },
        { id: "h-2", kind: "add", state: "pending", edited: false, revision: 0 },
        { id: "h-3", kind: "replace", state: "pending", edited: false, revision: 0 },
        { id: "h-4", kind: "replace", state: "pending", edited: false, revision: 0 },
      ],
      createdAt: "2026-07-28T10:14:00Z",
    },
    {
      id: "prop-0",
      draftId: "d-1",
      path: "Worktree cleanup command.md",
      agent,
      rationale: "Folded the two stray bullets into the paragraph above them.",
      threadId: "disc-1",
      commentId: "disc-1-c0",
      state: "accepted",
      candidateEdited: false,
      legacy: false,
      hunkCount: 1,
      counts: { pending: 0, accepted: 1, rejected: 0, discussing: 0 },
      ledger: [
        { id: "h-0", kind: "replace", state: "accepted", edited: false, revision: 0 },
      ],
      createdAt: "2026-07-28T09:50:00Z",
      decidedAt: "2026-07-28T09:58:00Z",
    },
  ],
};

/**
 * DCP-FR-08: the proposed text, keyed by proposal.
 *
 * `prop-2` is here without a matching row in `proposalsByDraft`, deliberately.
 * DCR-FR-03 opens the review on a proposal that *arrives* and not on one that
 * was already standing when the tab opened, so the arrival path can only be
 * reached by firing the event for a proposal the store has never seen:
 *
 * ```js
 * window.__fireBusEvent("draft-change-proposals-changed", {
 *   draftId: "d-1",
 *   proposal: { id: "prop-2", draftId: "d-1", path: "Worktree cleanup command.md",
 *     agent: { kind: "agent", agentId: "ag-1", handle: "arch", model: "…" },
 *     rationale: "…", threadId: "disc-1", commentId: "disc-1-c9",
 *     state: "pending", createdAt: new Date().toISOString() },
 * })
 * ```
 */
const proposalContent: Record<string, string> = {
  "prop-1":
    "# Worktree cleanup\n\n`worktree prune` removes every worktree whose branch is gone.\n\nIt asks before removing anything, and it removes nothing that has uncommitted work in it.\n\n## Scope\n\nOnly worktrees this project created.\n",
  "prop-0": "# Notes\n\nThe two loose ends are now one paragraph.\n",
  "prop-2":
    "# Worktree cleanup\n\n`worktree prune` removes every worktree whose branch has gone.\n\nIt asks first, and it removes nothing that has uncommitted work in it.\n\n## Scope\n\nOnly worktrees this project created. A worktree made by hand is left alone.\n\n## Refusals\n\nA worktree that is currently checked out is reported and skipped.\n",
};

/**
 * DCP-FR-27: the baseline a candidate save is checked against. A real backend
 * hashes the stored bytes; the harness only needs a value that changes on every
 * accepted write, so a per-proposal revision counter stands in for the hash.
 */
/**
 * DCP-FR-HRQN: the ordered changes each proposal holds.
 *
 * Each names the exact prompt text it alters rather than a line range, which is
 * what makes the changes decidable in any order (DCP-FR-VZTK): accepting one
 * rewrites the prompt underneath the others without invalidating what they
 * name. The `before` strings below are literal substrings of `ORIGINAL_PROMPT`,
 * so a review in the browser resolves them and draws them in the prose.
 */
const proposalHunks: Record<string, any[]> = {
  "prop-1": [
    // DCR-FR-05: the first names plain prose; the second and third quote
    // **Markdown** — a heading, an inline-code span, a bulleted list, a
    // paragraph break. A fixture of plain prose alone places every change
    // whether or not the surface can read source syntax, which is how a
    // proposal reporting five changes and drawing one reached an author.
    {
      id: "h-1",
      kind: "replace",
      revision: 0,
      before:
        "This note is about the cleanup command. It should describe the scope of the command, the confirmations it asks for, and the cases it refuses, but at this stage it is mostly scope.",
      after:
        "`worktree prune` removes every worktree whose branch is gone. It asks before removing anything, and it removes nothing that has uncommitted work in it.",
      anchor: {
        lead: "# Worktree cleanup\n\n",
        trail: "\n\n## Scope",
        hint_start: 21,
        hint_end: 200,
      },
    },
    {
      id: "h-2",
      kind: "add",
      revision: 0,
      // DCR-FR-09: an added block with a heading, a bulleted list and bold,
      // so a browser shows whether the proposed text renders rich or raw.
      after:
        "\n\n## Refusals\n\nThe command **never** removes:\n\n- a worktree that is currently checked out\n- a worktree with uncommitted work",
      anchor: {
        lead: "Worktrees this project created.",
        trail: "",
        hint_start: 240,
        hint_end: 240,
      },
    },
    {
      id: "h-3",
      kind: "replace",
      // A heading and the paragraph under it, across a paragraph break.
      before: "## Background\n\nParagraph 1. The command grew out of the worktree work",
      after: "## Where it came from\n\nParagraph 1. The command grew out of the worktree work",
      revision: 0,
      anchor: { lead: "", trail: "", hint_start: 400, hint_end: 470 },
    },
    {
      id: "h-4",
      kind: "replace",
      // An inline-code span, which reaches the rendered text without its
      // backticks.
      before: "`worktree prune` removes every worktree",
      after: "`worktree prune` removes each worktree",
      revision: 0,
      anchor: { lead: "", trail: "", hint_start: 0, hint_end: 40 },
    },
  ],
  "prop-0": [
    {
      id: "h-0",
      kind: "replace",
      revision: 0,
      before: "The two loose ends",
      after: "The two loose ends are now one paragraph",
      anchor: { lead: "# Notes\n\n", trail: "", hint_start: 9, hint_end: 27 },
    },
  ],
  "prop-2": [
    {
      id: "h-9",
      kind: "replace",
      revision: 0,
      before: "Worktrees this project created.",
      after:
        "Only worktrees this project created. A worktree made by hand is left alone.",
      anchor: { lead: "## Scope\n\n", trail: "\n", hint_start: 210, hint_end: 241 },
    },
  ],
};

/**
 * DCP-FR-PWSF: a proposal's own state is derived from its ledger and never
 * stored — `pending` while any change is undecided, `accepted` once every one
 * is decided and at least one was accepted, `rejected` otherwise.
 */
function recountProposal(row: any): void {
  const ledger = row.ledger ?? [];
  const counts = { pending: 0, accepted: 0, rejected: 0, discussing: 0 };
  for (const r of ledger) counts[r.state as keyof typeof counts] += 1;
  row.counts = counts;
  row.hunkCount = ledger.length;
  if (counts.pending + counts.discussing > 0) row.state = "pending";
  else row.state = counts.accepted > 0 ? "accepted" : "rejected";
  row.decidedAt = row.state === "pending" ? null : new Date().toISOString();
}

const candidateRevision: Record<string, number> = {};
function candidateChecksum(proposalId: string): string {
  return `sha-cand-${proposalId}-${candidateRevision[proposalId] ?? 0}`;
}

// ---- prompt change proposals (PCP-prompt-change-proposals.md) -------------
/**
 * PCP contract surface: the changes agents have proposed to a **prompt artifact
 * the project already holds** — the published-file counterpart of the draft
 * proposals above, and a different store with different commands (PCP-FR-29).
 *
 * `prompts/review.md` is the seeded target, exactly the file
 * `PCR-prompt-change-review.md` names in its own wireframe. It carries one
 * **pending** proposal, so the Editor tab's action-cluster indication
 * (PCR-FR-16), the comment rail's control (CMT-FR-48) and the review modal
 * itself (PCR-FR-01) are all reachable without an agent turn — and one already
 * **accepted**, so the decided-for-reading foot (DCR-FR-17) is too.
 */
const PROMPT_ARTIFACT = "prompts/review.md";
const PROMPT_RATIONALE =
  "The opening states what the panel is and what it does in one sentence, and " +
  "the second half is the half that matters. I have split it, and led the " +
  "ordering section with the rule rather than with its reason.";

/**
 * The candidate, derived from the artifact's own body rather than written out
 * beside it: a proposal is a whole document (PPC-FR-04), and building it from
 * `MD_BODY` keeps the two in step while scattering the hunks through a
 * comparison long enough to prove the foot stays pinned (PCR-FR-02).
 */
const PROMPT_CANDIDATE = MD_BODY.replace(
  "The Library is the project's artifact browser. It renders the scanned tree and\nlets the author open, classify, and act on every artifact in the project.",
  "The Library is the project's artifact browser.\n\nIt renders the scanned tree. It lets the author open, classify, and act on\nevery artifact the project holds.",
)
  .replace(
    "- **LIB-FR-01** — the panel lists artifacts grouped by folder.",
    "- **LIB-FR-01** — the panel lists artifacts grouped by folder, folders first.",
  )
  .replace(
    "- The walk skips `.git`, `node_modules`, and `.synthesis/drafts`.",
    "- The walk skips `.git`, `node_modules`, `dist`, and `.synthesis/drafts`.",
  )
  .replace(
    "Folders sort before files, and both sort case-insensitively, so the order is the\none a reader of the filesystem expects.",
    "Folders sort before files. Both sort case-insensitively.\n\nThat is the order a reader of the filesystem expects, so it is the order the\npanel keeps.",
  );

const promptProposalsByArtifact: Record<string, any[]> = {
  [PROMPT_ARTIFACT]: [
    {
      id: "pp-1",
      artifactId: PROMPT_ARTIFACT,
      path: PROMPT_ARTIFACT,
      agent,
      rationale: PROMPT_RATIONALE,
      threadId: "t-pp",
      commentId: "t-pp-c1",
      state: "pending",
      originChecksum: "sha-cand-pp-1-0",
      candidateEdited: false,
      commentOwed: false,
      createdAt: "2026-08-17T09:14:00Z",
    },
    {
      id: "pp-0",
      artifactId: PROMPT_ARTIFACT,
      path: PROMPT_ARTIFACT,
      agent,
      rationale: "Folded the two stray bullets into the paragraph above them.",
      threadId: "t-pp",
      commentId: "t-pp-c0",
      state: "accepted",
      candidateEdited: false,
      commentOwed: false,
      createdAt: "2026-08-15T09:50:00Z",
      decidedAt: "2026-08-15T09:58:00Z",
    },
  ],
};

/**
 * PCP-FR-11: the candidate's text, keyed by proposal.
 *
 * `pp-2` is here without a matching row, deliberately — the same shape the draft
 * fixture uses. PCR-FR-16 opens the review on a proposal that **arrives** and
 * never on one already standing when the tab opened, so the arrival path is
 * reachable only by firing the event for a proposal the store has never seen:
 *
 * ```js
 * window.__fireBusEvent("prompt-change-proposals-changed", {
 *   artifactId: "prompts/review.md",
 *   proposal: { id: "pp-2", artifactId: "prompts/review.md",
 *     path: "prompts/review.md",
 *     agent: { kind: "agent", agentId: "claude", handle: "claude", model: "opus-5", title: "UI/UX designer" },
 *     rationale: "…", threadId: "t-pp", commentId: "t-pp-c9",
 *     state: "pending", candidateEdited: false, commentOwed: false,
 *     createdAt: new Date().toISOString() },
 * })
 * ```
 */
const promptProposalContent: Record<string, string> = {
  "pp-1": PROMPT_CANDIDATE,
  "pp-0": "# Library panel\n\nThe two loose ends are now one paragraph.\n",
  "pp-2": PROMPT_CANDIDATE.replace(
    "### Empty projects",
    "### Refusals\n\nA tree that cannot be scanned is reported rather than rendered empty.\n\n### Empty projects",
  ),
};

const promptCandidateRevision: Record<string, number> = {};
function promptCandidateChecksum(proposalId: string): string {
  return `sha-cand-${proposalId}-${promptCandidateRevision[proposalId] ?? 0}`;
}

/**
 * PCR-FR-11 / PST-FR-16: an artifact whose bytes have moved since the fixture.
 *
 * An acceptance writes here, so the Editor's own surface renders the accepted
 * text with no external-change dialog — and a checksum that moves with it, so
 * the session's reset adopts a new baseline rather than the fixture's `sha-1`.
 */
const artifactBodies: Record<string, { body: string; checksum: string }> = {};
let nextArtifactChecksum = 1;

/**
 * The record a decision is taken against, **recording an arrived proposal the
 * fixture holds no row for**.
 *
 * `pp-2` reaches a window through `__fireBusEvent` alone, because PCR-FR-16
 * opens the review on an *arrival* and never on a proposal already standing.
 * Against a real backend that proposal is a record like any other, so a decision
 * on it must not be refused `proposal_not_found` — which is what made the
 * arrival path undecidable in the browser and looked like a feature defect.
 */
function promptRow(id: string) {
  const list = (promptProposalsByArtifact[PROMPT_ARTIFACT] ??= []);
  const found = list.find((p) => p.id === id);
  if (found) return found;
  if (promptProposalContent[id] === undefined) return undefined;
  const row = {
    id,
    artifactId: PROMPT_ARTIFACT,
    path: PROMPT_ARTIFACT,
    agent,
    rationale: PROMPT_RATIONALE,
    threadId: "t-pp",
    commentId: `t-pp-${id}`,
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    createdAt: new Date().toISOString(),
  };
  list.unshift(row);
  return row;
}

/**
 * CMT-FR-48 / CMS-FR-66: the conversation a prompt proposal is announced in.
 *
 * Anchored on `prompts/review.md` so the Editor tab's own rail renders it, with
 * one comment per proposal carrying a `promptProposal` attachment — which is the
 * rail's route into the review (PCR-FR-03) and the only place the **control
 * naming the file** can be looked at.
 */
function promptProposalThread() {
  return thread("t-pp", {
    artifactId: PROMPT_ARTIFACT,
    anchor: { start: 40, end: 96, quote: "renders the scanned tree" },
    comments: [
      {
        id: "t-pp-c0",
        author: agent,
        body: "Folded the two stray bullets into the paragraph above them.",
        quotes: [],
        attachments: [
          {
            kind: "promptProposal",
            proposalId: "pp-0",
            artifactId: PROMPT_ARTIFACT,
            path: PROMPT_ARTIFACT,
          },
        ],
        createdAt: "2026-08-15T09:50:00Z",
      },
      {
        id: "t-pp-c1",
        author: agent,
        body: PROMPT_RATIONALE,
        quotes: [],
        attachments: [
          {
            kind: "promptProposal",
            proposalId: "pp-1",
            artifactId: PROMPT_ARTIFACT,
            path: PROMPT_ARTIFACT,
          },
        ],
        createdAt: "2026-08-17T09:14:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-08-15T09:50:00Z",
    updatedAt: "2026-08-17T09:14:00Z",
  });
}

const discussionsByTarget: Record<string, any[]> = {
  "draft:d-1": [discussion("disc-1", "d-1")],
  "artifact:flows/release.flow.md": [
    artifactDiscussion("disc-flow-1", "flows/release.flow.md"),
  ],
  "artifact:specifications/ui/EDT-editor.md": [
    artifactDiscussion("disc-edt-1", "specifications/ui/EDT-editor.md"),
  ],
  /**
   * NTS-FR-28: `n-2` already carries a conversation, so **Discuss** on it
   * reopens one holding every message; `n-1` and `n-3` carry none, so the same
   * entry on them is the opening-composer route (NTS-FR-27). Both have to be
   * reachable without writing anything, which is why only one is seeded.
   */
  "note:n-2": [noteDiscussion("disc-note-2", "n-2")],
};

/**
 * CMS-FR-XWDA: the pending question sets the discussions hold, by thread id.
 *
 * `disc-1` — the draft discussion — is seeded with a three-question set, so the
 * answering block (`DQA-discussion-question-answering.md`) is reachable in every
 * presentation that renders that discussion without an agent turn having to
 * record one. The other discussions hold none, so the ordinary composer is
 * reachable too.
 */
const questionSetsByThread: Record<string, any> = {
  "disc-1": {
    setId: "qs-1",
    threadId: "disc-1",
    askedBy: {
      kind: "agent",
      agentId: "ag-arch",
      handle: "arch",
      model: "claude-opus-5",
      title: "Architect",
    },
    askedAt: "2026-09-10T12:00:00Z",
    questions: [
      {
        position: 1,
        text: "Should the ontology live in one spec or two?",
        options: [
          { position: 1, value: "one — a single spec covering both layers" },
          { position: 2, value: "two — paired ui/ and core/ specs" },
        ],
      },
      {
        position: 2,
        text: "Where should the flow diagram go?",
        options: [
          { position: 1, value: "the ui spec" },
          { position: 2, value: "the core spec" },
          { position: 3, value: "both, kept in step" },
        ],
      },
      {
        position: 3,
        text: "Do we rename the existing term?",
        options: [
          { position: 1, value: "rename it now" },
          { position: 2, value: "leave it and add an alias" },
        ],
      },
    ],
  },
};

function discussionTargetKey(target: any): string {
  if (!target || typeof target !== "object") return "";
  if (target.kind === "note") return `note:${target.noteId}`;
  return target.kind === "draft"
    ? `draft:${target.draftId}`
    : `artifact:${target.artifactId}`;
}

/** Every discussion the session holds, whatever target it hangs off. */
function allDiscussions(): any[] {
  return Object.values(discussionsByTarget).flat();
}

/** The discussions filed against one note — at most one (CMS-FR-62). */
function noteDiscussions(noteId: string): any[] {
  return discussionsByTarget[`note:${noteId}`] ?? [];
}

function note(id: string, body: string, over: Record<string, unknown> = {}) {
  return {
    id,
    scope: {
      kind: "entity",
      entityId: "specifications/ui/LIB-library.md",
      entityPath: "specifications/ui/LIB-library.md",
    },
    body,
    createdAt: "2026-07-20T08:00:00Z",
    updatedAt: "2026-07-25T14:30:00Z",
    ...over,
  };
}

/**
 * DRS-FR-29 / DRP-FR-20: the author's organising directories under the drafts
 * root. A folder's identity is its path, so `parent` is derivable from `path`
 * and is carried anyway because that is the wire shape (`DraftFolder`).
 *
 * The seed is deliberately awkward so the panel's layout has something to bite
 * on: it runs three levels deep, holds one folder with a name far wider than
 * the panel, and holds `research`, which is empty and stays empty (DRP-FR-29).
 */
const draftFolders: { path: string; parent: string }[] = [
  { path: "UI", parent: "" },
  { path: "UI/Components", parent: "UI" },
  {
    path: "UI/Components/Buttons and other long-named interactive controls",
    parent: "UI/Components",
  },
  { path: "backend", parent: "" },
  // DRP-FR-29: holds nothing, and still renders.
  { path: "research", parent: "" },
];

/**
 * Rewrite the path of `from` and of everything beneath it to `to`, and re-file
 * every draft that sat under it. A folder's identity being its path, a rename
 * and a move are the same operation on the mock's side, which is why both go
 * through here.
 */
function reparentDraftsSubtree(from: string, to: string) {
  for (const f of draftFolders) {
    if (f.path === from) {
      f.path = to;
      f.parent = to.includes("/") ? to.slice(0, to.lastIndexOf("/")) : "";
    } else if (f.path.startsWith(`${from}/`)) {
      f.path = `${to}${f.path.slice(from.length)}`;
      f.parent = f.path.slice(0, f.path.lastIndexOf("/"));
    }
  }
  for (const row of drafts as any[]) {
    if (row.folder === from) row.folder = to;
    else if (String(row.folder ?? "").startsWith(`${from}/`))
      row.folder = `${to}${String(row.folder).slice(from.length)}`;
  }
}

const drafts = [
  {
    id: "d-1",
    name: "Worktree cleanup command",
    status: "active",
    folder: "UI/Components",
    updatedAt: "2026-07-28T09:41:00Z",
    // DRP-FR-19: this draft carries the seeded pending proposal.
    hasPendingProposal: true,
  },
  {
    id: "d-2",
    name: "Comments export",
    status: "archived",
    folder: "backend",
    updatedAt: "2026-07-27T17:02:00Z",
  },
  /**
   * Filed at the deepest seeded level, so the indentation of a depth-3 row is
   * measurable rather than inferred.
   */
  {
    id: "d-4",
    name: "Icon button sizing",
    status: "active",
    folder: "UI/Components/Buttons and other long-named interactive controls",
    updatedAt: "2026-07-30T08:05:00Z",
  },
  /**
   * DRS-FR-15 / NAW-FR-41: a draft whose storage is NOT the single prompt
   * DRS-FR-11 requires. It still lists (dropping the row would read as data
   * loss), it offers Delete alone in the panel (DRP-FR-33), and opening its tab
   * reaches the blocked state rather than an editing surface — which is the one
   * branch of the New Artifact tab that renders neither the History rail nor the
   * action control.
   */
  {
    id: "d-3",
    name: "Registry migration sketch",
    status: "active",
    // Directly at the implicit root, so the root is a legible drop target.
    folder: "",
    inconsistent: true,
    updatedAt: "2026-07-29T11:15:00Z",
  },
  /**
   * GRD-FR-VLFO / DRP-FR-35: the draft whose graduation run is waiting on a
   * publication choice, so a row's graduation marker is on screen.
   */
  {
    id: "d-5",
    name: "Search ranking rules",
    status: "active",
    folder: "backend",
    updatedAt: "2026-08-12T11:30:00Z",
  },
  /**
   * DRP-FR-07: a draft a graduation has already turned into specifications.
   * Without one, the panel's `Graduated` filter position has nothing to show.
   */
  {
    id: "d-6",
    name: "Notification address rules",
    status: "graduated",
    folder: "backend",
    updatedAt: "2026-08-10T08:00:00Z",
  },
  /**
   * GRU-FR-13 / GRU-FR-37: the draft whose graduation run rests on a
   * multi-question escalation, so the paginated question area is reachable.
   */
  {
    id: "d-7",
    name: "Stale branch pruning",
    status: "active",
    folder: "backend",
    updatedAt: "2026-08-15T12:20:00Z",
  },
  /**
   * GRD-FR-69 … GRD-FR-87: the four drafts whose runs stand in the
   * **implementation** part. Each one crossed the specification publication
   * boundary, so each is `graduated` and read-only, and each is the draft one
   * of the seeded implementation runs names.
   */
  {
    id: "d-8",
    name: "Panel row virtualization",
    status: "graduated",
    folder: "UI",
    updatedAt: "2026-08-18T10:00:00Z",
  },
  {
    id: "d-9",
    name: "Token rotation",
    status: "graduated",
    folder: "backend",
    updatedAt: "2026-08-19T09:00:00Z",
  },
  {
    id: "d-10",
    name: "Diff gutter marks",
    status: "graduated",
    folder: "UI",
    updatedAt: "2026-08-20T14:00:00Z",
  },
  {
    id: "d-11",
    name: "Session restore",
    status: "graduated",
    folder: "backend",
    updatedAt: "2026-08-21T08:00:00Z",
  },
  /** The draft `g-11` graduated — the run whose iteration history is whole. */
  {
    id: "d-12",
    name: "Attachment thumbnails",
    status: "graduated",
    folder: "UI",
    updatedAt: "2026-08-22T15:30:00Z",
  },
  /** The draft `g-12` is blocked on — the run no turn could run the checks for. */
  {
    id: "d-13",
    name: "Log rotation",
    status: "graduating",
    folder: "backend",
    updatedAt: "2026-08-23T11:00:00Z",
  },
  /** The draft `g-13` graduated — the run whose Review let it through with remarks. */
  {
    id: "d-14",
    name: "Search result grouping",
    status: "graduated",
    folder: "UI",
    updatedAt: "2026-08-24T11:30:00Z",
  },
  /** The drafts of `g-14`, `g-15` and `g-16`, the runs of the hidden-path set. */
  {
    id: "d-15",
    name: "Session log capture",
    status: "graduating",
    folder: "backend",
    updatedAt: "2026-08-25T11:20:00Z",
  },
  {
    id: "d-16",
    name: "Reviewer transcript parsing",
    status: "graduating",
    folder: "backend",
    updatedAt: "2026-08-25T14:30:00Z",
  },
  {
    id: "d-17",
    name: "Coverage report bundling",
    status: "graduating",
    folder: "backend",
    updatedAt: "2026-08-26T11:00:00Z",
  },
  /**
   * GPP-FR-XPUO / DRP-FR-ZRJJ: a draft a claim of a GitHub Task created. It sits
   * at the drafts root and is read-only for its whole life.
   */
  {
    id: "gh-1",
    name: "Cache invalidation",
    status: "github_shadow",
    folder: "",
    githubIssue: SHADOW_ISSUE,
    updatedAt: "2026-09-30T10:00:00Z",
  },
];

const draftRecords: Record<string, Record<string, unknown>> = {
  "d-1": {
    id: "d-1",
    name: "Worktree cleanup command",
    // NAW-FR-25 / DRS-FR-25: the ONE prompt the draft's name is bound to.
    promptPath: "Worktree cleanup command.md",
    status: "active",
    createdAt: "2026-07-26T10:00:00Z",
    updatedAt: "2026-07-28T09:41:00Z",
  },
  "d-2": {
    id: "d-2",
    name: "Comments export",
    promptPath: "Comments export.md",
    status: "archived",
    destinationRoot: "specifications",
    createdAt: "2026-07-25T10:00:00Z",
    updatedAt: "2026-07-27T17:02:00Z",
  },
  "d-3": {
    id: "d-3",
    name: "Registry migration sketch",
    // DRS-FR-15: no file of it has been chosen as the prompt.
    promptPath: null,
    inconsistent: true,
    status: "active",
    createdAt: "2026-07-29T11:15:00Z",
    updatedAt: "2026-07-29T11:15:00Z",
  },
  "d-4": {
    id: "d-4",
    name: "Icon button sizing",
    promptPath: "Icon button sizing.md",
    status: "active",
    createdAt: "2026-07-30T08:05:00Z",
    updatedAt: "2026-07-30T08:05:00Z",
  },
  "d-5": {
    id: "d-5",
    name: "Search ranking rules",
    promptPath: "Search ranking rules.md",
    status: "active",
    createdAt: "2026-08-12T10:00:00Z",
    updatedAt: "2026-08-12T11:30:00Z",
  },
  "d-6": {
    id: "d-6",
    name: "Notification address rules",
    promptPath: "Notification address rules.md",
    status: "graduated",
    createdAt: "2026-08-09T10:00:00Z",
    updatedAt: "2026-08-10T08:00:00Z",
  },
  "d-7": {
    id: "d-7",
    name: "Stale branch pruning",
    promptPath: "Stale branch pruning.md",
    status: "active",
    createdAt: "2026-08-15T09:00:00Z",
    updatedAt: "2026-08-15T12:20:00Z",
  },
  "d-8": {
    id: "d-8",
    name: "Panel row virtualization",
    promptPath: "Panel row virtualization.md",
    status: "graduated",
    createdAt: "2026-08-17T10:00:00Z",
    updatedAt: "2026-08-18T10:00:00Z",
  },
  "d-9": {
    id: "d-9",
    name: "Token rotation",
    promptPath: "Token rotation.md",
    status: "graduated",
    createdAt: "2026-08-18T09:00:00Z",
    updatedAt: "2026-08-19T09:00:00Z",
  },
  "d-10": {
    id: "d-10",
    name: "Diff gutter marks",
    promptPath: "Diff gutter marks.md",
    status: "graduated",
    createdAt: "2026-08-19T14:00:00Z",
    updatedAt: "2026-08-20T14:00:00Z",
  },
  "d-11": {
    id: "d-11",
    name: "Session restore",
    promptPath: "Session restore.md",
    status: "graduated",
    createdAt: "2026-08-20T08:00:00Z",
    updatedAt: "2026-08-21T08:00:00Z",
  },
  "d-12": {
    id: "d-12",
    name: "Attachment thumbnails",
    promptPath: "Attachment thumbnails.md",
    status: "graduated",
    createdAt: "2026-08-21T09:00:00Z",
    updatedAt: "2026-08-22T15:30:00Z",
  },
  "d-13": {
    id: "d-13",
    name: "Log rotation",
    promptPath: "Log rotation.md",
    status: "graduating",
    createdAt: "2026-08-23T08:00:00Z",
    updatedAt: "2026-08-23T11:00:00Z",
  },
  "d-14": {
    id: "d-14",
    name: "Search result grouping",
    promptPath: "Search result grouping.md",
    status: "graduated",
    createdAt: "2026-08-23T09:00:00Z",
    updatedAt: "2026-08-24T11:30:00Z",
  },
  "d-15": {
    id: "d-15",
    name: "Session log capture",
    promptPath: "Session log capture.md",
    status: "graduating",
    createdAt: "2026-08-25T09:00:00Z",
    updatedAt: "2026-08-25T11:20:00Z",
  },
  "d-16": {
    id: "d-16",
    name: "Reviewer transcript parsing",
    promptPath: "Reviewer transcript parsing.md",
    status: "graduating",
    createdAt: "2026-08-25T12:00:00Z",
    updatedAt: "2026-08-25T14:30:00Z",
  },
  "d-17": {
    id: "d-17",
    name: "Coverage report bundling",
    promptPath: "Coverage report bundling.md",
    status: "graduating",
    createdAt: "2026-08-26T09:00:00Z",
    updatedAt: "2026-08-26T11:00:00Z",
  },
  /** GPP-FR-XPUO / DRS-FR-XDWS: the seeded GitHub-shadow draft. */
  "gh-1": {
    id: "gh-1",
    name: "Cache invalidation",
    promptPath: "Cache invalidation.md",
    status: "github_shadow",
    githubIssue: SHADOW_ISSUE,
    createdAt: "2026-09-30T10:00:00Z",
    updatedAt: "2026-09-30T10:00:00Z",
  },
};

/**
 * DRS-FR-25: the filename a draft's name derives to — separators and characters
 * illegal in a filename become `-`, then `.md`. Mirrors `primary_file_name` in
 * `src-tauri/src/drafts.rs`, so the rail shows what the real backend would.
 */
const primaryFileName = (name: string) =>
  `${name.trim().replace(/[/\\:*?"<>|\u0000-\u001f]/g, "-")}.md`;

/**
 * DRS-FR-26 on the filesystems this application actually runs on: APFS and NTFS
 * are case-insensitive, so `Spec.md` is already taken when `spec.md` exists.
 * A collision is therefore another file matching case-insensitively — never the
 * file being renamed itself, which `rename_entry` in `src-tauri/src/drafts.rs`
 * recapitalises through a staging name.
 */
const takenBySomeoneElse = (files: string[], target: string, self: string | null) =>
  files.some((f) => f !== self && f.toLowerCase() === target.toLowerCase());

/* --- Draft image assets (DAS-draft-assets.md / NAW-FR-50 … NAW-FR-56) ----
 *
 * An image is NOT a file of the draft (NAW-FR-56): it sits in the draft's own
 * `assets/` folder beside the one prompt, so it is held here rather than in
 * `draftFiles`. The prompt names it `../assets/<id>.<ext>`, because the prompt
 * itself sits in `files/`.
 */

/** A 120x60 checkerboard PNG — the bytes every seeded asset is served as. */
const DRAFT_ASSET_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAHgAAAA8CAIAAAAiz+n/AAAAj0lEQVR42u3asQ0AMAgDMA7jkl7MWV07MiGkWsqcwWOUqMx+8lQ/mt8EDtCgQYMGDRoHaNCgQYMGjQM0aNCgt0Kzm2kGDRo0aNCgQeMADRo0aNCgcYAGDRr0Wmh2hn/QoEGDBo0DNGjQoEGDxgEaNGjQoL1J2Rn+QYMGDRo0DtCgQYMGDRoHaNCgQYP+uPkCDxPP/Vd0+zsAAAAASUVORK5CYII=";

type MockDraftAsset = {
  mediaType: string;
  filename: string | null;
  data: string;
  bytes: number;
};

/**
 * DAS-FR-02: what each draft's `assets/` folder holds, keyed by the
 * draft-relative path `read_draft_image` and `discard_draft_image` name.
 *
 * `d-4` is seeded with one asset the prompt below references twice — once
 * inline and once through a reference-style label — so NAW-FR-52's "every
 * reference form resolves the same way" and NAW-FR-53's "removing one of
 * several references to a shared image leaves every other reference drawing"
 * are both reachable without pasting anything.
 */
const draftAssets: Record<string, Record<string, MockDraftAsset>> = {
  "d-4": {
    "assets/8f3a91c2.png": {
      mediaType: "image/png",
      filename: "button-sizes.png",
      data: DRAFT_ASSET_PNG,
      bytes: 327,
    },
  },
};

let nextAssetOrdinal = 1;

/**
 * NAW-FR-51: the refusal `store_draft_image` answers with, so each of the five
 * typed refusals is reachable in the browser. Off by default — every run would
 * otherwise be unable to paste at all.
 *
 *   http://localhost:5199/?imageRefused=image_too_large
 *   window.__mockImageStoreRefusal = "draft_locked_by_graduation"
 */
let imageStoreRefusal: string | null = harnessValue("imageRefused");
Object.defineProperty(globalThis, "__mockImageStoreRefusal", {
  get: () => imageStoreRefusal,
  set: (code: string | null) => {
    imageStoreRefusal = code || null;
  },
  configurable: true,
});

/**
 * NAW-FR-51: how long `store_draft_image` holds before it answers. Zero by
 * default, which makes the pending state a single microtask and so impossible
 * to see; raise it to watch (and screenshot) the marker at the cursor.
 *
 *   window.__mockImageStoreDelayMs = 4000
 */
let imageStoreDelayMs = Number(harnessValue("imageStoreDelayMs")) || 0;
Object.defineProperty(globalThis, "__mockImageStoreDelayMs", {
  get: () => imageStoreDelayMs,
  set: (ms: number) => {
    imageStoreDelayMs = Number(ms) || 0;
  },
  configurable: true,
});

/**
 * DSH-FR-04 / DSH-FR-08 / DSH-FR-17 / DSH-FR-18: what each of the four
 * Dashboard loaders answers with, and how long it holds before it answers.
 *
 * The seeds in the switch are what a browser pass sees by default. This knob is
 * what makes the states a fixed seed cannot reach checkable in the window: a
 * loader that answers with nothing (the widget hides, or the whole page empties),
 * and one slow enough for the loading and the stale presentations to be read and
 * screenshotted.
 *
 * The keys are the widget names of `DashboardWidgetName`.
 *
 *   window.__mockDashboard.override.recently_edited = []     // delete to clear
 *   window.__mockDashboard.delayMs.pending_git = 4000
 */
const dashboardKnob: {
  override: Record<string, unknown>;
  delayMs: Record<string, number>;
} = { override: {}, delayMs: {} };
(globalThis as Record<string, unknown>).__mockDashboard = dashboardKnob;

/** Puts one loader's seeded answer through the knob above. */
async function dashboardAnswer<T>(widget: string, seeded: T): Promise<T> {
  const ms = Number(dashboardKnob.delayMs[widget]) || 0;
  if (ms > 0) await new Promise((resolve) => setTimeout(resolve, ms));
  const override = dashboardKnob.override[widget];
  return override === undefined ? seeded : (override as T);
}

/**
 * DAS-FR-08: the draft-relative path a Markdown destination resolves to, or
 * `null` where it resolves to no draft-owned asset.
 *
 * Mirrors what the backend refuses: an external address, an absolute path, and
 * a path leaving the draft all resolve to nothing, and the surface renders its
 * placeholder for each (NAW-FR-52).
 */
function draftAssetPath(destination: string): string | null {
  const d = destination.trim();
  if (d === "" || /^[a-z][a-z0-9+.-]*:/i.test(d) || d.startsWith("//")) return null;
  if (d.startsWith("/")) return null;
  // The prompt sits in `files/`, so an asset beside it is one level up.
  const rest = d.startsWith("../") ? d.slice(3) : d;
  if (rest.includes("..")) return null;
  return rest.startsWith("assets/") ? rest : null;
}

/**
 * Draft file bodies an accepted proposal has rewritten (DCP-FR-11), keyed by
 * draft-relative path.
 *
 * `d-4`'s prompt is seeded here so the New Artifact tab opens on a prompt that
 * already embeds pictures: one drawn inline, the same one drawn again through a
 * reference-style label defined at the foot, and three destinations that resolve
 * to no draft-owned asset — a missing file, an external address, and a path
 * leaving the draft. That is exactly the fixture NAW-FR-52 describes, so the
 * drawn image and all three placeholders are on screen the moment the tab opens
 * (NAW-FR-52, EDT-FR-85).
 */
const draftBodies: Record<string, string> = {
  // DCR-FR-05: the prompt the seeded proposal's changes are anchored in, so a
  // review in the browser resolves them and draws them in the prose.
  //
  // DCR-FR-30: it is deliberately **longer than a viewport**, with the two
  // anchored changes at either end of it. A prompt that fits on screen puts
  // every change in view at once, and a browser check of "moving the review
  // scrolls the document to the change" then passes whether or not anything
  // scrolls. The filler sits between the two anchors and touches neither.
  "Worktree cleanup command.md":
    "# Worktree cleanup\n\nThis note is about the cleanup command. It should describe the scope of the command, the confirmations it asks for, and the cases it refuses, but at this stage it is mostly scope.\n\n" +
    "`worktree prune` removes every worktree whose branch is gone.\n\n" +
    "## Background\n\n" +
    Array.from(
      { length: 12 },
      (_, i) =>
        `Paragraph ${i + 1}. The command grew out of the worktree work and has never been written down properly. This paragraph stands between the two proposed changes so that neither is on screen while the other is.`,
    ).join("\n\n") +
    "\n\n## Scope\n\nWorktrees this project created.\n",
  "Icon button sizing.md": `# Icon button sizing

The sizing table, drawn inline: ![the sizing table](../assets/8f3a91c2.png) and
the sentence carries on past it.

The same table again, through a reference-style label: ![the sizing table][sizes]

A destination naming a file that is not there:
![a lost diagram](../assets/does-not-exist.png)

An external address, which the draft does not own:
![a picture on the web](https://example.invalid/sizes.png)

A path leaving the draft: ![somewhere else](../../elsewhere/sizes.png)

The author must still be able to type anywhere in this prompt while those
placeholders are showing.

[sizes]: ../assets/8f3a91c2.png
`,
};

/**
 * DRS-FR-11: what each draft's storage actually holds. A consistent draft holds
 * exactly its one prompt; `d-3` holds two files and no prompt, which is what
 * makes it the inconsistent one.
 *
 * Kept as a list rather than collapsed to the record's `promptPath` because the
 * rename refusal (DRS-FR-26) and the drafts search both read it.
 */
const draftFiles: Record<string, string[]> = {
  "d-1": ["Worktree cleanup command.md"],
  "d-2": ["Comments export.md"],
  // DRS-FR-15: two Markdown files and no way to tell which is the prompt.
  "d-3": ["sketch.md", "registry.md"],
  "d-4": ["Icon button sizing.md"],
  "d-5": ["Search ranking rules.md"],
  "d-6": ["Notification address rules.md"],
  "d-7": ["Stale branch pruning.md"],
  "d-8": ["Panel row virtualization.md"],
  "d-9": ["Token rotation.md"],
  "d-10": ["Diff gutter marks.md"],
  "d-11": ["Session restore.md"],
  "d-12": ["Attachment thumbnails.md"],
  "d-13": ["Log rotation.md"],
  // GPP-FR-XPUO / DRS-FR-XDWS: the seeded GitHub-shadow draft.
  "gh-1": ["Cache invalidation.md"],
};

// ---- the draft's own version history (DHS-draft-history.md) --------------

/**
 * DHS-FR-05 … DHS-FR-11: the settled versions of each draft's prompt, oldest
 * first, and the text each one holds.
 *
 * The rail records **settled versions and nothing else** (NAW-FR-36): an
 * accepted proposal puts one here, the first of them settling the prompt it
 * superseded as `Original` beside it (DHS-FR-07). Author typing puts nothing
 * here — it is simply the live prompt — which is why
 * `save_draft_file_contents` below appends no entry and only moves the live
 * digest.
 *
 * `d-1` carries three versions so the rail's three standings (`Original`,
 * `Superseded`, `Latest accepted`) are all visible at once; `d-2` carries the
 * two a first acceptance leaves. `d-4` carries **none**, which is what a draft
 * nobody has proposed anything to looks like — its live prompt is its
 * `Original`. `d-3` carries none either, being the inconsistent draft that does
 * not open at all.
 */
interface MockHistoryEntry {
  id: string;
  draftId: string;
  seq: number;
  path: string;
  createdAt: string;
  byteLen: number;
  sha256: string;
  source: Record<string, unknown>;
  /** DHS-FR-11: the snapshot's own text, read only when a version is selected. */
  content: string;
}

const ORIGINAL_PROMPT =
  "# Worktree cleanup\n\nThis note is about the cleanup command. It should describe the scope of the command, the confirmations it asks for, and the cases it refuses, but at this stage it is mostly scope.\n\n## Scope\n\nWorktrees this project created.\n";

/**
 * DHS-FR-08 / NAW-FR-36: one version per accepted **change**, each naming the
 * change that produced it.
 *
 * Deferring to the moment the proposal resolves would leave accepted writes
 * with no version recording them, and would make the rail claim the prompt was
 * modified by hand when nobody typed a character.
 */
function recordAcceptedVersion(
  row: any,
  proposalId: string,
  hunkId: string,
  prior: string,
  next: string,
): void {
  const list = (draftHistories[row.draftId] ??= []);
  // DHS-FR-07: the first acceptance settles the prompt it supersedes as the
  // `Original`, which is what a draft holds instead of a version recorded
  // before there was anything to record.
  if (list.length === 0) {
    const original: MockHistoryEntry = {
      id: `dh-${nextHistoryId++}`,
      draftId: row.draftId,
      seq: 1,
      path: row.path,
      createdAt: new Date().toISOString(),
      byteLen: prior.length,
      sha256: `sha-hist-${nextHistoryId}`,
      source: { kind: "original" },
      content: prior,
    };
    list.push(original);
    const { content: _prior, ...wire } = original;
    fireBus("draft-history-changed", { draftId: row.draftId, entry: wire });
  }
  const entry: MockHistoryEntry = {
    id: `dh-${nextHistoryId++}`,
    draftId: row.draftId,
    seq: (list[list.length - 1]?.seq ?? 0) + 1,
    path: row.path,
    createdAt: new Date().toISOString(),
    byteLen: next.length,
    sha256: `sha-hist-${nextHistoryId}`,
    source: {
      kind: "proposal_accepted",
      proposalId,
      hunkId,
      agent: row.agent,
    },
    content: next,
  };
  list.push(entry);
  delete liveDigests[row.draftId];
  const { content: _content, ...wire } = entry;
  fireBus("draft-history-changed", { draftId: row.draftId, entry: wire });
}

const draftHistories: Record<string, MockHistoryEntry[]> = {
  "d-1": [
    {
      id: "dh-1",
      draftId: "d-1",
      seq: 1,
      path: "Worktree cleanup command.md",
      createdAt: "2026-07-26T10:00:00Z",
      byteLen: ORIGINAL_PROMPT.length,
      sha256: "sha-hist-1",
      source: { kind: "original" },
      content: ORIGINAL_PROMPT,
    },
    {
      id: "dh-2",
      draftId: "d-1",
      seq: 2,
      path: "Worktree cleanup command.md",
      createdAt: "2026-07-27T14:22:00Z",
      byteLen: 412,
      sha256: "sha-hist-2",
      source: { kind: "proposal_accepted", proposalId: "prop-0", agent },
      content:
        "# Worktree cleanup\n\n`worktree prune` removes every worktree whose branch is gone.\n\n## Scope\n\nWorktrees this project created.\n",
    },
    {
      id: "dh-3",
      draftId: "d-1",
      seq: 3,
      path: "Worktree cleanup command.md",
      createdAt: "2026-07-28T09:41:00Z",
      byteLen: MD_BODY.length,
      sha256: "sha-hist-3",
      source: { kind: "proposal_accepted", proposalId: "prop-0", agent },
      content: MD_BODY,
    },
  ],
  "d-2": [
    {
      id: "dh-4",
      draftId: "d-2",
      seq: 1,
      path: "Comments export.md",
      createdAt: "2026-07-25T10:00:00Z",
      byteLen: ORIGINAL_PROMPT.length,
      sha256: "sha-hist-4",
      source: { kind: "original" },
      content: ORIGINAL_PROMPT,
    },
    {
      id: "dh-5",
      draftId: "d-2",
      seq: 2,
      path: "Comments export.md",
      createdAt: "2026-07-25T16:12:00Z",
      byteLen: MD_BODY.length,
      sha256: "sha-hist-5",
      source: { kind: "proposal_accepted", proposalId: "prop-0", agent },
      content: MD_BODY,
    },
  ],
  "d-3": [],
  // DHS-FR-07: no version at all — the live prompt is the `Original` until a
  // change is accepted against it.
  "d-4": [],
};

let nextHistoryId = 6;

/**
 * DHS-FR-10: the live prompt's own digest, returned beside the entries so the
 * rail can say whether the author has typed since the newest version
 * (NAW-FR-37) without reading a snapshot.
 *
 * Tracked per draft and moved by every write the tab makes, so the modified
 * marker appears after typing and clears again when an acceptance lands — which
 * is the whole of what the marker is for.
 */
const liveDigests: Record<string, { sha256: string; byteLen: number }> = {};
let nextLiveDigest = 1;

function liveOf(draftId: string) {
  const record: any = draftRecords[draftId];
  const entries = draftHistories[draftId] ?? [];
  const newest = entries[entries.length - 1];
  const moved = liveDigests[draftId];
  return {
    path: String(record?.promptPath ?? newest?.path ?? ""),
    byteLen: moved?.byteLen ?? newest?.byteLen ?? 0,
    sha256: moved?.sha256 ?? newest?.sha256 ?? "sha-live-empty",
    // NAW-FR-37: true until the author has typed since the newest version.
    matchesLatest: moved === undefined,
  };
}

let nextDraftId = 8;

/**
 * DRP-FR-14: the panel state the store hands back, mutated by every save so a
 * reload in the same session restores what was last left rather than the seed.
 */
const draftsPanelState: {
  statusFilter: string;
  textFilter: string;
  expandedFolders: string[];
} = {
  statusFilter: "active",
  textFilter: "",
  // `archive/old` names no folder that exists — retained, never surfaced.
  expandedFolders: ["UI", "UI/Components", "backend", "archive/old"],
};

const githubTokens = [
  {
    id: "tok-1",
    label: "Personal laptop",
    accountLogin: "raver119",
    scopes: ["repo", "workflow"],
    maskedHint: "9f2c",
    addedAt: "2026-05-02T08:00:00Z",
    lastVerifiedAt: "2026-07-27T10:00:00Z",
    state: "valid",
  },
  {
    id: "tok-2",
    label: "CI (fine-grained)",
    accountLogin: null,
    scopes: [],
    maskedHint: "1ab4",
    addedAt: "2026-06-11T08:00:00Z",
    lastVerifiedAt: null,
    state: "unverified",
  },
];

/**
 * AIC-FR-10 / EAC-FR-IRRD: the closed set of turn kinds an override may be held
 * against, declared once so the two setters below cannot drift apart from each
 * other or from `TURN_KINDS` in src-tauri/src/agentic.rs.
 */
const AGENTIC_TURN_KINDS = [
  "authoring",
  "implementation",
  "review",
  "semantic_rebase",
];

const agentics = [
  {
    vendor: "claude_code",
    kind: "cli",
    displayName: "Claude Code",
    binaryPath: "/opt/homebrew/bin/claude",
    pathOrigin: "detected",
    baseUrl: null,
    // AIC-FR-25 / AIC-FR-26: Claude Code is the one CLI vendor that holds a
    // credential — an OAuth token, because it runs where it cannot sign in for
    // itself. It cannot reach `verified` without one, so a verified record
    // always carries `set` plus the hint describing it.
    keyState: "set",
    maskedHint: "ygAA",
    keyRequired: true,
    state: "verified",
    version: "2.1.4",
    verifiedAt: "2026-07-27T10:00:00Z",
    models: [
      { id: "opus-5", label: "Opus 5" },
      { id: "sonnet-5", label: "Sonnet 5" },
    ],
    modelsOrigin: "probed",
    selectedModel: "opus-5",
    reasoningEfforts: [
      { id: "low", label: "Low" },
      { id: "medium", label: "Medium" },
      { id: "high", label: "High" },
      { id: "xhigh", label: "Extra high" },
      { id: "max", label: "Max" },
    ],
    selectedEffort: "high",
    modelOverrides: {} as Record<string, string>,
    effortOverrides: {} as Record<string, string>,
    active: true,
  },
  {
    vendor: "codex",
    kind: "cli",
    displayName: "Codex",
    binaryPath: null,
    pathOrigin: "unset",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: false,
    state: "unconfigured",
    version: null,
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    reasoningEfforts: [],
    selectedEffort: null,
    modelOverrides: {} as Record<string, string>,
    effortOverrides: {} as Record<string, string>,
    active: false,
  },
  {
    vendor: "opencode",
    kind: "cli",
    displayName: "OpenCode",
    binaryPath: "/usr/local/bin/opencode",
    pathOrigin: "user_supplied",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: false,
    state: "missing",
    version: null,
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    reasoningEfforts: [],
    selectedEffort: null,
    modelOverrides: {} as Record<string, string>,
    effortOverrides: {} as Record<string, string>,
    active: false,
  },
  {
    vendor: "claude_agent_api",
    kind: "api",
    displayName: "Claude Agent API",
    binaryPath: null,
    pathOrigin: "unset",
    baseUrl: "https://api.anthropic.com",
    keyState: "set",
    maskedHint: "7d1e",
    keyRequired: true,
    state: "verified",
    version: null,
    verifiedAt: "2026-07-20T10:00:00Z",
    models: [{ id: "claude-opus-5", label: "Claude Opus 5" }],
    modelsOrigin: "probed",
    selectedModel: "claude-opus-5",
    reasoningEfforts: [],
    selectedEffort: null,
    modelOverrides: {} as Record<string, string>,
    effortOverrides: {} as Record<string, string>,
    active: false,
  },
  {
    vendor: "custom_agent_api",
    kind: "api",
    displayName: "Custom Agent API",
    binaryPath: null,
    pathOrigin: "unset",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: false,
    state: "unconfigured",
    version: null,
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    reasoningEfforts: [],
    selectedEffort: null,
    modelOverrides: {} as Record<string, string>,
    effortOverrides: {} as Record<string, string>,
    active: false,
  },
];

const aiApis = [
  {
    provider: "anthropic",
    displayName: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    keyState: "set",
    maskedHint: "4c8b",
    keyRequired: true,
    state: "verified",
    verifiedAt: "2026-07-27T10:00:00Z",
    models: [
      // `ModelReasoning` in src/types.ts: mandatory / defaultEnabled /
      // supportedEfforts / defaultEffort. (The older `{kind,efforts}` shape
      // this seed carried matched nothing the UI reads.)
      { id: "claude-opus-5", label: "Claude Opus 5", reasoning: { mandatory: false, supportedEfforts: ["low", "medium", "high"], defaultEffort: "high" } },
      { id: "claude-sonnet-5", label: "Claude Sonnet 5" },
      { id: "claude-haiku-5", label: "Claude Haiku 5" },
      { id: "claude-opus-5-thinking", label: "Claude Opus 5 (always thinking)", reasoning: { mandatory: true, defaultEnabled: true } },
      // A very long label, so a row and the model selector are laid out against one.
      { id: "claude-opus-5-extended-context-preview-2026-05-14", label: "Claude Opus 5 Extended Context Preview (2026-05-14, long form)" },
    ],
    modelsOrigin: "probed",
    selectedModel: "claude-opus-5",
    selectedReasoning: { kind: "effort", effort: "high" },
    active: true,
  },
  {
    // Verified with a long model list, so the persona editor's filterable model
    // selector (AGT-FR-15) has something to filter when this provider is the
    // active one.
    provider: "openai",
    displayName: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    keyState: "set",
    maskedHint: "9f21",
    keyRequired: true,
    state: "verified",
    verifiedAt: "2026-07-28T09:12:00Z",
    models: [
      { id: "gpt-5", label: "GPT-5", reasoning: { mandatory: false, supportedEfforts: ["minimal", "low", "medium", "high"], defaultEffort: "medium" } },
      { id: "gpt-5-mini", label: "GPT-5 mini", reasoning: { mandatory: false, supportedEfforts: ["low", "high"], defaultEffort: "low" } },
      { id: "gpt-5-nano", label: "GPT-5 nano" },
      { id: "gpt-4.1", label: "GPT-4.1" },
      { id: "gpt-4.1-mini", label: "GPT-4.1 mini" },
      { id: "o4-mini", label: "o4-mini", reasoning: { mandatory: true, supportedEfforts: ["low", "medium", "high"], defaultEffort: "medium" } },
      { id: "o3", label: "o3", reasoning: { mandatory: true, defaultEnabled: true } },
      { id: "o3-pro", label: "o3-pro", reasoning: { mandatory: true, defaultEnabled: true } },
      { id: "gpt-4o", label: "GPT-4o" },
      { id: "gpt-4o-mini", label: "GPT-4o mini" },
      {
        id: "openai/gpt-5-codex-preview-2026-05-14-extended-context",
        label: "GPT-5 Codex Preview (2026-05-14, extended context)",
        reasoning: { mandatory: false, supportedEfforts: ["low", "high"] },
      },
    ],
    modelsOrigin: "probed",
    selectedModel: "gpt-5",
    selectedReasoning: { kind: "effort", effort: "medium" },
    active: false,
  },
  {
    provider: "openrouter",
    displayName: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    keyState: "unavailable",
    maskedHint: "aa10",
    keyRequired: true,
    state: "key_unavailable",
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    selectedReasoning: null,
    active: false,
  },
  {
    provider: "custom",
    displayName: "Custom (OpenAI-compatible)",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: true,
    state: "unconfigured",
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    selectedReasoning: null,
    active: false,
  },
];

// --- Conversational agents (AGR-agent-registry.md / AGT-agents.md) ---------
//
// The machine-wide registry, plus which of them the demo project has enrolled.
// Mutable, because create/update/delete/enrol/remove all write through to it
// and the UI re-renders from what the operation returns.
//
// Deliberate variety: two short nicknames, one long one, and a very long one
// paired with a very long model label, so row layout is exercised; one agent
// on a model the active provider does not offer, so `model_unavailable` is
// visible in every surface that states availability.
type MockAgent = {
  id: string;
  nickname: string;
  /**
   * AGR-FR-23 / AGT-FR-42: the role the persona takes, stored trimmed and `""`
   * where none was named. Seeded deliberately mixed — a plain one, a very long
   * one, and an enrolled agent with none at all — so the mention picker's
   * trailing position (AGT-FR-43) can be seen both filled and blank.
   */
  title: string;
  modelId: string;
  instructions: string;
  createdAt: string;
  updatedAt: string;
  reasoning: { kind: string; effort?: string } | null;
};

let agentsRegistry: MockAgent[] = [
  {
    id: "agt-arch",
    nickname: "arch",
    title: "UI/UX designer",
    modelId: "claude-opus-5",
    instructions:
      "Argue about structure. Say what is missing before what is wrong.",
    createdAt: "2026-06-02T10:00:00Z",
    updatedAt: "2026-07-20T11:30:00Z",
    reasoning: { kind: "effort", effort: "high" },
  },
  {
    id: "agt-sec",
    nickname: "sec",
    title: "Security reviewer",
    modelId: "claude-sonnet-5",
    instructions: "",
    createdAt: "2026-06-04T08:20:00Z",
    updatedAt: "2026-06-04T08:20:00Z",
    reasoning: null,
  },
  {
    id: "agt-scribe",
    nickname: "scribe",
    title:
      "Documentation editor and long-form technical prose specialist",
    // The active provider (Anthropic) does not offer this model, so this agent
    // reads as `model_unavailable` everywhere availability is stated.
    modelId: "anthropic/claude-opus-5",
    instructions: "Rewrite for a reader who has not read the previous page.",
    createdAt: "2026-06-11T14:05:00Z",
    updatedAt: "2026-06-11T14:05:00Z",
    reasoning: null,
  },
  {
    id: "agt-long",
    nickname: "specification-reviewer-extraordinaire",
    // AGT-FR-43: no title at all, on an *enrolled and ready* agent, so the
    // picker row that must leave its trailing position blank is reachable.
    title: "",
    modelId: "claude-opus-5-extended-context-preview-2026-05-14",
    instructions:
      "Read the whole specification before saying anything about any part of it.",
    createdAt: "2026-06-19T16:45:00Z",
    updatedAt: "2026-07-01T09:00:00Z",
    reasoning: { kind: "effort", effort: "high" },
  },
];

/**
 * AAP-FR-APRV: the AI API provider that serves agents, from the registry alone
 * (the key is assumed present). The seed has no project override.
 */
function activeAiApiResolution(): { resolution: string; catalog: any | null } {
  const record = aiApis.find((r) => r.active);
  if (!record) {
    const anyConfigured = aiApis.some((r) => r.state !== "unconfigured");
    return { resolution: anyConfigured ? "none_selected" : "none_configured", catalog: null };
  }
  const state = record.state === "key_unavailable" ? "verified" : record.state;
  if (state !== "verified") return { resolution: "none_selected", catalog: null };
  return {
    resolution: "inherited",
    catalog: {
      provider: record.provider,
      displayName: record.displayName,
      state,
      models: record.models,
    },
  };
}

/** AGR-FR-16: computed at read time from the active catalog, not stored. */
function availabilityOf(agent: MockAgent): string {
  const { resolution, catalog } = activeAiApiResolution();
  if (!catalog) {
    return resolution === "none_configured" ? "provider_unconfigured" : "provider_unverified";
  }
  if (!catalog.models.some((m: { id: string }) => m.id === agent.modelId)) {
    return "model_unavailable";
  }
  return "ready";
}

/** AGR-FR-06: the refusals a create or update gives, with no provider in the draft. */
function checkAgentModel(modelId: string): void {
  const { resolution, catalog } = activeAiApiResolution();
  if (!catalog) {
    throw resolution === "none_configured" ? "provider_unconfigured" : "provider_not_verified";
  }
  if (!catalog.models.some((m: { id: string }) => m.id === modelId)) throw "unknown_model";
}

const byNickname = (a: MockAgent, b: MockAgent) =>
  a.nickname.toLowerCase().localeCompare(b.nickname.toLowerCase());

const sortedAgents = () => [...agentsRegistry].sort(byNickname);

// The demo project's enrolment: two ready agents plus the degraded one, so the
// roster, the project section and the mention picker all show a warning row.
let enrolledAgentIds = ["agt-arch", "agt-scribe", "agt-long"];

const projectAgents = () =>
  sortedAgents()
    .filter((agent) => enrolledAgentIds.includes(agent.id))
    .map((agent) => ({ agent, availability: availabilityOf(agent) }));

/**
 * AGT-FR-36 / CMT-FR-78: enrol extra **ready** agents, so a conversation
 * carrying `@all` resolves to a set far wider than the four the demo seeds.
 *
 * The seed keeps the roster small on purpose — a row layout is read more easily
 * with four rows than with twenty. But the one place a wide roster is the whole
 * point is the composer placeholder, which must stay on one line however many
 * agents a project enrols: that is CSS, so only a browser can measure it, and it
 * needs a roster no fixture should carry by default.
 *
 * Every agent made here takes the model `agt-arch` takes, that model being one
 * the active provider of the seeded AI API registry offers, so each is `ready` and each
 * is therefore named by `@all`. The surfaces read the roster once when they
 * mount (there is no roster-changed event), so re-open the artifact after
 * calling this.
 *
 *   window.__mockEnrolAgents(8)                 // agent01 … agent08
 *   window.__mockEnrolAgents(3, "reviewer")     // reviewer01 … reviewer03
 */
(globalThis as Record<string, unknown>).__mockEnrolAgents = (
  count: number,
  prefix = "agent",
): string[] => {
  const template = agentsRegistry.find((x) => x.id === "agt-arch")!;
  const made: string[] = [];
  for (let i = 1; i <= count; i += 1) {
    const nickname = `${prefix}${String(i).padStart(2, "0")}`;
    if (agentsRegistry.some((x) => x.nickname === nickname)) continue;
    const created: MockAgent = {
      ...template,
      id: `agt-${prefix}-${i}`,
      nickname,
      title: "Extra agent added from the console",
    };
    agentsRegistry = [...agentsRegistry, created];
    enrolledAgentIds = [...enrolledAgentIds, created.id];
    made.push(nickname);
  }
  return made;
};

/**
 * AGC-FR-22: the turns in flight. Seeded with one so the roster's answering
 * marker (AGT-FR-05) is visible without firing an event first; a turn can be
 * added or terminated from the console via `window.__fireBusEvent`.
 */
let agentTurns: any[] = [
  {
    id: "turn-1",
    agentId: "agt-arch",
    nickname: "arch",
    // CMT-FR-42: `t-1` is a thread `list_comment_threads` actually serves, so
    // the pending contribution renders in a card rather than being filtered out
    // for naming a thread the rail has never heard of.
    origin: {
      discussionId: "t-1",
      target: { kind: "artifact", artifactId: "specifications/ui/LIB-library.md" },
      fragmentTarget: {
        owner: { kind: "artifact", artifactId: "specifications/ui/LIB-library.md" },
        path: "specifications/ui/LIB-library.md",
        start: 40,
        end: 96,
        quote: "renders the scanned tree",
      },
    },
    triggerCommentId: "t-1-c1",
    state: "running",
    failure: null,
    // AGC-FR-33: the tool calls active in the turn, ordered by activation. A
    // turn with none reads **Thinking…** (CMT-FR-80); `__mockToolCallBegins`
    // below is how a browser check drives it to any other status.
    activeToolCalls: [],
    startedAt: "2026-08-02T09:14:00Z",
    endedAt: null,
  },
  {
    // The long-nicknamed agent, so a roster row has to fit an ellipsised
    // nickname *and* the answering marker at once (AGT-FR-04, AGT-FR-05).
    id: "turn-2",
    agentId: "agt-long",
    nickname: "specification-reviewer-extraordinaire",
    origin: {
      discussionId: "t-3",
      target: { kind: "artifact", artifactId: "specifications/ui/LIB-library.md" },
      fragmentTarget: {
        owner: { kind: "artifact", artifactId: "specifications/ui/LIB-library.md" },
        path: "specifications/ui/LIB-library.md",
        start: 300,
        end: 330,
        quote: "lens",
      },
    },
    triggerCommentId: "t-3-c1",
    state: "running",
    failure: null,
    activeToolCalls: [],
    startedAt: "2026-08-02T09:20:00Z",
    endedAt: null,
  },
];

/** The harness's own emitter seam (see `mock-event.ts`). */
function fireBus(name: string, payload?: unknown): void {
  const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
    | ((n: string, p?: unknown) => void)
    | undefined;
  fire?.(...toUnifiedEvent(name, payload));
}

/**
 * The thread as `add_comment` would return it — the fixture (or the stored
 * discussion) with everything appended to it this session folded back in, which
 * is the payload `comment-thread-changed` carries (CMS-FR-51).
 */
function foldedThread(threadId: string): any {
  const stored = allDiscussions().find((t) => t.id === threadId);
  if (stored) return { ...stored };
  const base = thread(threadId);
  return {
    ...base,
    comments: [...base.comments, ...(appendedComments[base.id] ?? [])],
  };
}

/**
 * AGC-FR-55: the recovery registry, and a refusal `retry_agent_turn` can be made
 * to answer with so CMT-FR-74's restore-on-refusal is drivable in a browser.
 */
let recoverableTurns: any[] = harnessFlag("failedTurn")
  ? [
      /**
       * CMT-FR-43 / CMT-FR-73: a recoverable failure standing in `t-1`, so the
       * card's Retry offer is there the moment a surface mounts.
       *
       * `agent-turn-state-changed` can raise the same offer from the console,
       * but only in the surface that is mounted when it fires: every surface
       * reads `list_recoverable_agent_turn_failures` once on mount (CMT-FR-75)
       * and the registry answered empty, so attaching, detaching or maximizing
       * the conversation dropped the offer. The offer has to be in the registry
       * for the SAME conversation to carry it in the rail, in the detached
       * overlay and in the maximized tab, which is what CVP-FR-33 is about.
       *
       * Off by default: a Retry offer in every run would change what every
       * other check of this card measures.
       */
      {
        id: "turn-failed-1",
        agentId: "agt-arch",
        nickname: "arch",
        origin: {
          discussionId: "t-1",
          target: { kind: "artifact", artifactId: "specifications/ui/LIB-library.md" },
          fragmentTarget: {
            owner: { kind: "artifact", artifactId: "specifications/ui/LIB-library.md" },
            path: "specifications/ui/LIB-library.md",
            start: 40,
            end: 96,
            quote: "renders the scanned tree",
          },
        },
        triggerCommentId: "t-1-c1",
        state: "failed",
        failure: "unreachable",
        // AGC-FR-55: only a retryable failure is offered again (CVL-FR-18).
        retryPermitted: true,
        // AGC-FR-34: a turn in any terminal state holds no active call.
        activeToolCalls: [],
        startedAt: "2026-08-02T09:10:00Z",
        endedAt: "2026-08-02T09:11:00Z",
      },
    ]
  : [];
/**
 * AGC-FR-39 / CMT-FR-82: the image-notice registry — at most one entry per
 * conversation, each a terminal turn that carried `imagesOmitted`.
 *
 * Off by default, because a notice beside every agent contribution would change
 * what every other check of the card measures:
 *
 *   http://localhost:5199/?imageNotice
 *   window.__mockTurnOmitsImages("disc-1")   // raises one from the console
 */
let imageNoticeTurns: any[] = harnessFlag("imageNotice")
  ? [
      {
        id: "turn-images-omitted-1",
        agentId: "agt-arch",
        nickname: "arch",
        // CMT-FR-82: the notice sits with the agent's own contribution, so it
        // hangs off the conversation the contribution is in. The origin carries
        // the draft as well as the thread, because that is what a surface
        // matches its own conversations on.
        origin: { discussionId: "disc-1", target: { kind: "draft", draftId: "d-1" }, fragmentTarget: null },
        triggerCommentId: "disc-1-c1",
        state: "succeeded",
        failure: null,
        retryPermitted: false,
        imagesOmitted: true,
        activeToolCalls: [],
        startedAt: "2026-07-28T09:52:00Z",
        endedAt: "2026-07-28T09:53:00Z",
      },
      {
        id: "turn-images-omitted-2",
        agentId: "agt-arch",
        nickname: "arch",
        origin: {
          discussionId: "disc-edt-1",
          target: { kind: "artifact", artifactId: "specifications/ui/EDT-editor.md" },
          fragmentTarget: null,
        },
        triggerCommentId: "disc-edt-1-c1",
        state: "succeeded",
        failure: null,
        retryPermitted: false,
        imagesOmitted: true,
        activeToolCalls: [],
        startedAt: "2026-08-02T09:12:00Z",
        endedAt: "2026-08-02T09:13:00Z",
      },
    ]
  : [];

/**
 * AGC-FR-39: a terminal turn that omitted its images, published the way the
 * backend publishes it — the registry entry is written before the event, so a
 * card that reads either sees the same thing (CMT-FR-82).
 */
(globalThis as Record<string, unknown>).__mockTurnOmitsImages = (
  threadId: string,
  target: Record<string, unknown> = { kind: "draft", draftId: "d-1" },
): any => {
  const turn = {
    id: `turn-images-${Math.random().toString(36).slice(2, 8)}`,
    agentId: "agt-arch",
    nickname: "arch",
    origin: { discussionId: threadId, target, fragmentTarget: null },
    triggerCommentId: `${threadId}-c1`,
    state: "succeeded",
    failure: null,
    retryPermitted: false,
    imagesOmitted: true,
    activeToolCalls: [],
    startedAt: new Date().toISOString(),
    endedAt: new Date().toISOString(),
  };
  imageNoticeTurns = [
    turn,
    ...imageNoticeTurns.filter(
      (t) => t.origin.discussionId !== threadId,
    ),
  ];
  fireBus("agent-turn-state-changed", { ...turn });
  return turn;
};

let retryRefusal: string | null = null;
/**
 * CMT-FR-74: how long `retry_agent_turn` holds before it answers. Zero — a
 * dispatch that initiates within a microtask — is the default, so nothing that
 * does not ask for it waits; a browser check of the **disabled** control needs
 * the dispatch to stay in flight long enough to be seen, which is what this is:
 *
 *   window.__mockRetryDelayMs = 1500
 */
let retryDelayMs = 0;
Object.defineProperty(globalThis, "__mockRetryDelayMs", {
  get: () => retryDelayMs,
  set: (ms: number) => {
    retryDelayMs = Number(ms) || 0;
  },
  configurable: true,
});

/**
 * AGC-FR-52 / CMT-FR-73: a turn whose attempts ran out on a **recoverable**
 * failure, which is what leaves a failed contribution carrying Retry.
 *
 * Two events in the order the backend emits them is not needed here: the turn's
 * own terminal event carries `retryPermitted`, and the entry the query serves is
 * written before it is published (AGC-FR-55), so a card that reads either sees
 * the same thing.
 *
 *   window.__mockTurnFailsRecoverably("turn-1", "unreachable")
 */
(globalThis as Record<string, unknown>).__mockTurnFailsRecoverably = (
  turnId: string,
  failure = "unreachable",
): any => {
  const turn = agentTurns.find((t) => t.id === turnId);
  if (!turn) throw new Error(`no such turn: ${turnId}`);
  const failed = {
    ...turn,
    state: "failed",
    failure,
    retryPermitted: true,
    // AGC-FR-34: a terminal event carries an empty list whatever was active.
    activeToolCalls: [],
    endedAt: new Date().toISOString(),
  };
  agentTurns = agentTurns.filter((t) => t.id !== turnId);
  // At most one per conversation: a later recoverable failure replaces the one
  // it displaces, so the card exposes Retry for the most recently failed turn.
  recoverableTurns = [
    failed,
    ...recoverableTurns.filter(
      (t) => JSON.stringify(t.origin) !== JSON.stringify(failed.origin),
    ),
  ];
  fireBus("agent-turn-state-changed", failed);
  return failed;
};

/**
 * CMT-FR-74: make the next `retry_agent_turn` refuse, so the failure
 * contribution is restored and the typed reason renders inline.
 *
 *   window.__mockRefuseRetry("thread_locked")   // null clears it
 */
(globalThis as Record<string, unknown>).__mockRefuseRetry = (
  reason: string | null,
): void => {
  retryRefusal = reason;
};

/**
 * AGC-FR-48: drive one turn to a state from the console.
 *
 * `awaiting_reply` is the state that is terminal and outstanding at once, so it
 * is kept in `agentTurns` — every other terminal state leaves, which is what
 * makes `list_agent_turns` here agree with the backend for a rail that reads it
 * after the event rather than before.
 *
 *   window.__mockSetTurnState("turn-1", "awaiting_reply")
 */
(globalThis as Record<string, unknown>).__mockSetTurnState = (
  turnId: string,
  state: string,
  failure: string | null = null,
): any => {
  const turn = agentTurns.find((t) => t.id === turnId);
  if (!turn) throw new Error(`no such turn: ${turnId}`);
  turn.state = state;
  turn.failure = failure;
  turn.endedAt = state === "running" ? null : new Date().toISOString();
  // AGC-FR-34: a terminating turn leaves no active call behind.
  if (state !== "running") turn.activeToolCalls = [];
  if (state !== "running" && state !== "awaiting_reply") {
    agentTurns = agentTurns.filter((t) => t.id !== turnId);
  }
  fireBus("agent-turn-state-changed", { ...turn });
  return { ...turn };
};

/**
 * AGC-FR-33 / CMT-FR-80: begin a tool call in a running turn, so its pending
 * contribution reads that tool's activity status.
 *
 * The activation sequence is handed out here exactly as the registry hands it
 * out — ascending, unique within the turn, and never reused — because CMT-FR-81
 * picks the status from that and never from the order the events arrived.
 *
 *   window.__mockToolCallBegins("turn-1", "openrouter:web_search")
 */
const nextActivationSeq = new Map<string, number>();
(globalThis as Record<string, unknown>).__mockToolCallBegins = (
  turnId: string,
  tool: string,
): any => {
  const turn = agentTurns.find((t) => t.id === turnId);
  if (!turn) throw new Error(`no such turn: ${turnId}`);
  // Held beside the turn rather than on it, so what the event carries is an
  // `AgentTurn` and nothing else — the backend's payload has no such field.
  const seq = (nextActivationSeq.get(turnId) ?? 0) + 1;
  nextActivationSeq.set(turnId, seq);
  const call = { id: `call-${seq}`, tool, activationSeq: seq };
  turn.activeToolCalls = [...(turn.activeToolCalls ?? []), call];
  // AGC-FR-34: the whole turn, still `running`, rather than a difference.
  fireBus("agent-turn-state-changed", { ...turn });
  return call.id;
};

/**
 * AGC-FR-33: a call leaves the list the moment it succeeds, refuses, or is
 * abandoned. Pass the id `__mockToolCallBegins` answered with, or omit it to
 * finish the most recently activated call.
 *
 *   window.__mockToolCallFinishes("turn-1")
 */
(globalThis as Record<string, unknown>).__mockToolCallFinishes = (
  turnId: string,
  callId?: string,
): any => {
  const turn = agentTurns.find((t) => t.id === turnId);
  if (!turn) throw new Error(`no such turn: ${turnId}`);
  const active = (turn.activeToolCalls ?? []) as Array<{
    id: string;
    activationSeq: number;
  }>;
  const target =
    callId ??
    active.reduce(
      (latest, c) => (!latest || c.activationSeq > latest.activationSeq ? c : latest),
      undefined as { id: string; activationSeq: number } | undefined,
    )?.id;
  turn.activeToolCalls = active.filter((c) => c.id !== target);
  fireBus("agent-turn-state-changed", { ...turn });
  return { ...turn };
};

/**
 * AGC-FR-48 / CMT-FR-43: an agent ends its turn by asking the author something.
 *
 * Two events in the order the backend emits them — the question lands in the
 * conversation as an ordinary comment first, then the turn reports
 * `awaiting_reply` — so the rail is never in a state where the pending card has
 * gone and nothing has replaced it.
 *
 *   window.__mockAgentAsks("turn-1", "Should graduation write one spec or two?")
 */
(globalThis as Record<string, unknown>).__mockAgentAsks = (
  turnId: string,
  body: string,
): any => {
  const turn = agentTurns.find((t) => t.id === turnId);
  if (!turn) throw new Error(`no such turn: ${turnId}`);
  const threadId: string = turn.origin.discussionId;
  const posted = {
    id: `${threadId}-q${++appendSeq}`,
    author: { ...agent, agentId: turn.agentId, handle: turn.nickname },
    body,
    quotes: [],
    attachments: [],
    createdAt: new Date().toISOString(),
  };
  const stored = allDiscussions().find((t) => t.id === threadId);
  if (stored) {
    stored.comments = [...stored.comments, posted];
    stored.updatedAt = posted.createdAt;
  } else {
    appendedComments[threadId] = [
      ...(appendedComments[threadId] ?? []),
      posted,
    ];
  }
  fireBus("comment-thread-changed", foldedThread(threadId));
  return (
    globalThis as unknown as {
      __mockSetTurnState: (id: string, s: string) => any;
    }
  ).__mockSetTurnState(turnId, "awaiting_reply");
};

/**
 * AGC-FR-48 / CTA-FR-ZOLW: an agent ends its turn with an answer.
 *
 * The same two events in the same order as `__mockAgentAsks` — the answer lands
 * as an ordinary comment first, then the turn reports `delivered` — so the
 * pending row is replaced by the answer and nothing stands between them.
 *
 *   window.__mockAgentAnswers("turn-1", "Two specs: one per surface.")
 */
(globalThis as Record<string, unknown>).__mockAgentAnswers = (
  turnId: string,
  body: string,
): any => {
  const turn = agentTurns.find((t) => t.id === turnId);
  if (!turn) throw new Error(`no such turn: ${turnId}`);
  const threadId: string = turn.origin.discussionId;
  const posted = {
    id: `${threadId}-r${++appendSeq}`,
    author: { ...agent, agentId: turn.agentId, handle: turn.nickname },
    body,
    quotes: [],
    attachments: [],
    createdAt: new Date().toISOString(),
  };
  const stored = allDiscussions().find((t) => t.id === threadId);
  if (stored) {
    stored.comments = [...stored.comments, posted];
    stored.updatedAt = posted.createdAt;
  } else {
    appendedComments[threadId] = [
      ...(appendedComments[threadId] ?? []),
      posted,
    ];
  }
  fireBus("comment-thread-changed", foldedThread(threadId));
  return (
    globalThis as unknown as {
      __mockSetTurnState: (id: string, s: string) => any;
    }
  ).__mockSetTurnState(turnId, "delivered");
};

/** The running turns of one discussion, newest first — for a browser check. */
(globalThis as Record<string, unknown>).__mockRunningTurns = (
  discussionId: string,
): any[] =>
  agentTurns.filter(
    (t) => t.state === "running" && t.origin?.discussionId === discussionId,
  );

const OLD_TEXT = `# Library panel

The Library is the project's artifact browser.

## Requirements

- **LIB-FR-01** — the panel lists artifacts.
- **LIB-FR-02** — the tree is derived from the scan.
`;

const NEW_TEXT = `# Library panel

The Library is the project's artifact browser. It renders the scanned tree.

## Requirements

- **LIB-FR-01** — the panel lists artifacts grouped by folder.
- **LIB-FR-02** — the tree is derived from the filesystem scan.
- **LIB-FR-12** — an "All files" lens shows unclassified files too.
`;

/**
 * DFV-FR-57: the *previous* revision of a source file, so a Diff tab opened on
 * a non-Markdown path has a language on both sides of the comparison rather
 * than Markdown prose facing source. Keyed by extension, like `SOURCE_BODIES`,
 * and deliberately a near-miss of it — same file, a few lines earlier — so the
 * hunks the viewer builds are small, realistic, and land on lines a grammar
 * recognises.
 */
const SOURCE_OLD_BODIES: Record<string, string> = {
  ts: `import { invoke } from "@tauri-apps/api/core";

export interface Artifact {
  id: string;
  name: string;
}

/** Load one artifact's contents. */
export async function load(id: string): Promise<string> {
  const res = await invoke<{ body: string }>("load_artifact_contents", { id });
  return res.body;
}
`,
  rs: `use std::collections::HashMap;

/// Resolve a project path.
pub fn resolve(root: &str, path: &str) -> String {
    let seen: HashMap<&str, usize> = HashMap::new();
    format!("{root}/{path}")
}
`,
};

/** The extension of a path, lowercased, or `""` when it has none. */
function extOf(path: string): string {
  return (path.split("/").pop()?.split(".").pop() ?? "").toLowerCase();
}

const diffPayload = {
  isBinary: false,
  hunks: [
    {
      header: "@@ -1,7 +1,8 @@",
      lines: [
        { kind: "context", oldLineno: 1, newLineno: 1, content: "# Library panel" },
        { kind: "context", oldLineno: 2, newLineno: 2, content: "" },
        {
          kind: "del",
          oldLineno: 3,
          content: "The Library is the project's artifact browser.",
        },
        {
          kind: "add",
          newLineno: 3,
          content: "The Library is the project's artifact browser. It renders the scanned tree.",
        },
        { kind: "context", oldLineno: 4, newLineno: 4, content: "" },
        { kind: "context", oldLineno: 5, newLineno: 5, content: "## Requirements" },
        { kind: "context", oldLineno: 6, newLineno: 6, content: "" },
        { kind: "del", oldLineno: 7, content: "- **LIB-FR-01** — the panel lists artifacts." },
        {
          kind: "add",
          newLineno: 7,
          content: "- **LIB-FR-01** — the panel lists artifacts grouped by folder.",
        },
        {
          kind: "add",
          newLineno: 8,
          content: '- **LIB-FR-12** — an "All files" lens shows unclassified files too.',
        },
      ],
    },
    // A second hunk deep in the file, so the gutter carries three- and
    // four-digit line numbers as a real diff on a long document does — which is
    // what exercises DFV's fixed-width gutter (DFV-FR-37) at a large Source
    // size.
    {
      header: "@@ -998,4 +999,5 @@",
      lines: [
        { kind: "context", oldLineno: 998, newLineno: 999, content: "## Test scenarios" },
        { kind: "context", oldLineno: 999, newLineno: 1000, content: "" },
        {
          kind: "del",
          oldLineno: 1000,
          content: "1. **LIB-FR-01** — the tree renders grouped by folder.",
        },
        {
          kind: "add",
          newLineno: 1001,
          content: "1. **LIB-FR-01** — the tree renders grouped by folder, pinned first.",
        },
        {
          kind: "add",
          newLineno: 1002,
          content: "2. **LIB-FR-07** — the All files lens lists unclassified files.",
        },
        { kind: "context", oldLineno: 1001, newLineno: 1003, content: "" },
      ],
    },
  ],
};

let prefs: Record<string, unknown> = {
  theme: "light",
  // GSS-FR-25 / CHG-FR-34: the Changes panel's footer action, which the
  // graduation start preflight opens its own control on and never writes back
  //. Seeded so both branches are reachable in one step:
  // `?commitAction=push` opens the preflight on a Push it cannot perform.
  changesCommitAction: harnessValue("commitAction") ?? "commit",
};

/**
 * The operating system's disposition toward the application
 * (`../core/NTD-notification-delivery.md` NTD-FR-02). Granted by default so the
 * Notifications section's ordinary shape — no permission control, an enabled
 * rehearsal — is what the harness shows first. `?permission=denied`,
 * `?permission=not_requested`, and `?permission=unsupported` select the other
 * three so each branch of GLS-FR-26 is reachable without editing this file.
 */
let notificationPermission: string =
  new URLSearchParams(location.search).get("permission") ?? "granted";

/**
 * Notifications this session posted and has not withdrawn, keyed as NTD-FR-06
 * keys them so a repeat post replaces in place rather than stacking. Exposed as
 * `window.__notificationCentre` because there is no OS centre behind the
 * harness to look in — this stands in for looking at it.
 */
const notificationCentre = new Map<
  string,
  { id: string; key: string; title: string; body: string; payload: string }
>();
(globalThis as Record<string, unknown>).__notificationCentre =
  notificationCentre;
let notificationSeq = 0;
let layout: Record<string, unknown> | null = null;
// PSS-FR-21: the project-public store, including the optional draft template.
// `draftTemplate: null` is the UNSET state, which is what `create_draft` reads
// as "create the prompt empty" (DRS-FR-06 / DRS-FR-39).
// PSS-FR-JRWC / PSS-FR-TQMV: the graduation concurrency limit and the
// execution time limit, which the Graduation settings section reads and writes.
let projectConfig: {
  lineEndings: string;
  draftTemplate: string | null;
  graduationConcurrencyLimit: number | "unlimited";
  executionTimeoutMs: number | null;
} = {
  lineEndings: "lf",
  draftTemplate: null,
  // PSS-FR-JRWC: one is the resting limit. Seed another at
  // http://localhost:5199/?graduationLimit=3 or ?graduationLimit=unlimited.
  graduationConcurrencyLimit: (() => {
    const seeded =
      typeof location === "undefined"
        ? null
        : new URLSearchParams(location.search).get("graduationLimit");
    if (seeded === "unlimited") return "unlimited" as const;
    const n = Number(seeded);
    return seeded !== null && Number.isInteger(n) && n >= 1 ? n : 1;
  })(),
  executionTimeoutMs: null,
};
let searchSeq = 0;
/**
 * SET-FR-QKKQ: make `save_project_config` fail, so the Graduation section's
 * save-failure state is reachable in the browser. Off by default.
 *
 *   http://localhost:5199/?projectConfigSaveRefused=disk%20full
 *   window.__mockProjectConfigSaveRefusal = "disk full"
 */
let projectConfigSaveRefusal: string | null = harnessValue("projectConfigSaveRefused");
Object.defineProperty(globalThis, "__mockProjectConfigSaveRefusal", {
  get: () => projectConfigSaveRefusal,
  set: (message: string | null) => {
    projectConfigSaveRefusal = message || null;
  },
  configurable: true,
});

const searchHits = [
  {
    id: "specifications/ui/LIB-library.md",
    name: "LIB-library.md",
    path: "specifications/ui/LIB-library.md",
    ordinal: 1,
    group: "artifact",
    matchKind: "name",
    subtype: "spec",
    editContext: "standalone",
  },
  {
    id: "specifications/core/PST-project-storage.md",
    name: "PST-project-storage.md",
    path: "specifications/core/PST-project-storage.md",
    ordinal: 2,
    group: "artifact",
    matchKind: "content",
    subtype: "spec",
    editContext: "standalone",
    line: 42,
    snippet: "the Library panel publishes the tree it already loaded",
  },
  {
    id: "src/main.rs",
    name: "main.rs",
    path: "src/main.rs",
    ordinal: 3,
    group: "file",
    matchKind: "content",
    line: 8,
    snippet: "  synthesis_lib::run()",
  },
];

/* --- Graduation (GRD-graduation.md / GRU / GRV) --------------------------- */

/**
 * The project's graduation queue, in enqueue order (GRD-FR-VLFO).
 *
 * Seeded so every surface the feature adds is reachable without driving a run:
 * `g-1` is working with a two-path manifest, `g-2` is an **in-place** run
 * waiting behind it (so GRU-FR-06's bordered warning renders), `g-3` holds a
 * validated change set to review, and `g-4` is approved and waiting
 * on a publication choice. One draft — `d-4` — is deliberately left
 * out of the queue, so the New Artifact tab's **Graduate** action is reachable
 * on it (NAW-FR-17).
 *
 * **The array order is the enqueue order** `list_graduation_queue` answers with
 * (GRD-FR-VLFO), which is what the rail reverses to draw newest first (GRH-FR-03)
 * and what a queued run's position is counted from (GRU-FR-28). The rail
 * therefore reads, top to bottom, `g-5` … `g-1`, and the leading marker sits on
 * `g-1` at its **foot** — the first non-terminal run in enqueue order, which is
 * exactly the case GRH-FR-03 calls out. The `enqueuedAt` values are
 * illustrative dates rather than a second statement of that order; nothing here
 * renders one, and the array is the only order any surface reads.
 */
function gitSource(name: string) {
  return {
    kind: "git",
    sourceWorktreePath: "/Users/demo/dev/acme",
    sourceBranch: "main",
    sourceRevision: "a91bc04",
    graduationBranch: `synthesis/graduation/${name}`,
    graduationWorktreePath: `/Users/demo/.synthesis/graduations/${name}`,
  };
}

/** GRD-FR-73: the isolated environment the implementation part works in. */
function implementationSource(id: string) {
  return {
    kind: "git",
    // GRD-FR-94: the commit that holds the published specification, and never
    // the source worktree's later HEAD.
    implementationBaseRevision: SPECIFICATION_REVISION,
    implementationBranch: `synthesis/implementation/${id}`,
    implementationWorktreePath: `/Users/demo/.synthesis/implementations/${id}`,
  };
}

/** GRD-FR-94: the one commit an implementation of the seeded runs is built on. */
const SPECIFICATION_REVISION = "5eec1f1";

/** GRD-FR-75: one path of the implementation change set. */
function implementationEntry(path: string, operation: string) {
  return {
    path,
    operation,
    baselineRevision: operation === "created" ? null : SPECIFICATION_REVISION,
    classification: null,
    reason: null,
    failures: [],
  };
}

function manifestEntry(
  path: string,
  operation: string,
  reason: string,
  failures: any[] = [],
) {
  return {
    path,
    operation,
    baselineRevision: operation === "created" ? null : "a91bc04",
    classification: operation === "created" ? "new specification" : "amendment",
    reason,
    failures,
  };
}

/**
 * GRD-FR-08 / GRD-FR-64: the worktree the start preflight reads, and the
 * complete unfiltered set it reports for it — one of every shape GRD-FR-64
 * names, so a browser pass can read both sides of the index told apart, a
 * rename's two locations, and an untracked path.
 */
const GRADUATION_SOURCE_WORKTREE = "/Users/demo/dev/acme";

const UNCOMMITTED_PATHS = [
  { path: "specifications/ui/CHG-changes.md", stagedStatus: "modified", unstagedStatus: null },
  { path: "src/components/Changes.tsx", stagedStatus: "modified", unstagedStatus: "modified" },
  { path: "src/state/appPreferences.ts", stagedStatus: null, unstagedStatus: "modified" },
  { path: "docs/retired-note.md", stagedStatus: null, unstagedStatus: "deleted" },
  {
    path: "specifications/ui/GRV-graduation-review.md",
    stagedStatus: "renamed",
    unstagedStatus: null,
    previousPath: "specifications/ui/GRV-review.md",
  },
  { path: "scratch/notes.txt", stagedStatus: null, unstagedStatus: "untracked" },
];

/**
 * GRV-FR-36: whether the commit that answers the preflight has been made, which
 * is what the next `start_graduation` finds the worktree clean after.
 */
let graduationWorktreeCommitted = false;

const graduationRuns: any[] = [
  {
    id: "g-1",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-1",
    mode: "git",
    state: "running",
    // GRD-FR-69: the one queue this run is assigned to now.
    queue: "graduation",
    input: {
      draftId: "d-1",
      draftName: "Worktree cleanup command",
      prompt: "A command that removes worktrees whose branch is gone.",
      promptChecksum: "sha-p1",
      capturedAt: "2026-08-14T09:00:00Z",
    },
    source: gitSource("worktree-cleanup-command"),
    // GRU-FR-32 / GRU-FR-43 / GRU-FR-44: a run that has been round many times,
    // so the iteration history has enough rows to scroll within itself and to
    // hold both kinds of account — the ones the loop wrote and the one the
    // author's own request for changes left behind.
    iteration: 9,
    manifest: {
      computedAt: "2026-08-14T09:12:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/WTC-worktree-context.md",
          "updated",
          "The prompt asks for cleanup of worktrees whose branch is gone.",
        ),
        manifestEntry(
          "specifications/ui/WTS-worktree-selector.md",
          "created",
          "The prompt asks for the command to be offered in the selector.",
        ),
      ],
      passed: true,
      summary: "Two specification paths.",
    },
    escalation: null,
    lastFailure: null,
    handoffPrompt: null,
    publication: null,
    enqueuedAt: "2026-08-14T09:00:00Z",
    updatedAt: "2026-08-14T09:12:00Z",
  },
  {
    id: "g-2",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-3",
    mode: "in_place",
    state: "queued",
    // GSD-FR-WQPD: the run that makes `docs-refresh` occupied while no run
    // holds it, and leaves `editor-work` free.
    streamId: "s-4",
    streamName: "docs-refresh",
    // GRD-FR-69: the one queue this run is assigned to now.
    queue: "graduation",
    input: {
      draftId: "d-3",
      draftName: "Registry migration sketch",
      prompt: "How the registry moves off the flat file.",
      promptChecksum: "sha-p3",
      capturedAt: "2026-08-14T09:05:00Z",
    },
    // GRU-FR-06: the mode with no undo, so the warning block is reachable.
    source: { kind: "in_place", projectDirectory: "/Users/demo/dev/acme" },
    iteration: 0,
    manifest: null,
    escalation: null,
    lastFailure: null,
    handoffPrompt: null,
    publication: null,
    enqueuedAt: "2026-08-14T09:05:00Z",
    updatedAt: "2026-08-14T09:05:00Z",
  },
  {
    id: "g-3",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-2",
    mode: "git",
    state: "awaiting_review",
    // GRD-FR-69: the one queue this run is assigned to now.
    queue: "graduation",
    input: {
      draftId: "d-2",
      draftName: "Comments export",
      prompt: "Exporting a comment thread as Markdown.",
      promptChecksum: "sha-p2",
      capturedAt: "2026-08-13T16:00:00Z",
    },
    source: gitSource("comments-export"),
    iteration: 2,
    manifest: {
      computedAt: "2026-08-13T16:40:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/CMS-comments.md",
          "updated",
          "The prompt asks for the export to be part of the comment store.",
        ),
        manifestEntry(
          "specifications/ui/CEX-comment-export.md",
          "created",
          "The prompt asks for a surface the author exports a thread from.",
        ),
        manifestEntry(
          "specifications/tools/EXT-export-thread-tool.md",
          "created",
          "The prompt names the tool an agent uses to export a thread.",
        ),
        manifestEntry(
          "specifications/ui/CMT-comment-threads-and-a-very-long-file-name-that-tests-the-rail.md",
          "updated",
          "The prompt asks for the thread list to offer the export.",
        ),
      ],
      passed: true,
      summary: "Four specification paths, all justified by the prompt.",
    },
    escalation: null,
    lastFailure: null,
    // GRD-FR-37 /: the run in `awaiting_review` is the only one whose
    // review surface opens, so it is the one that has to carry the prompt the
    // loop's hand-off phase composed. The review renders this text in its
    // hand-off region; a run that composed none keeps `handoffPrompt: null`,
    // which the other runs here still show.
    handoffPrompt:
      "Implement the comment export described in " +
      "specifications/ui/CEX-comment-export.md (CEX-FR-01 through CEX-FR-12) " +
      "and specifications/core/CMS-comments.md (CMS-FR-40, extended). Both " +
      "specs are the source of truth; read them before writing anything.",
    publication: null,
    enqueuedAt: "2026-08-13T16:00:00Z",
    updatedAt: "2026-08-13T16:40:00Z",
  },
  {
    id: "g-4",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-5",
    mode: "git",
    state: "awaiting_publication_choice",
    // GRD-FR-69: the one queue this run is assigned to now.
    queue: "graduation",
    input: {
      draftId: "d-5",
      draftName: "Search ranking rules",
      prompt: "How search results are ranked and grouped.",
      promptChecksum: "sha-p5",
      capturedAt: "2026-08-12T11:00:00Z",
    },
    source: gitSource("search-ranking-rules"),
    iteration: 1,
    manifest: {
      computedAt: "2026-08-12T11:30:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/SRR-search-ranking.md",
          "created",
          "The prompt asks for the ranking rules to be written down.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    escalation: null,
    lastFailure: null,
    handoffPrompt: null,
    publication: null,
    enqueuedAt: "2026-08-12T11:00:00Z",
    updatedAt: "2026-08-12T11:30:00Z",
  },
  /**
   *: the run that graduated `d-6`, whose draft status is `graduated`.
   * A terminal run is what `get_draft_graduation` must answer with for the
   * draft's tab to carry the hand-off affordance at all, and the record is kept
   * for the project's life however long ago the run finished.
   *
   * The prompt is long on purpose: NAW-FR-47 makes the surface scroll the text
   * within itself, so a short one would never show whether Copy stays on the
   * surface.
   */
  {
    id: "g-5",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-6",
    mode: "git",
    input: {
      draftId: "d-6",
      draftName: "Notification address rules",
      prompt: "Which address a notification is sent to, and when.",
      promptChecksum: "sha-p6",
      capturedAt: "2026-08-09T10:00:00Z",
    },
    source: gitSource("notification-address-rules"),
    // GRD-FR-71: a specification published as changes nobody committed does
    // not enqueue an implementation. The run holds neither queue and waits for
    // the author's explicit **Implement** (GRU-FR-LBPR).
    state: "awaiting_implementation",
    queue: null,
    specificationRevision: null,
    iteration: 2,
    manifest: {
      computedAt: "2026-08-10T07:50:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/NAR-notification-addresses.md",
          "created",
          "The prompt asks for the address rules to be written down.",
        ),
        manifestEntry(
          "specifications/ui/NTF-notifications.md",
          "updated",
          "The prompt asks for the notification surface to name the address.",
        ),
      ],
      passed: true,
      summary: "Two specification paths.",
    },
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the notification address rules described in " +
      "specifications/core/NAR-notification-addresses.md (NAR-FR-01 through " +
      "NAR-FR-18) and the address the notification surface names in " +
      "specifications/ui/NTF-notifications.md (NTF-FR-31, extended). Both " +
      "specifications are the source of truth; read them, and every " +
      "specification they name, before writing anything.\n\n" +
      "Read first:\n" +
      "  - specifications/core/NAR-notification-addresses.md — the rules " +
      "themselves, and the order they resolve in.\n" +
      "  - specifications/ui/NTF-notifications.md — where the resolved " +
      "address is shown, and what it says when no address resolves.\n" +
      "  - specifications/core/PRF-preferences.md — the record the account " +
      "address is held in.\n\n" +
      "What to build, in order:\n" +
      "  1. The resolution itself, in the core, with the fallback chain the " +
      "specification sets out and no surface knowledge in it at all.\n" +
      "  2. The command the frontend reads a resolved address through, " +
      "registered in the invoke handler.\n" +
      "  3. The notification surface's own reading of it, which names the " +
      "address it will send to and stays legible when none resolves.\n\n" +
      "A line long enough that the hand-off surface is checked for horizontal " +
      "overflow rather than assumed to soft-wrap: the resolution order is " +
      "account address, then project address, then the address the invitation " +
      "was accepted at, then nothing at all — and nothing at all is an " +
      "ordinary outcome rather than a failure.\n\n" +
      "Definition of done: `pnpm test` green, `pnpm build` green, " +
      "`cargo test` green, and `cargo check` clean.",
    publication: {
      // GRD-FR-71: the uncommitted publication, which is the one that leaves a
      // run in `awaiting_implementation`.
      publication: { kind: "uncommitted" },
      appliedPaths: [
        "specifications/core/NAR-notification-addresses.md",
        "specifications/ui/NTF-notifications.md",
      ],
      commitId: null,
    },
    enqueuedAt: "2026-08-09T10:00:00Z",
    updatedAt: "2026-08-10T08:00:00Z",
  },
  /**
   * GRU-FR-13 / GRU-FR-37 through GRU-FR-42: the run resting on a
   * **multi-question** escalation, which is the only way the paginated question
   * area, the answered indication, the unsent answer draft, and the gating of
   * **Send answers** are reachable at all.
   *
   * Seeded to GEA-FR-SXXB, GRU-FR-XQVG, GEA-FR-TEMG's shape: three recorded questions, the first with two
   * proposed responses, the second with three, the third with none, so the
   * radio group, its widest form, and the free-text field standing alone are
   * each on screen without driving a run. The positions are 1, 2, 3 and are
   * what every answer names (ESU-FR-16); the `description` of each response is
   * display-only and is submitted nowhere (GRU-FR-13).
   *
   * Placed last, so the rail — which draws newest first (GRH-FR-03) — shows it
   * at the head and it is one click from the section opening. The leading
   * marker still sits on `g-1`, the first non-terminal run in enqueue order.
   */
  {
    id: "g-6",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-7",
    mode: "git",
    state: "awaiting_user_decision",
    // GRD-FR-69: the one queue this run is assigned to now.
    queue: "graduation",
    input: {
      draftId: "d-7",
      draftName: "Stale branch pruning",
      prompt: "A command that prunes branches whose upstream is gone.",
      promptChecksum: "sha-p7",
      capturedAt: "2026-08-15T09:00:00Z",
    },
    source: gitSource("stale-branch-pruning"),
    iteration: 2,
    manifest: null,
    escalation: {
      // GXD-FR-HGSU: the authoring route, which is the one that records a
      // clarification; the surface renders identically either way.
      origin: "cli_clarification",
      reason:
        "The prompt asks for stale branches to be pruned but never says what " +
        "counts as stale, nor what the command does with a branch that still " +
        "has work on it.",
      questions: [
        {
          position: 1,
          question:
            "What should the command do with a branch whose upstream is gone but which still holds unmerged commits?",
          options: [
            {
              answer: "skip",
              summary: "Skip the branch",
              description: "It is reported and left alone.",
            },
            {
              answer: "prune",
              summary: "Prune it anyway",
              description: "The commits are lost with it.",
            },
          ],
        },
        {
          position: 2,
          question: "Where should the rule for staleness be written down?",
          options: [
            {
              answer: "ui",
              summary: "In the UI spec",
              description: "It is a surface rule.",
            },
            {
              answer: "core",
              summary: "In the core spec",
              description: "It is a backend rule.",
            },
            {
              answer: "both",
              summary: "In both",
              description: "Each states its own half of it.",
            },
          ],
        },
        {
          position: 3,
          question:
            "Is the command wanted at all, or is the existing worktree cleanup enough?",
          options: [],
        },
      ],
      raisedAt: "2026-08-15T12:20:00Z",
    },
    lastFailure: null,
    handoffPrompt: null,
    publication: null,
    enqueuedAt: "2026-08-15T09:00:00Z",
    updatedAt: "2026-08-15T12:20:00Z",
  },
  /**
   * GRU-FR-45 … GRU-FR-55: the four runs that stand in the **implementation**
   * part, one per state whose surface differs.
   *
   * They are runs of the same eight-phase record rather than runs of their own
   * (GRU-FR-45): each carries the `queue` it is assigned to, the
   * `specificationRevision` its isolation is built from, and the
   * `implementationSource` that isolation is. `g-5` above completes the set
   * from the other side — the one run in `awaiting_implementation`, holding
   * neither queue and waiting on the author's explicit **Implement**.
   *
   * `g-7` is working with an implementation change set and a Review pass that
   * sent it back with one finding of each severity (GRU-FR-49, GRU-FR-50);
   * `g-8` is blocked on a dirty source worktree (GRU-FR-48); `g-9` has passed
   * Review and is waiting on the implementation publication choice
   * (GRU-FR-51); `g-10` is `implemented`, so both publications' paths are on
   * screen (GRH-FR-18).
   */
  {
    id: "g-7",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-8",
    mode: "git",
    state: "implementing",
    queue: "implementation",
    input: {
      draftId: "d-8",
      draftName: "Panel row virtualization",
      prompt: "Only the rows in view are rendered, and the rail keeps its place.",
      promptChecksum: "sha-p8",
      capturedAt: "2026-08-17T10:00:00Z",
    },
    source: gitSource("panel-row-virtualization"),
    iteration: 2,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 2,
    implementationSource: implementationSource("g-7"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-17T12:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/ui/PRV-panel-row-virtualization.md",
          "created",
          "The prompt asks for the rows in view to be the ones rendered.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    // GRU-FR-46: the current implementation change set, and no verdict — a
    // Review that has not returned has judged nothing.
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-18T09:40:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src/components/VirtualRows.tsx", "created"),
        implementationEntry("src/components/DraftsPanel.tsx", "updated"),
        implementationEntry("src/state/rowWindow.ts", "created"),
        implementationEntry("src/styles/panels.css", "updated"),
      ],
      passed: true,
      summary: "Two created, two updated.",
    },
    // GRU-FR-49 / GRU-FR-50: every finding of the run's Review passes, whole
    // and in the order the record holds them. One of each severity, one of
    // them repository-wide.
    reviewHistory: [
      {
        iteration: 1,
        at: "2026-08-18T09:20:00Z",
        verdict: {
          verdict: "revise",
          rationale:
            "Two requirements of the specification are not delivered, and the rail loses its place on a re-render.",
          findings: [
            {
              severity: "critical",
              description:
                "The row window is computed from the scroll offset before the container has been measured, so the first paint renders no rows at all.",
              affectedFiles: [
                "src/components/VirtualRows.tsx",
                "src/state/rowWindow.ts",
              ],
              correction:
                "Measure the container first, and render the window only once a height is known.",
            },
            {
              severity: "major",
              description:
                "The rail's selected row is not kept in view when the window moves, which PRV-FR-09 requires.",
              affectedFiles: ["src/components/DraftsPanel.tsx"],
              correction:
                "Scroll the selected row into view whenever the window changes, and cover it with a test.",
            },
            {
              severity: "minor",
              description:
                "Two helpers compute the same row height from the same token.",
              affectedFiles: [],
              correction: "Keep one helper and call it from both places.",
            },
          ],
        },
        correction:
          "Measure the container before computing the window, keep the selected row in view, and keep one row-height helper.",
      },
      /**
       * GRU-FR-63: the Review of the **newest** pass, which sent it back. It
       * started no further pass yet, so it stands in that pass's account as
       * what became of it — the "sent this pass back" wording, beside the
       * "passed this pass" one `g-11` carries.
       */
      {
        iteration: 2,
        at: "2026-08-18T14:10:00Z",
        verdict: {
          verdict: "revise",
          rationale:
            "The first paint now renders rows, but the selected row is still lost when the window moves, and the new measurement runs on every scroll frame.",
          findings: [
            {
              severity: "major",
              description:
                "The container is measured inside the scroll handler, so every frame of a fast scroll forces a synchronous layout of the whole panel and the rail stutters on a list of any length.",
              affectedFiles: [
                "src/components/VirtualRows.tsx",
                "src/state/rowWindow.ts",
              ],
              correction:
                "Measure the container once with a resize observer and read the cached height in the scroll handler, and cover it with a test that scrolls twice and asserts one measurement.",
            },
            {
              severity: "minor",
              description:
                "The selected row is scrolled into view with the default alignment, so a selection just below the window jumps it to the top rather than bringing the row to the nearest edge.",
              affectedFiles: ["src/components/DraftsPanel.tsx"],
              correction:
                "Scroll the selected row to the nearest edge rather than to the top.",
            },
          ],
        },
        correction:
          "Measure the container with a resize observer rather than in the scroll handler, and scroll the selected row to the nearest edge.",
      },
    ],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the row virtualization described in " +
      "specifications/ui/PRV-panel-row-virtualization.md. The specification is " +
      "the source of truth; read it before writing anything.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/ui/PRV-panel-row-virtualization.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-17T10:00:00Z",
    updatedAt: "2026-08-18T09:40:00Z",
  },
  {
    id: "g-8",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-9",
    mode: "git",
    state: "implementation_blocked",
    queue: "implementation",
    input: {
      draftId: "d-9",
      draftName: "Token rotation",
      prompt: "A stored token is rotated before it expires, and never twice at once.",
      promptChecksum: "sha-p9",
      capturedAt: "2026-08-18T09:00:00Z",
    },
    source: gitSource("token-rotation"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 0,
    implementationSource: null,
    manifest: {
      kind: "specification",
      computedAt: "2026-08-18T10:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/TKR-token-rotation.md",
          "created",
          "The prompt asks for the rotation rules to be written down.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    // GRU-FR-48: what is blocking it, that nothing was published, and the act
    // that clears it — the complete uncommitted set the preflight reported.
    implementationBlocker: {
      code: "source_worktree_dirty",
      message:
        "Commit or set aside the uncommitted work in the source worktree, then continue.",
      uncommitted: {
        sourceWorktreePath: GRADUATION_SOURCE_WORKTREE,
        paths: [
          {
            path: "src/components/TokenPicker.tsx",
            stagedStatus: null,
            unstagedStatus: "modified",
          },
          {
            path: "src-tauri/src/github/tokens.rs",
            stagedStatus: "modified",
            unstagedStatus: null,
          },
          {
            path: "scratch/rotation-notes.txt",
            stagedStatus: null,
            unstagedStatus: "untracked",
          },
        ],
      },
    },
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the token rotation described in " +
      "specifications/core/TKR-token-rotation.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/core/TKR-token-rotation.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-18T09:00:00Z",
    updatedAt: "2026-08-19T09:00:00Z",
  },
  /**
   * GRU-FR-48 / GRD-FR-97: the run **no turn could run the project's checks
   * for**. Three implementation turns in a row met an unreachable toolchain
   * and each filed its own account of it, so the surface states which command
   * was unreachable rather than that an implementation stopped.
   *
   * The messages are written as a model writes them — several lines, a command
   * line among them, and a path long enough that a box which does not wrap
   * would widen the whole panel. Two of the three name one code and the third
   * names another, so the sentence above the list counts three turns and names
   * two distinct codes.
   */
  {
    id: "g-12",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-13",
    mode: "git",
    state: "implementation_blocked",
    queue: "implementation",
    input: {
      draftId: "d-13",
      draftName: "Log rotation",
      prompt:
        "A log file is rotated once it passes its size, and the oldest is dropped.",
      promptChecksum: "sha-p13",
      capturedAt: "2026-08-23T08:00:00Z",
    },
    source: gitSource("log-rotation"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 3,
    implementationSource: implementationSource("g-12"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-23T09:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/infra/LGR-log-rotation.md",
          "created",
          "The prompt asks for the rotation rules to be written down.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-23T10:55:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src-tauri/src/logging/rotation.rs", "created"),
      ],
      passed: true,
      summary: "One created.",
    },
    implementationBlocker: {
      code: "toolchain_unavailable",
      message:
        "Make the toolchain reachable where the turns run, then Continue to try " +
        "the work again.",
      reports: [
        {
          iteration: 1,
          code: "cargo_not_found",
          message:
            "I cannot run the project's checks in this worktree.\n\n" +
            "`cargo test` exits immediately with `command not found: cargo`, and " +
            "`rustup` is not on the path either, so I cannot install the toolchain " +
            "the way `src-tauri/rust-toolchain.toml` pins it.\n\n" +
            "I looked for it at " +
            "/Users/demo/.cargo/bin/cargo and at " +
            "/Users/demo/Library/Caches/synthesis/worktrees/g-12/implementation/.cargo/bin/cargo, " +
            "and neither exists. Without the checks I cannot tell whether the " +
            "rotation I wrote compiles, so I have written nothing further and " +
            "stopped here rather than reporting work I could not verify.",
        },
        {
          iteration: 2,
          code: "cargo_not_found",
          message:
            "Same obstacle as the turn before me: `cargo` is still not on the path.\n\n" +
            "I retried the check the way the project documents it — " +
            "`cd src-tauri && cargo check` — and the shell answered " +
            "`zsh: command not found: cargo` again. I also tried " +
            "`$HOME/.cargo/env`, which does not exist, and " +
            "`/opt/homebrew/bin/rustup`, which does not exist either.\n\n" +
            "Nothing about the worktree changed between the two turns, so I do " +
            "not think retrying a third time will answer differently. This needs " +
            "the toolchain installed on the machine rather than a further pass.",
        },
      ],
    },
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the log rotation described in " +
      "specifications/infra/LGR-log-rotation.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/infra/LGR-log-rotation.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-23T08:00:00Z",
    updatedAt: "2026-08-23T11:00:00Z",
  },
  {
    id: "g-9",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-10",
    mode: "git",
    state: "awaiting_implementation_publication_choice",
    queue: "implementation",
    input: {
      draftId: "d-10",
      draftName: "Diff gutter marks",
      prompt: "The gutter marks which lines changed, and says so in words too.",
      promptChecksum: "sha-p10",
      capturedAt: "2026-08-19T14:00:00Z",
    },
    source: gitSource("diff-gutter-marks"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 3,
    implementationSource: implementationSource("g-9"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-19T15:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/ui/DGM-diff-gutter-marks.md",
          "created",
          "The prompt asks for the gutter to say what it marks.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-20T13:50:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src/diff/gutter.tsx", "created"),
        implementationEntry("src/diff/marks.ts", "created"),
        implementationEntry("src/diff/Unified.tsx", "updated"),
        implementationEntry("src/diff/SideBySide.tsx", "updated"),
        implementationEntry("src/diff/Final.tsx", "updated"),
        implementationEntry("src/styles/diff.css", "updated"),
        implementationEntry("src/diff/gutter.test.tsx", "created"),
        implementationEntry("src/diff/marks.test.ts", "created"),
        implementationEntry("src/diff/index.ts", "updated"),
      ],
      passed: true,
      summary: "Five created, four updated.",
    },
    // GRU-FR-50: a `ready` verdict renders as a pass with no findings, together
    // with its rationale.
    reviewHistory: [
      {
        iteration: 1,
        at: "2026-08-20T13:00:00Z",
        verdict: {
          verdict: "revise",
          rationale: "The mark is drawn but never named, so it reads as colour alone.",
          findings: [
            {
              severity: "major",
              description:
                "The gutter mark carries no text, so what it means is on colour alone.",
              affectedFiles: ["src/diff/gutter.tsx"],
              correction:
                "Give every mark a word, and a title an assistive technology reads.",
            },
          ],
        },
        correction: "Give every mark a word.",
      },
      {
        iteration: 2,
        at: "2026-08-20T13:45:00Z",
        verdict: {
          verdict: "ready",
          rationale:
            "Every requirement of DGM is delivered, each mark carries a word, and the tests cover both directions.",
          findings: [],
        },
        correction: null,
      },
    ],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the gutter marks described in " +
      "specifications/ui/DGM-diff-gutter-marks.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/ui/DGM-diff-gutter-marks.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-19T14:00:00Z",
    updatedAt: "2026-08-20T14:00:00Z",
  },
  /**
   * GRU-FR-43 / GRU-FR-49 / GRU-FR-53 / GRU-FR-63: the run that carries a
   * **whole** iteration history of both parts at once, so the account the
   * findings now stand in is read at its heaviest rather than at its lightest.
   *
   * Two specification passes and three implementation passes, with a
   * `revision_history` and a `review_history` that account for the same
   * decisions from the two sides GRD-FR-80 records them on. The newest
   * implementation row therefore holds **both** kinds of account at once — the
   * Review that caused it, and the Review that ended it — which is the tallest
   * an account can be:
   *
   *   - implementation pass 3 — caused by the Review of pass 2, and ended by
   *     the `ready` Review of its own (GRU-FR-63);
   *   - implementation pass 2 — caused by the Review of pass 1, whose three
   *     findings are one of each severity: a critical with **two** affected
   *     paths, a major with one, and a minor with **none**, which is the
   *     repository-wide case GRU-FR-49 asks the surface to say in words;
   *   - implementation pass 1 — the hand-off prompt;
   *   - specification pass 2 — the validation sent pass 1 back;
   *   - specification pass 1 — the captured prompt.
   *
   * The paths and the corrections are long on purpose. A finding whose path
   * fits the panel proves nothing about a finding whose path does not, and the
   * account is where a long path either wraps or widens the panel.
   */
  {
    id: "g-11",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-12",
    mode: "git",
    state: "awaiting_implementation_publication_choice",
    queue: "implementation",
    input: {
      draftId: "d-12",
      draftName: "Attachment thumbnails",
      prompt:
        "A comment attachment shows a thumbnail, and the thumbnail is built once and kept.",
      promptChecksum: "sha-p12",
      capturedAt: "2026-08-21T09:00:00Z",
    },
    source: gitSource("attachment-thumbnails"),
    iteration: 2,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 3,
    implementationSource: implementationSource("g-11"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-21T10:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/ui/ATH-attachment-thumbnails.md",
          "created",
          "The prompt asks for an attachment to show a thumbnail.",
        ),
        manifestEntry(
          "specifications/core/CMS-comments.md",
          "updated",
          "The prompt asks for the thumbnail to be kept beside the attachment.",
        ),
      ],
      passed: true,
      summary: "Two specification paths.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-22T15:30:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry(
          "src/components/comments/attachments/AttachmentThumbnail.tsx",
          "created",
        ),
        implementationEntry("src/state/attachments/thumbnailCache.ts", "created"),
        implementationEntry("src-tauri/src/comments/thumbnails.rs", "created"),
        implementationEntry("src/components/CommentAttachments.tsx", "updated"),
      ],
      passed: true,
      summary: "Three created, one updated.",
    },
    /**
     * GRD-FR-80: the three Reviews, numbered by the implementation part's own
     * count. The first sent pass 1 back with three findings, the second sent
     * pass 2 back with one, and the third passed pass 3.
     */
    reviewHistory: [
      {
        iteration: 1,
        at: "2026-08-22T09:15:00Z",
        verdict: {
          verdict: "revise",
          rationale:
            "The thumbnail is built on every render rather than once, the cache is never bounded, and a failed build is reported as a success.",
          findings: [
            {
              severity: "critical",
              description:
                "The thumbnail is rebuilt on every render of the attachment row, because the cache is keyed by the object identity of the attachment record rather than by its identifier and checksum. A thread holding forty attachments therefore decodes forty images on each keystroke in the comment box, which is what the specification's second requirement exists to prevent.",
              affectedFiles: [
                "src/components/comments/attachments/AttachmentThumbnail.tsx",
                "src/state/attachments/thumbnailCache.ts",
              ],
              correction:
                "Key the cache by the attachment identifier and its content checksum together, so an attachment whose bytes did not change is decoded once for the life of the window, and add a test that renders the same row twice and asserts the decoder was called once.",
            },
            {
              severity: "major",
              description:
                "A thumbnail the backend could not build is written into the cache as an empty record and then read back as a finished thumbnail, so the attachment shows a blank frame instead of saying that no preview could be made.",
              affectedFiles: ["src-tauri/src/comments/thumbnails.rs"],
              correction:
                "Return the refusal as a refusal rather than as an empty thumbnail, keep the failure out of the cache, and let the surface say in words that no preview could be made for this attachment.",
            },
            {
              severity: "minor",
              description:
                "The maximum thumbnail edge is written as a bare number in four places, and the four do not agree with one another or with the number the specification states.",
              affectedFiles: [],
              correction:
                "Declare the maximum edge once, beside the other layout tokens the project already keeps in one place, and read it from there everywhere it is used — including the backend, which currently carries its own copy of it.",
            },
          ],
        },
        correction:
          "Key the thumbnail cache by identifier and checksum, report a failed build as a refusal rather than as an empty thumbnail, and declare the maximum thumbnail edge in one place that both sides read.",
      },
      {
        iteration: 2,
        at: "2026-08-22T13:40:00Z",
        verdict: {
          verdict: "revise",
          rationale:
            "The cache is now correct but unbounded, so a long session holds every thumbnail it ever built.",
          findings: [
            {
              severity: "major",
              description:
                "Nothing ever evicts a thumbnail, so a session that scrolls a year of threads keeps every decoded image alive for as long as the window is open.",
              affectedFiles: ["src/state/attachments/thumbnailCache.ts"],
              correction:
                "Bound the cache and evict the least recently read entry when it is full, and cover the eviction with a test that fills it past the bound.",
            },
          ],
        },
        correction:
          "Bound the thumbnail cache and evict the least recently read entry when it is full.",
      },
      {
        iteration: 3,
        at: "2026-08-22T15:20:00Z",
        verdict: {
          verdict: "ready",
          rationale:
            "Every requirement of ATH is delivered, the cache is keyed and bounded, a failed build says so in words, and both sides read one maximum edge.",
          findings: [],
        },
        correction: null,
      },
    ],
    /**
     * GRD-FR-56: the same decisions from the other side. The two records name
     * one pass each, so the surface reads the Review and the explanation stands
     * behind it (GRU-FR-53).
     */
    seedRevisions: [
      {
        iteration: 1,
        at: "2026-08-21T11:20:00Z",
        rationale:
          "The change set states what a thumbnail is but never says what the surface shows while one is being built, which the prompt asks for.",
        correction:
          "Add a requirement covering the interval before a thumbnail exists, and a scenario for it.",
        cause: "validation",
      },
      {
        iteration: 1,
        at: "2026-08-22T09:15:00Z",
        rationale:
          "The thumbnail is built on every render rather than once, the cache is never bounded, and a failed build is reported as a success.",
        correction:
          "Key the thumbnail cache by identifier and checksum, report a failed build as a refusal rather than as an empty thumbnail, and declare the maximum thumbnail edge in one place that both sides read.",
        cause: "review",
      },
      {
        iteration: 2,
        at: "2026-08-22T13:40:00Z",
        rationale:
          "The cache is now correct but unbounded, so a long session holds every thumbnail it ever built.",
        correction:
          "Bound the thumbnail cache and evict the least recently read entry when it is full.",
        cause: "review",
      },
    ],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the attachment thumbnails described in " +
      "specifications/ui/ATH-attachment-thumbnails.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: [
        "specifications/ui/ATH-attachment-thumbnails.md",
        "specifications/core/CMS-comments.md",
      ],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-21T09:00:00Z",
    updatedAt: "2026-08-22T15:30:00Z",
  },
  /**
   * GRD-FR-113 / GRU-FR-51 / GRU-FR-63 /: the run whose Review **let
   * the work through carrying remarks** — a `revise` verdict that ended the
   * drive, with `endedDrive` true, no correction, and two `minor` findings
   * nobody was asked to act on.
   *
   * The other run in this state (`g-9`) is the unchanged case: a Review that
   * passed with nothing to say. This one is the second variant, so the two
   * statements the publication choice can make are both on screen at once.
   *
   * The first remark's description and its two paths are long on purpose. The
   * remarks render inside the publication **modal** as well as on the row, and
   * a modal is where a long description either wraps or pushes the publish
   * controls off the bottom of the window.
   */
  {
    id: "g-13",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-14",
    mode: "git",
    state: "awaiting_implementation_publication_choice",
    queue: "implementation",
    input: {
      draftId: "d-14",
      draftName: "Search result grouping",
      prompt:
        "Search results group by the file they came from, and a group says how many it holds.",
      promptChecksum: "sha-p14",
      capturedAt: "2026-08-23T09:00:00Z",
    },
    source: gitSource("search-result-grouping"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 2,
    implementationSource: implementationSource("g-13"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-23T10:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/ui/SRG-search-result-grouping.md",
          "created",
          "The prompt asks for results to group by file.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-24T11:30:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry(
          "src/components/search/results/grouping/SearchResultGroupHeader.tsx",
          "created",
        ),
        implementationEntry("src/state/search/grouping.ts", "created"),
        implementationEntry("src/components/SearchResults.tsx", "updated"),
      ],
      passed: true,
      summary: "Two created, one updated.",
    },
    reviewHistory: [
      {
        iteration: 1,
        at: "2026-08-24T09:10:00Z",
        verdict: {
          verdict: "revise",
          rationale:
            "A group states no count, which the specification's second requirement asks for.",
          findings: [
            {
              severity: "major",
              description:
                "A group header names the file but never says how many results it holds.",
              affectedFiles: ["src/components/SearchResults.tsx"],
              correction:
                "Put the count of the group beside the file name, and cover it with a test.",
            },
          ],
        },
        correction: "Give every group header its count.",
      },
      /**
       * GRD-FR-113: the Review that ended the drive. `revise`, but it let the
       * work through — so its findings are advisory remarks, it carries no
       * correction, and `endedDrive` says which of the two it was.
       */
      {
        iteration: 2,
        at: "2026-08-24T11:20:00Z",
        endedDrive: true,
        verdict: {
          verdict: "revise",
          rationale:
            "Every requirement of SRG is delivered. What is left is advisory: nothing here is worth another pass.",
          findings: [
            {
              severity: "minor",
              description:
                "The group header composes its count with a template string in three separate places rather than reading one helper, so a project that later wants the count spelt in words — “one result” rather than “1 result”, which is how every other count on this surface is already spelt — has three edits to make instead of one, and the three are far enough apart in the tree that a reader is unlikely to find all of them from any one of them. Nothing about this is wrong today and no requirement of the specification depends on it, which is why it is a remark rather than a finding that would have started another pass.",
              affectedFiles: [
                "src/components/search/results/grouping/SearchResultGroupHeader.tsx",
                "src/state/search/grouping.ts",
              ],
              // GRD-FR-78: a remark is validated like every other finding, so
              // it still carries the correction text the schema demands. It is
              // the **outcome** that carries none, because no next turn was
              // told anything (GRD-FR-113).
              correction:
                "Read the count from one helper beside the other counts this surface already spells.",
            },
            {
              severity: "minor",
              description:
                "The grouping comparator sorts by the whole path, so two files with the same name under different directories read as unrelated groups in the list even where the surface has room to say which directory each came from.",
              affectedFiles: [
                "src/components/search/results/grouping/internals/comparators/groupOrderingByAbsoluteRepositoryPath.ts",
              ],
              correction:
                "Sort by the file name first and let the directory break the tie.",
            },
          ],
        },
        correction: null,
      },
    ],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the search result grouping described in " +
      "specifications/ui/SRG-search-result-grouping.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/ui/SRG-search-result-grouping.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-23T09:00:00Z",
    updatedAt: "2026-08-24T11:30:00Z",
  },
  {
    id: "g-10",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-11",
    mode: "git",
    state: "implemented",
    // GRD-FR-69: a terminal run holds neither queue.
    queue: null,
    input: {
      draftId: "d-11",
      draftName: "Session restore",
      prompt: "The tabs that were open come back with the window.",
      promptChecksum: "sha-p11",
      capturedAt: "2026-08-20T08:00:00Z",
    },
    source: gitSource("session-restore"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    /**
     * GRU-FR-63: **one** implementation pass, which the Review let
     * through. The count is the turns that finished (GRD-FR-56), so a run whose
     * first pass was passed has made one — a two here says a second pass exists
     * and makes the `ready` verdict read as the reason that second pass
     * happened, which is the one thing this record must not say.
     */
    implementationIteration: 1,
    implementationSource: implementationSource("g-10"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-20T09:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/ui/SSR-session-restore.md",
          "created",
          "The prompt asks for the open tabs to come back.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-21T07:50:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src/state/session.ts", "created"),
        implementationEntry("src/App.tsx", "updated"),
      ],
      passed: true,
      summary: "One created, one updated.",
    },
    reviewHistory: [
      {
        iteration: 1,
        at: "2026-08-21T07:40:00Z",
        verdict: {
          verdict: "ready",
          rationale: "Every requirement of SSR is delivered and covered.",
          findings: [],
        },
        correction: null,
      },
    ],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the session restore described in " +
      "specifications/ui/SSR-session-restore.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/ui/SSR-session-restore.md"],
      commitId: SPECIFICATION_REVISION,
    },
    // GRD-FR-85 / GRH-FR-18: what the implementation landed, beside what the
    // specification landed.
    implementationPublication: {
      publication: { kind: "commit" },
      appliedPaths: ["src/state/session.ts", "src/App.tsx"],
      commitId: "b40d9ce",
    },
    enqueuedAt: "2026-08-20T08:00:00Z",
    updatedAt: "2026-08-21T08:00:00Z",
  },
  /**
   * GRU-FR-XKQD / GRU-FR-WJHV: the run whose implementation change set has a
   * **hidden** set — the paths the repository's ignore rules keep out of it,
   * with a bound the record says it truncated. Without one seeded here the
   * region below the implementation change set never renders at all.
   */
  {
    id: "g-14",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-15",
    mode: "git",
    state: "implementing",
    queue: "implementation",
    input: {
      draftId: "d-15",
      draftName: "Session log capture",
      prompt: "Each session writes its own log, and the oldest are dropped.",
      promptChecksum: "sha-p15",
      capturedAt: "2026-08-25T09:00:00Z",
    },
    source: gitSource("session-log-capture"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 1,
    implementationSource: implementationSource("g-14"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-25T10:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/SLC-session-log-capture.md",
          "created",
          "The prompt asks for the log rules to be written down.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-25T11:20:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src/state/sessionLog.ts", "created"),
        implementationEntry("src/logging.ts", "updated"),
      ],
      passed: true,
      summary: "One created, one updated.",
      // GRU-FR-WJHV: an ignored directory carries a trailing `/`; `omitted`
      // is how many the bound left out of the list.
      hidden: { paths: ["src/logs/", "notes.txt"], omitted: 3 },
    },
    reviewHistory: [],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the session log capture described in " +
      "specifications/core/SLC-session-log-capture.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/core/SLC-session-log-capture.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-25T09:00:00Z",
    updatedAt: "2026-08-25T11:20:00Z",
  },
  /**
   * GRU-FR-XKQD: the same region with **one** hidden path and no bound, so the
   * singular sentence renders and no `· n not listed` follows it. The path is
   * 200 characters on purpose: this list sits beside the run's controls, and a
   * path that does not wrap is what pushes the panel into horizontal scroll.
   */
  {
    id: "g-16",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-17",
    mode: "git",
    state: "implementing",
    queue: "implementation",
    input: {
      draftId: "d-17",
      draftName: "Coverage report bundling",
      prompt: "The coverage report is bundled with the build it came from.",
      promptChecksum: "sha-p17",
      capturedAt: "2026-08-26T09:00:00Z",
    },
    source: gitSource("coverage-report-bundling"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 1,
    implementationSource: implementationSource("g-16"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-26T10:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/infra/CRB-coverage-report-bundling.md",
          "created",
          "The prompt asks for the bundling rules to be written down.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-26T11:00:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src/state/coverageBundle.ts", "created"),
      ],
      passed: true,
      summary: "One created.",
      hidden: {
        paths: [
          "src/components/graduation/implementation/generated/artifacts/intermediate/coverage/reports/html/assets/vendor/chunks/graduation-implementation-hidden-paths-overflow-probe-x-x-x-x-x-x-x-x-x-fixture.map",
        ],
        omitted: 0,
      },
    },
    reviewHistory: [],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the coverage report bundling described in " +
      "specifications/infra/CRB-coverage-report-bundling.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/infra/CRB-coverage-report-bundling.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-26T09:00:00Z",
    updatedAt: "2026-08-26T11:00:00Z",
  },
  /**
   * GRU-FR-48: the run blocked because **two reviews in a row returned an
   * answer that could not be read**. The code carries no case of its own in
   * `blockerStatement`, so the sentence is the blocker's own message behind
   * "Nothing was published."
   */
  {
    id: "g-15",
    projectKey: "/Users/demo/dev/acme",
    draftId: "d-16",
    mode: "git",
    state: "implementation_blocked",
    queue: "implementation",
    input: {
      draftId: "d-16",
      draftName: "Reviewer transcript parsing",
      prompt: "The reviewer's answer is read back as a verdict and its findings.",
      promptChecksum: "sha-p16",
      capturedAt: "2026-08-25T12:00:00Z",
    },
    source: gitSource("reviewer-transcript-parsing"),
    iteration: 1,
    specificationRevision: SPECIFICATION_REVISION,
    implementationIteration: 2,
    implementationSource: implementationSource("g-15"),
    manifest: {
      kind: "specification",
      computedAt: "2026-08-25T13:00:00Z",
      baseline: { kind: "git_revision", revision: "a91bc04" },
      entries: [
        manifestEntry(
          "specifications/core/RTP-reviewer-transcript-parsing.md",
          "created",
          "The prompt asks for the reading of a verdict to be written down.",
        ),
      ],
      passed: true,
      summary: "One specification path.",
    },
    implementationManifest: {
      kind: "implementation",
      computedAt: "2026-08-25T14:30:00Z",
      baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
      entries: [
        implementationEntry("src/state/graduation/verdict.ts", "created"),
      ],
      passed: true,
      summary: "One created.",
    },
    implementationBlocker: {
      code: "review_result_invalid",
      message:
        "Two reviews in a row returned an answer that could not be read. " +
        "Continue to ask for another review.",
      paths: [],
    },
    reviewHistory: [],
    escalation: null,
    lastFailure: null,
    handoffPrompt:
      "Implement the reviewer transcript parsing described in " +
      "specifications/core/RTP-reviewer-transcript-parsing.md.",
    publication: {
      publication: { kind: "commit" },
      appliedPaths: ["specifications/core/RTP-reviewer-transcript-parsing.md"],
      commitId: SPECIFICATION_REVISION,
    },
    enqueuedAt: "2026-08-25T12:00:00Z",
    updatedAt: "2026-08-25T14:30:00Z",
  },
];

/**
 * GRD-FR-52 … GRD-FR-56: the stage each seeded run stands at, so the Runs
 * panel's progress row (GRU-FR-26) is reachable without driving a run.
 *
 * Attached after the seed rather than written into each entry: what it holds
 * is a function of the run's state, exactly as the backend's own mapping is
 * (GRD-FR-53), so writing it out per run would invite the two to disagree.
 * `g-1` additionally carries a loop and its explanation, because a run that
 * has been round is the case the block exists for (GRU-FR-32).
 */
/**
 * GRU-FR-43: the words an account is made of.
 *
 * Held apart from the loop that seeds them so a pass reads as its own reason
 * rather than as the same sentence eight times, and so one of them is long
 * enough to prove the account wraps inside the panel instead of widening it.
 */
const REVISION_RATIONALES = [
  "The change set adds the command but says nothing about what happens when no worktree is stale, which the prompt asks for.",
  "The requirement for the empty case names no outcome, so nothing tells a reader what the command reports when it removed nothing at all.",
  "Two requirements now describe the same removal from opposite directions, and a reader cannot tell which one governs when they disagree.",
  "The scenario for a branch that is gone assumes the worktree is clean, so the case the prompt actually asks about — a dirty worktree whose branch is gone — is written down nowhere in the specification at all.",
  "The application refused one path: a specification under `specifications/ui/` may not be created by a graduation whose prompt names no surface.",
  "The removal is specified, but the record of what was removed is not, so a run that removed four worktrees and a run that removed none leave the same trace behind them.",
  "The validation and the application's checks disagree about where the command lives, and the change set answers neither.",
  "The command's own name is given in three spellings across the two files, and the specification never says which of them is the one a user types.",
];

/** GRD-FR-56: the author's own words, which are the whole account. */
const AUTHOR_CORRECTIONS = [
  "Say what the command prints when it removed nothing. It should read as an ordinary outcome, not as a failure.",
  "Drop the interactive confirmation entirely. The command runs unattended in a hook, so anything that waits for a keypress there is a hang rather than a prompt, and I would rather have a flag that refuses than a question nobody can answer.",
];

/** The whole text the next execution turn is told to act on. */
const REVISION_CORRECTIONS = [
  "Add a requirement covering the case where no worktree is stale, and a scenario for it.\n\nThe prompt asks for the command to say what it did, so the empty case has to be one of the things it can say.",
  "State the outcome the empty case produces, in the same words the non-empty case uses, so a reader comparing the two sees one command rather than two.",
  "Reconcile the two removal requirements into one, and keep the numbering of whichever you keep.",
  "Write the dirty-worktree case down: what the command does, what it reports, and what it leaves behind. Add a scenario for it beside the clean one.",
  "Move the surface requirement into the core specification, or name the surface in the prompt. The application will keep refusing the path until one of those is true.",
  "Specify the record the command leaves: what it removed, what it skipped, and why it skipped it.",
  "Put the command in one place and say so once. Then make the surface reference it rather than restate it.",
  "Pick one spelling of the command's name, use it everywhere, and say in the specification that it is the name a user types.",
];

const STAGE_BY_STATE: Record<string, [string, string]> = {
  // The specification part.
  queued: ["queued", "active"],
  running: ["authoring", "active"],
  requested_changes: ["authoring", "active"],
  awaiting_user_decision: ["validation", "waiting"],
  awaiting_review: ["acceptance", "waiting"],
  awaiting_publication_choice: ["acceptance", "waiting"],
  publishing: ["acceptance", "active"],
  publication_conflict: ["acceptance", "blocked"],
  interrupted: ["authoring", "paused"],
  // GRD-FR-71: the specification is published and the run holds neither queue.
  awaiting_implementation: ["acceptance", "complete"],
  // The implementation part (GRD-FR-69).
  implementation_queued: ["implementation_queued", "waiting"],
  implementing: ["implementation", "active"],
  implementation_blocked: ["implementation", "blocked"],
  awaiting_implementation_publication_choice: [
    "implementation_acceptance",
    "waiting",
  ],
  implementation_publishing: ["implementation_acceptance", "active"],
  implementation_publication_conflict: ["implementation_acceptance", "blocked"],
  // Terminal.
  implemented: ["implementation_acceptance", "complete"],
  implementation_accepted: ["implementation_acceptance", "complete"],
  rejected: ["authoring", "stopped"],
  discarded: ["authoring", "stopped"],
  failed: ["authoring", "stopped"],
};

/** GRD-FR-52: the eight stages, in the order a run passes through them. */
const STAGE_ORDER = [
  "queued",
  "authoring",
  "validation",
  "acceptance",
  "implementation_queued",
  "implementation",
  "review",
  "implementation_acceptance",
];

/** GRD-FR-54: the reason recorded for arriving at each stage. */
const STAGE_ARRIVAL_REASON: Record<string, string> = {
  authoring: "queue_released",
  validation: "validation_started",
  acceptance: "acceptance_started",
  implementation_queued: "implementation_enqueued",
  implementation: "implementation_started",
  review: "review_started",
  implementation_acceptance: "implementation_acceptance_started",
};

function applyObservability(run: any) {
  const [currentStage, stageCondition] = STAGE_BY_STATE[run.state] ?? [
    "authoring",
    "active",
  ];
  const at = run.updatedAt;
  const history: any[] = [
    { from: null, to: "queued", iteration: 0, at: run.enqueuedAt, reason: "enqueued" },
  ];
  // GRD-FR-54: one move per stage the run has actually arrived at, in order,
  // so a run standing in the implementation part carries the whole of how it
  // got there rather than only the specification half of it.
  const reached = STAGE_ORDER.indexOf(currentStage);
  for (let step = 1; step <= reached; step += 1) {
    history.push({
      from: STAGE_ORDER[step - 1],
      to: STAGE_ORDER[step],
      iteration: step <= 3 ? run.iteration : (run.implementationIteration ?? 0),
      at,
      reason: STAGE_ARRIVAL_REASON[STAGE_ORDER[step]] ?? "migration",
    });
  }
  const revisions: any[] = [];
  if (run.id === "g-1") {
    /**
     * GRU-FR-32 / GRU-FR-43 / GRU-FR-44: eight passes with an account and a
     * ninth under way.
     *
     * Both causes are present, because they read differently: a pass the loop
     * ended carries a rationale **and** a correction, so the account offers the
     * disclosure beneath it; a pass the author ended carries `rationale: null`,
     * so their own words are the whole account and no disclosure is offered
     * (GRD-FR-56).
     *
     * Long enough that the list has to scroll within itself rather than push
     * the account off the bottom of the panel.
     */
    const causes = [
      "validation",
      "validation",
      "requested_changes",
      "validation",
      "application_gate",
      "requested_changes",
      "validation_and_application_gate",
      "validation",
    ];
    for (let n = 1; n <= causes.length; n += 1) {
      const cause = causes[n - 1];
      const author = cause === "requested_changes";
      history.push(
        {
          from: "authoring",
          to: "validation",
          iteration: n,
          at,
          reason: "validation_started",
        },
        {
          from: author ? "acceptance" : "validation",
          to: "authoring",
          iteration: n,
          at,
          reason: author ? "requested_changes" : "validation_revision",
        },
      );
      revisions.push({
        iteration: n,
        at,
        // GRU-FR-33: the rationale and the correction are model text, so the
        // harness can put hostile text in them. Add `?hostileRevisionText` to
        // the URL to get markup, a link, a heading, a fence, and an unbroken
        // token that is longer than the panel — everything the block must show
        // as ordinary characters and never as something to click or to widen
        // against.
        //   http://localhost:5199/?hostileRevisionText
        rationale: author
          ? // GRD-FR-56: the author's own request carries no rationale. Their
            // words are the correction, and the account is that alone.
            null
          : harnessFlag("hostileRevisionText") && n === 1
            ? "# Heading\n\nThe change set <b>adds</b> the command but [says nothing](https://example.com/attack) about the empty case. <img src=x onerror=alert(1)> <script>alert(2)</script>\n\n    indented code block\n\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            : REVISION_RATIONALES[(n - 1) % REVISION_RATIONALES.length],
        correction:
          harnessFlag("hostileRevisionText") && n === 1
            ? "```js\nwindow.open('https://example.com')\n```\n\n* Add a requirement for the empty case\n* See http://example.com/also-not-a-link\n\n<a href=\"https://example.com\">not a link</a> and a token: https://example.com/very/long/path/that/keeps/going/and/going/and/going/and/going/and/going"
            : author
              ? AUTHOR_CORRECTIONS[(n - 1) % AUTHOR_CORRECTIONS.length]
              : REVISION_CORRECTIONS[(n - 1) % REVISION_CORRECTIONS.length],
        cause,
      });
    }
  }
  /**
   * GRD-FR-56 / GRU-FR-53: a run that writes its own explanations out.
   *
   * `g-1`'s eight passes are generated, because eight of one shape is what
   * makes the list scroll. A run whose passes are each a different decision —
   * one part sending work back to the other — is written out beside its
   * `reviewHistory` instead, so the two records of one decision agree.
   */
  if (Array.isArray(run.seedRevisions)) revisions.push(...run.seedRevisions);
  run.observability = {
    observabilityVersion: 1,
    currentStage,
    stageCondition,
    queue: {
      waitReason: currentStage === "queued" ? "run_ahead" : null,
      leftQueuedAt: currentStage === "queued" ? null : at,
    },
    stageHistory: history,
    revisionHistory: revisions,
    // GOB-FR-XYCY: the backend always serializes `passes` (a Vec with serde
    // default), so a version-1 record without it is a shape no backend sends.
    passes: [],
  };
}

for (const run of graduationRuns) applyObservability(run);

/* --- Work streams (WKS-work-streams.md) ----------------------------------
 *
 * A work stream is the named branch and working copy a graduation run executes
 * in (WKS-FR-QMTV). Both graduation surfaces read this listing: the start
 * dialog offers the streams a run can be enqueued on (GSD-FR-ZPWN), and the
 * restart confirmation offers the same set (GRT-FR-IMRI).
 *
 * Three streams, deliberately unlike each other:
 *   - `s-1` is ordinary and holds the seeded runs;
 *   - `s-2` is **busy**, so the start dialog's "waits behind it" note is
 *     reachable (GSD-FR-HRJE);
 *   - `s-3` is **missing**, so the filter that drops a stream with no working
 *     copy is exercised rather than assumed (GSD-FR-ZPWN).
 */
const workStreams: any[] = [
  {
    id: "s-1",
    projectKey: "/Users/demo/dev/acme",
    name: "editor-work",
    branch: "synthesis/stream/editor-work",
    worktreePath: "/Users/demo/.synthesis/streams/editor-work",
    baseBranch: "main",
    baseRevision: "a91bc04",
    createdAt: "2026-08-10T09:00:00Z",
    busyRunId: null,
    isMissing: false,
  },
  {
    id: "s-2",
    projectKey: "/Users/demo/dev/acme",
    name: "registry-migration",
    branch: "synthesis/stream/registry-migration",
    worktreePath: "/Users/demo/.synthesis/streams/registry-migration",
    baseBranch: "main",
    baseRevision: "a91bc04",
    createdAt: "2026-08-12T09:00:00Z",
    busyRunId: "g-1",
    isMissing: false,
  },
  {
    id: "s-3",
    projectKey: "/Users/demo/dev/acme",
    name: "gone-stream",
    branch: "synthesis/stream/gone-stream",
    worktreePath: "/Users/demo/.synthesis/streams/gone-stream",
    baseBranch: "main",
    baseRevision: "a91bc04",
    createdAt: "2026-07-01T09:00:00Z",
    busyRunId: null,
    isMissing: true,
  },
  // GSD-FR-WQPD: a stream no run holds, which runs wait on all the same. It is
  // what makes the queued-only occupancy case reachable, beside s-1 (free) and
  // s-2 (held by a run).
  {
    id: "s-4",
    projectKey: "/Users/demo/dev/acme",
    name: "docs-refresh",
    branch: "synthesis/stream/docs-refresh",
    worktreePath: "/Users/demo/.synthesis/streams/docs-refresh",
    baseBranch: "main",
    baseRevision: "a91bc04",
    createdAt: "2026-08-20T09:00:00Z",
    busyRunId: null,
    isMissing: false,
  },
  // WSS-FR-RJTN: a stream whose merge run rests `awaiting_author`, so the row
  // that waits on the author and the Open in Runs control are both reachable.
  {
    id: "s-5",
    projectKey: "/Users/demo/dev/acme",
    name: "bounds-work",
    branch: "synthesis/stream/bounds-work",
    worktreePath: "/Users/demo/.synthesis/streams/bounds-work",
    baseBranch: "main",
    baseRevision: "a91bc04",
    createdAt: "2026-08-22T09:00:00Z",
    busyRunId: null,
    isMissing: false,
  },
  // WSS-FR-OFCU: a stream whose merge run is reconciling, so the stream is busy
  // and its row shows `Merge spec-split` with its state in words.
  {
    id: "s-6",
    projectKey: "/Users/demo/dev/acme",
    name: "spec-split",
    branch: "synthesis/stream/spec-split",
    worktreePath: "/Users/demo/.synthesis/streams/spec-split",
    baseBranch: "main",
    baseRevision: "a91bc04",
    createdAt: "2026-08-24T09:00:00Z",
    busyRunId: "g-30",
    isMissing: false,
  },
];

/**
 * AGV-FR-02: a short, believable transcript for any run or merge attempt.
 *
 * Deterministic in the id, so the same surface reads the same lines every time
 * and a screenshot can be compared with the one before it.
 */
function agentActivityRecords(runId: string) {
  // RUN-FR-LQDT: a merge run's turns are agent work of the run, read by its id.
  const merge =
    runId.startsWith("att-") ||
    Boolean(graduationRuns.find((r) => r.id === runId)?.merge);
  const lines: [string, string][] = merge
    ? [
        ["invocation", "claude --print (semantic reconciliation)"],
        ["started", "Reading the conflicted hunks"],
        ["reasoning", "Both sides renamed the same requirement id."],
        ["tool_call", "read_file specifications/ui/GRU-graduation-runs.md"],
        ["file_change", "specifications/ui/GRU-graduation-runs.md"],
        ["message", "Kept both requirements and renumbered the later one."],
      ]
    : [
        ["invocation", "claude --print (implementation)"],
        ["started", "Reading the specification"],
        ["tool_call", "read_file specifications/ui/EDT-editor.md"],
        ["message", "The change is contained to one component."],
        ["finished", "exit 0"],
      ];
  return lines.map(([kind, summary], i) => ({
    seq: i + 1,
    at: new Date(Date.parse(now) + i * 4000).toISOString(),
    channel: kind === "invocation" ? "executor" : "stdout",
    kind,
    summary,
    payload: JSON.stringify({ kind, summary }),
    payloadTruncated: false,
  }));
}


/**
 * GRD-FR-PZAK / GRD-FR-HQPD: the stream a seeded run belongs to and what it
 * does with work standing in that stream.
 *
 * Attached after the seed rather than written into every entry, so the runs
 * above stay one statement about the loop and this stays one statement about
 * the stream assignment.
 */
for (const run of graduationRuns) {
  if (!run.streamId) {
    const stream = workStreams[Number(String(run.id).replace(/\D/g, "")) % 2];
    run.streamId = stream.id;
    run.streamName = stream.name;
  }
  if (!run.standingWork) run.standingWork = "commit";
}

/** GRD-FR-KDWA: the log index every run carries. Healthy and empty. */
function streamIndex(name: "activity" | "structured") {
  return {
    stream: name,
    latestSequence: 0,
    recordCount: 0,
    durableThroughSequence: 0,
    byteLength: 0,
    segments: [],
  };
}

/**
 * One run in the shape `src/types/graduation.ts` describes today.
 *
 * The seeded runs above predate the work-stream model, so the two surfaces this
 * harness has to reach — GRT's restart confirmation, which is offered for a
 * discarded run and no other state (GRT-FR-CTNO), and GRU-FR-TFEZ's report of a
 * push the remote refused — get runs written against the current type instead
 * of a legacy entry patched into it.
 */
function makeStreamRun(over: any) {
  return {
    projectKey: "/Users/demo/dev/acme",
    streamId: "s-1",
    streamName: "editor-work",
    standingWork: "commit",
    standingWorkOutcome: null,
    baseCommit: "a91bc04",
    commits: [],
    autoStart: true,
    archived: false,
    workTurns: 1,
    reviewTurns: 0,
    checkpoint: {
      pass: 1,
      changedPaths: [],
      hiddenPaths: [],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
    },
    logs: {
      logStorageVersion: 1,
      activity: streamIndex("activity"),
      structured: streamIndex("structured"),
      persistence: {
        status: "healthy",
        pendingCount: 0,
        updatedAt: "2026-09-06T09:00:00Z",
      },
    },
    observability: {
      observabilityVersion: 1,
      currentStage: "working",
      stageCondition: "active",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "working",
          task: "Make the editor keep its scroll position.",
          findings: [],
          startedAt: "2026-09-06T09:02:00Z",
        },
      ],
    },
    enqueuedAt: "2026-09-06T09:00:00Z",
    createdAt: "2026-09-06T09:00:00Z",
    updatedAt: "2026-09-06T09:10:00Z",
    ...over,
  };
}

graduationRuns.push(
  // GRT-FR-CTNO: the one state Restart is offered for.
  makeStreamRun({
    id: "g-20",
    draftId: "d-6",
    state: "discarded",
    input: {
      draftId: "d-6",
      draftName: "Scroll position on reopen",
      prompt: "Keep the editor's scroll position when a tab is reopened.",
      promptChecksum: "sha-p20",
      capturedAt: "2026-09-05T09:00:00Z",
    },
  }),
  // GRU-FR-TFEZ: a run whose stream branch the remote did not take. The run was
  // not stopped by it, so it is working rather than blocked.
  makeStreamRun({
    id: "g-21",
    draftId: "d-7",
    state: "working",
    standingWork: "commit_and_push",
    standingWorkOutcome: {
      commit: "b73de91",
      pushed: false,
      pushFailure: { code: "invalid_token" },
    },
    input: {
      draftId: "d-7",
      draftName: "Gutter marks for review",
      prompt: "Mark reviewed hunks in the diff gutter.",
      promptChecksum: "sha-p21",
      capturedAt: "2026-09-05T10:00:00Z",
    },
  }),
  // GRU-FR-DVWY: an interrupted run that made commits. It holds no stream, so
  // it offers Revert beside Continue and Discard run.
  makeStreamRun({
    id: "g-22",
    draftId: "d-8",
    state: "interrupted",
    commits: ["c41e7a2", "9d03b6f"],
    input: {
      draftId: "d-8",
      draftName: "Row virtualization rework",
      prompt: "Virtualize the rows of long panels.",
      promptChecksum: "sha-p22",
      capturedAt: "2026-09-05T11:00:00Z",
    },
  }),
  // GRU-FR-DVWY / GRU-FR-WDWB: a discarded run that made commits. It offers
  // Revert beside Restart.
  makeStreamRun({
    id: "g-23",
    draftId: "d-9",
    state: "discarded",
    commits: ["e28f5c1"],
    input: {
      draftId: "d-9",
      draftName: "Token rotation rework",
      prompt: "Rotate provider tokens before they expire.",
      promptChecksum: "sha-p23",
      capturedAt: "2026-09-05T12:00:00Z",
    },
  }),
);

/**
 * GRU-FR-TXLW / GRU-FR-HEQB / GRU-FR-VDSK / GRU-FR-OZAR: a run that changed
 * many paths and took two passes, so the path tree has to scroll in its box,
 * only its root folders open, the open pass row has an account long enough to
 * scroll under it, and the region is long enough to scroll under its foot bar.
 */
function largeChangeSet(): string[] {
  const paths: string[] = [];
  const areas = ["Editor", "Library", "Runs", "Review", "Settings", "Shell"];
  for (const area of areas) {
    for (let i = 1; i <= 18; i++) paths.push(`src/components/${area}/part${i}.tsx`);
  }
  for (let i = 1; i <= 30; i++) paths.push(`src/state/graduation/step${i}.ts`);
  for (let i = 1; i <= 20; i++) paths.push(`specifications/ui/SPEC-${i}-a-long-specification-name-that-keeps-going-and-going.md`);
  paths.push("docs/guides/graduation/overview.md");
  paths.push("src-tauri/src/graduation/loop/dispatch.rs");
  paths.push("src-tauri/src/graduation/loop/review.rs");
  paths.push("README.md");
  return paths;
}
const LONG_ACCOUNT_FINDINGS = Array.from({ length: 8 }, (_, i) => ({
  severity: i % 2 === 0 ? "critical" : "major",
  description: `Finding ${i + 1}: the change set renames a command but leaves the old name in the help text, the shortcut table, and the palette entry.`,
  affectedFiles: [`src/components/Editor/part${i + 1}.tsx`, "src/state/graduation/step1.ts"],
  correction: "Rename every remaining reference and add a test that reads the palette.",
}));
graduationRuns.push(
  makeStreamRun({
    id: "g-24",
    draftId: "d-24",
    state: "working",
    input: {
      draftId: "d-24",
      draftName: "Large change set",
      prompt: "Rework every panel to one tree row.",
      promptChecksum: "sha-p24",
      capturedAt: "2026-09-05T13:00:00Z",
    },
    checkpoint: {
      pass: 2,
      changedPaths: largeChangeSet(),
      hiddenPaths: [
        "build/cache/a.bin",
        "build/cache/b.bin",
        "node_modules/.vite/deps/chunk.js",
        ".env.local",
      ],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
    },
    observability: {
      observabilityVersion: 1,
      currentStage: "working",
      stageCondition: "active",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "failed",
          task: "Rework every panel to one tree row.\n".repeat(6),
          verdict: "revise",
          rationale: "The work is close, but eight references to the old name remain. ".repeat(6),
          findings: LONG_ACCOUNT_FINDINGS,
          nextInstruction: "Fix every finding above, then run the full test suite.\n".repeat(4),
          startedAt: "2026-09-05T13:02:00Z",
          endedAt: "2026-09-05T13:30:00Z",
        },
        {
          pass: 2,
          status: "working",
          task: "Fix every finding of the first pass.\n".repeat(10),
          findings: [],
          startedAt: "2026-09-05T13:31:00Z",
        },
      ],
    },
  }),
);

/**
 * GRU-FR-BHJO / GRU-FR-QLRQ / GRU-FR-JAEY: two runs stopped by a failure. The
 * first reached its time limit, so its notice routes to the Graduation
 * settings section; the second had its agent exit, so it does not.
 */
graduationRuns.push(
  makeStreamRun({
    id: "g-25",
    draftId: "d-25",
    state: "interrupted",
    interruption: {
      reason: "execution_timeout",
      at: "2026-09-06T11:00:00Z",
      detail: "The agent turn ran for 120 minutes and was stopped.",
      streamReleased: true,
    },
    input: {
      draftId: "d-25",
      draftName: "Slow migration turn",
      prompt: "Migrate every settings row to the new store.",
      promptChecksum: "sha-p25",
      capturedAt: "2026-09-05T14:00:00Z",
    },
  }),
  makeStreamRun({
    id: "g-26",
    draftId: "d-26",
    state: "interrupted",
    interruption: {
      reason: "agent_exited",
      at: "2026-09-06T11:30:00Z",
      detail: "The agent process exited with status 1.",
      streamReleased: true,
    },
    input: {
      draftId: "d-26",
      draftName: "Agent crash turn",
      prompt: "Rename the stream fields.",
      promptChecksum: "sha-p26",
      capturedAt: "2026-09-05T15:00:00Z",
    },
  }),
);

/**
 * GRD-FR-MRNQ / GRD-FR-VCTH: the merge runs the harness queue holds.
 *
 * A merge run carries `merge` data, has no draft, and is titled `Merge <stream>`.
 * `g-30` is reconciling in `spec-split`, so that stream is busy; `g-31` rests
 * `awaiting_author` in `bounds-work` with two questions, so the escalation of a
 * merge run (GEA-FR-MRDK, GEA-FR-WQZH) is reachable; `g-32` is a merge that
 * landed as a commit in `docs-refresh`, so a result with its short commit
 * (GRU-FR-JRMA) is reachable.
 */
function mergeData(over: any) {
  return {
    name: "Merge spec-split",
    streamBranch: "synthesis/stream/spec-split",
    baseBranch: "main",
    baseTip: "b".repeat(40),
    streamTip: "c".repeat(40),
    mergeBase: "a".repeat(40),
    snapshotCommit: "d".repeat(40),
    publication: { kind: "uncommitted" },
    changedPaths: [
      "specifications/ui/TAB-tab-strip.md",
      "specifications/ui/SNV-shell-navigation.md",
      "specifications/core/GRD-graduation.md",
      "src/styles/components.css",
      "src/components/Tabs.tsx",
    ],
    unresolvedPaths: [
      "specifications/ui/TAB-tab-strip.md",
      "specifications/ui/SNV-shell-navigation.md",
      "src/styles/components.css",
    ],
    conflicts: [
      { path: "specifications/ui/TAB-tab-strip.md", baseChange: "updated", streamChange: "updated" },
      { path: "specifications/ui/SNV-shell-navigation.md", baseChange: "updated", streamChange: "deleted" },
      { path: "src/styles/components.css", baseChange: "updated", streamChange: "updated" },
    ],
    result: null,
    ...over,
  };
}

function makeMergeRunSeed(over: any) {
  const { merge, ...rest } = over;
  return makeStreamRun({
    standingWork: "keep",
    baseCommit: "d".repeat(40),
    input: {
      draftId: "",
      draftName: "",
      prompt: "Merge the work stream into its base branch.",
      promptChecksum: "sha-merge",
      capturedAt: "2026-09-06T09:00:00Z",
    },
    merge: mergeData(merge ?? {}),
    ...rest,
  });
}

graduationRuns.push(
  makeMergeRunSeed({
    id: "g-30",
    streamId: "s-6",
    streamName: "spec-split",
    state: "working",
    workTurns: 1,
    checkpoint: {
      pass: 1,
      changedPaths: ["specifications/ui/TAB-tab-strip.md"],
      hiddenPaths: [],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
    },
    observability: {
      observabilityVersion: 1,
      currentStage: "working",
      stageCondition: "active",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "working",
          task: "Resolve the three paths Git could not merge and reconcile the rest.",
          findings: [],
          startedAt: "2026-09-06T09:02:00Z",
        },
      ],
    },
  }),
  makeMergeRunSeed({
    id: "g-31",
    streamId: "s-5",
    streamName: "bounds-work",
    state: "awaiting_author",
    workTurns: 1,
    merge: {
      name: "Merge bounds-work",
      streamBranch: "synthesis/stream/bounds-work",
      publication: { kind: "commit", message: "Merge bounds-work into main" },
      changedPaths: ["specifications/core/GRB-graduation-rebase.md", "specifications/core/WKS-work-streams.md"],
      unresolvedPaths: ["specifications/core/GRB-graduation-rebase.md"],
      conflicts: [
        { path: "specifications/core/GRB-graduation-rebase.md", baseChange: "updated", streamChange: "updated" },
      ],
    },
    escalation: {
      origin: "work",
      raisedAt: "2026-09-06T09:20:00Z",
      reason:
        "Both branches rewrote the bound the graduation loop stops at, and the two versions contradict each other rather than overlap.",
      questions: [
        {
          position: 1,
          question: "Which bound should the merged specification state?",
          options: [
            {
              answer: "turns",
              summary: "The bound on main",
              description: "An attempt runs as many turns as it needs up to the bound.",
            },
            {
              answer: "attempts",
              summary: "The bound on the stream",
              description: "Each attempt runs one turn, and the merge stops after the bound.",
            },
          ],
        },
      ],
    },
    checkpoint: {
      pass: 1,
      changedPaths: [],
      hiddenPaths: [],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
    },
    observability: {
      observabilityVersion: 1,
      currentStage: "working",
      stageCondition: "waiting",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "working",
          task: "Resolve the path Git could not merge.",
          findings: [],
          startedAt: "2026-09-06T09:02:00Z",
        },
      ],
    },
  }),
  makeMergeRunSeed({
    id: "g-32",
    streamId: "s-4",
    streamName: "docs-refresh",
    state: "completed",
    workTurns: 1,
    reviewTurns: 1,
    commits: ["9f2c1ab4d5e6f708192a3b4c5d6e7f8091a2b3c4"],
    merge: {
      name: "Merge docs-refresh",
      streamBranch: "synthesis/stream/docs-refresh",
      publication: { kind: "commit", message: "Merge docs-refresh into main" },
      changedPaths: ["docs/guides/overview.md", "docs/guides/setup.md"],
      unresolvedPaths: ["docs/guides/overview.md"],
      conflicts: [
        { path: "docs/guides/overview.md", baseChange: "updated", streamChange: "updated" },
      ],
      result: {
        published: "commit",
        commit: "9f2c1ab4d5e6f708192a3b4c5d6e7f8091a2b3c4",
        mergedPaths: ["docs/guides/overview.md", "docs/guides/setup.md"],
      },
    },
    observability: {
      observabilityVersion: 1,
      currentStage: "done",
      stageCondition: "complete",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "passed",
          task: "Resolve the path Git could not merge.",
          verdict: "ready",
          rationale: "The reconciled result keeps both branches' intent.",
          findings: [],
          startedAt: "2026-09-06T09:02:00Z",
          endedAt: "2026-09-06T09:30:00Z",
        },
      ],
    },
  }),
);

/** WKS-FR-KHJS: the stream's newest live merge run, from the queue alone. */
function mergeRunOf(streamId: string) {
  const runs = graduationRuns.filter(
    (r) => r.merge && r.streamId === streamId && r.state !== "discarded" && !r.archived,
  );
  const run = runs[runs.length - 1];
  return run ? { runId: run.id, name: run.merge.name, state: run.state } : null;
}

/** Whether a merge run of the stream stands in a state that holds its place. */
function mergeRunHolds(streamId: string) {
  const link = mergeRunOf(streamId);
  return Boolean(link) && link!.state !== "completed" && link!.state !== "failed";
}

/** WKS-FR-ZLWT: a merge a conflict handed off, as the queue holds it. */
function enqueueMergeRun(stream: any, publication: any) {
  const run = makeMergeRunSeed({
    id: `g-merge-${Date.now()}`,
    streamId: stream.id,
    streamName: stream.name,
    state: "queued",
    workTurns: 0,
    merge: {
      name: `Merge ${stream.name}`,
      streamBranch: stream.branch,
      baseBranch: stream.baseBranch,
      publication: publication ?? { kind: "uncommitted" },
    },
  });
  graduationRuns.push(run);
  return run;
}

/**
 * GCM-FR-47: one manifest path's text on each side of its change.
 *
 * A specification is the length a specification is, so this pair is too: a
 * change set of two-line files leaves the review's file region short of content
 * and the modal short of its own maximum height, which hides both what the
 * comparison does when it has to scroll and whether the surface fits the
 * smallest window the shell supports.
 *
 * The pair holds, deliberately:
 *   - an unchanged head, so context rows are present in every visualization;
 *   - a **replaced** paragraph with a word-level difference inside it, so
 *     Unified and Final are not the same picture and DFV-FR-32's marking within
 *     a replaced unit is reachable;
 *   - a **removed** block, so Side-by-side has a row facing inert filler;
 *   - added sections at the foot;
 *   - one very long line, so horizontal overflow is measured rather than
 *     assumed.
 */
const graduationBaselineText = `# Comments

A thread anchors to a range of the artifact it was left on.

## Anchors

An anchor is a range of the artifact, held as an offset and a length. The store
resolves it against the revision the thread was left on, and reports the range
it resolved to.

An anchor the store cannot resolve is reported as detached, and the thread is
listed at the foot of the artifact rather than beside a line of it.

## Ordering

Threads are listed in the order their anchors resolve to, and comments within a
thread in the order they were left.

## Retention

A thread is kept for as long as the artifact is. Deleting the artifact deletes
the threads left on it, which is the one place a comment is removed without its
author asking.

## Notification

The author of a thread is notified when a comment is left on it.
`;

const graduationCurrentText = `# Comments

A thread anchors to a range of the artifact it was left on.

## Anchors

An anchor is a range of the artifact, held as an offset and a length. The store
resolves it against the revision the thread was left on, and reports both the
range it resolved to and the confidence it resolved with.

An anchor the store cannot resolve is reported as detached, and the thread is
listed at the foot of the artifact rather than beside a line of it.

## Ordering

Threads are listed in the order their anchors resolve to, and comments within a
thread in the order they were left.

## Retention

A thread is kept for as long as the artifact is. Deleting the artifact deletes
the threads left on it, which is the one place a comment is removed without its
author asking.

## Export

A thread may be exported as Markdown, which writes the thread's comments in the order they were left, each with its author and the time it was left at, and which never rewrites the anchor it was left against — a very long line, deliberately, so the review's file region is checked for horizontal overflow rather than assumed to wrap.

An export names the artifact, the range the anchor resolved to, and the revision
it resolved against, so a thread read outside the application still says what it
was left on.

### What an export never holds

An export holds no draft, no proposal, and no comment its author withdrew: what
it writes is the thread as it stands, and a reader of the file is reading what a
reader of the thread reads.
`;

/** GRD-FR-01: the five states a run does not come back from. */
const GRADUATION_TERMINAL = [
  "implemented",
  "implementation_accepted",
  "rejected",
  "discarded",
  "failed",
];

/**
 * DRS-FR-18 / GRD-FR-LGDV: what a draft's own surface reads, from the queue.
 *
 * `graduated` is carried beside the state because the five terminal states are
 * reached from either side of the specification publication boundary, so a
 * reader holding the state alone cannot tell a discarded implementation from a
 * rejected specification (GRD-FR-38, GRD-FR-39). Here the boundary is satisfied
 * exactly when a specification publication was recorded, or the in-place
 * acceptance that stands for one moved the run into the implementation part.
 */
function draftGraduation(draftId: string): any {
  const run = graduationRuns.find((r) => r.draftId === draftId);
  if (!run) return null;
  return {
    runId: run.id,
    state: run.state,
    locked: !GRADUATION_TERMINAL.includes(run.state),
    graduated: run.publication != null || IMPLEMENTATION_PART.includes(run.state),
  };
}

/** GRD-FR-69: the states that belong to the implementation part. */
const IMPLEMENTATION_PART = [
  "implementation_queued",
  "implementing",
  "implementation_blocked",
  "awaiting_implementation_publication_choice",
  "implementation_publishing",
  "implementation_publication_conflict",
  "implemented",
  "implementation_accepted",
];

const findRun = (runId: string) => {
  const run = graduationRuns.find((r) => r.id === runId);
  // eslint-disable-next-line no-throw-literal
  if (!run) throw "run_not_found";
  return run;
};

/* --- Graduation agent activity (GRS-graduation-run-log-storage.md) ---------
 *
 * Run `g-40` is working on its second pass after a review sent it back, so its
 * Review phase holds an earlier pass the window must still list (GLW-FR-CKLZ).
 * Records are kept per run, phase, and pass, and `read_graduation_logs` answers
 * from them the way the backend does: ascending by sequence, bounded to a page.
 *
 * Add a record while the window is open, as a running phase would:
 *   window.__appendGraduationActivity("g-40", "working", 2, "message", "text")
 */
const activityKinds = [
  "started",
  "message",
  "tool_call",
  "tool_result",
  "progress",
  "diagnostic",
];
const graduationActivity: Record<
  string,
  Array<{ sequence: number; at: string; phaseId: string; pass: number | null; kind: string; summary: string }>
> = { "g-40": [] };
(function seedGraduationActivity() {
  const records = graduationActivity["g-40"];
  const base = Date.parse("2026-09-06T09:02:00Z");
  const add = (phaseId: string, pass: number | null, count: number) => {
    for (let i = 0; i < count; i += 1) {
      const sequence = records.length + 1;
      records.push({
        sequence,
        at: new Date(base + sequence * 4000).toISOString(),
        phaseId,
        pass,
        kind: activityKinds[i % activityKinds.length],
        summary: `${phaseId} pass ${pass ?? "run-level"} step ${i + 1}`,
      });
    }
  };
  add("queued", null, 1);
  add("working", 1, 60);
  add("review", 1, 6);
  add("working", 2, 4);
})();

function graduationActivityIndex(runId: string) {
  const records = graduationActivity[runId] ?? [];
  const segments: any[] = [];
  for (const r of records) {
    const last = segments[segments.length - 1];
    if (last && last.phaseId === r.phaseId && last.pass === r.pass) {
      last.lastSequence = r.sequence;
      last.recordCount += 1;
    } else {
      segments.push({
        phaseId: r.phaseId,
        pass: r.pass,
        firstSequence: r.sequence,
        lastSequence: r.sequence,
        recordCount: 1,
      });
    }
  }
  return {
    stream: "activity",
    latestSequence: records.length,
    recordCount: records.length,
    durableThroughSequence: records.length,
    byteLength: records.length * 120,
    segments,
  };
}

graduationRuns.push(
  makeStreamRun({
    id: "g-40",
    draftId: "d-40",
    state: "working",
    workTurns: 2,
    reviewTurns: 1,
    input: {
      draftId: "d-40",
      draftName: "Keep scroll position",
      prompt: "Keep the editor's scroll position when a tab is reopened.",
      promptChecksum: "sha-p40",
      capturedAt: "2026-09-06T09:00:00Z",
    },
    checkpoint: {
      pass: 2,
      changedPaths: [],
      hiddenPaths: [],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
    },
    observability: {
      observabilityVersion: 1,
      currentStage: "working",
      stageCondition: "active",
      queue: { waitReason: null, leftQueuedAt: "2026-09-06T09:02:00Z" },
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "2026-09-06T09:02:00Z", reason: "work_started" },
        { from: "working", to: "review", pass: 1, at: "2026-09-06T09:05:00Z", reason: "review_started" },
        { from: "review", to: "working", pass: 2, at: "2026-09-06T09:08:00Z", reason: "review_revision" },
      ],
      revisionHistory: [],
      passes: [
        { pass: 1, status: "failed", task: "Keep the scroll position.", findings: [], startedAt: "2026-09-06T09:02:00Z" },
        { pass: 2, status: "working", task: "Keep the scroll position.", findings: [], startedAt: "2026-09-06T09:08:00Z" },
      ],
    },
  }),
);
(graduationRuns[graduationRuns.length - 1] as any).logs.activity = graduationActivityIndex("g-40");

function syncGraduationLogIndex(runId: string) {
  const run: any = graduationRuns.find((r) => r.id === runId);
  if (run) run.logs.activity = graduationActivityIndex(runId);
}

(globalThis as Record<string, unknown>).__appendGraduationActivity = (
  runId: string,
  phaseId: string,
  pass: number | null,
  kind: string,
  summary: string,
) => {
  const records = (graduationActivity[runId] ??= []);
  const sequence = records.length + 1;
  records.push({ sequence, at: new Date().toISOString(), phaseId, pass, kind, summary });
  syncGraduationLogIndex(runId);
  (globalThis as any).__fireBusEvent?.("graduation-log-records-appended", {
    runId,
    stream: "activity",
    latestSequence: sequence,
  });
};

function readGraduationActivity(a: any) {
  const runId = String(a.runId ?? "");
  const phaseId = String(a.phaseId ?? "");
  const scope = a.pass ?? { kind: "phase" };
  findRun(runId);
  const limit = Math.min(Math.max(Number(a.limit ?? 50), 1), 200);
  const query = String(a.query ?? "").trim().toLowerCase();
  const all = graduationActivity[runId] ?? [];
  let inScope = all.filter(
    (r) =>
      r.phaseId === phaseId &&
      (scope.kind === "pass"
        ? r.pass === scope.pass
        : scope.kind === "run_level"
          ? r.pass === null
          : true),
  );
  if (query)
    inScope = inScope.filter((r) =>
      `${r.kind} ${r.summary}`.toLowerCase().includes(query),
    );
  const cursor = a.cursor ?? null;
  let slice: typeof inScope;
  let oldestReached: boolean;
  if (cursor?.direction === "after") {
    slice = inScope.filter((r) => r.sequence > cursor.sequence).slice(0, limit);
    oldestReached = true;
  } else if (cursor?.direction === "before") {
    const older = inScope.filter((r) => r.sequence < cursor.sequence);
    slice = older.slice(Math.max(older.length - limit, 0));
    oldestReached = older.length <= limit;
  } else {
    slice = inScope.slice(Math.max(inScope.length - limit, 0));
    oldestReached = inScope.length <= limit;
  }
  const first = slice[0];
  const last = slice[slice.length - 1];
  return {
    runId,
    stream: "activity",
    phaseId,
    scope,
    entries: slice.map((r) => ({
      record: { ...r, pass: r.pass },
      presentation: { runLevel: r.pass === null },
    })),
    nextCursor: last
      ? { runId, stream: "activity", direction: "after", sequence: last.sequence }
      : cursor?.direction === "after"
        ? cursor
        : null,
    olderCursor:
      first && !oldestReached
        ? { runId, stream: "activity", direction: "before", sequence: first.sequence }
        : null,
    matchedTotal: inScope.length,
    oldestReached,
    latestSequence: all.length,
    status: inScope.length === 0 && !query ? "empty" : "available",
    search: !query ? "not_requested" : inScope.length ? "matched" : "search_no_match",
    failure: null,
  };
}

/**
 * GRV-FR-13 /: the review in **in-place** mode, where the header
 * carries the warning, a sentence stands where Reject would be, and the
 * hand-off region is present exactly as it is in Git mode. The seeded in-place
 * run (`g-2`) is `queued`, so nothing in the shell can otherwise open its
 * review — this moves it to `awaiting_review` with a manifest of its own and
 * fires the events every graduation surface re-reads on. From the console:
 *
 *   window.__mockGraduationInPlaceReview()
 */
(globalThis as Record<string, unknown>).__mockGraduationInPlaceReview = () => {
  const run = graduationRuns.find((r) => r.id === "g-2");
  if (!run) return null;
  run.state = "awaiting_review";
  run.iteration = 1;
  run.manifest = {
    computedAt: "2026-08-14T09:20:00Z",
    // GCM-FR-20: in place the baseline is descriptive and never a rollback
    // source, so it names when the run started rather than a revision.
    baseline: { kind: "in_place_run_start", capturedAt: "2026-08-14T09:05:00Z" },
    entries: [
      {
        path: "specifications/core/REG-registry.md",
        operation: "updated",
        baselineRevision: null,
        classification: "amendment",
        reason: "The prompt asks for the registry to move off the flat file.",
        failures: [],
      },
      {
        path: "specifications/core/REM-registry-migration.md",
        operation: "created",
        baselineRevision: null,
        classification: "new specification",
        reason: "The prompt asks for the move itself to be written down.",
        failures: [],
      },
    ],
    passed: true,
    summary: "Two specification paths, written in place.",
  };
  run.handoffPrompt =
    "Implement the registry migration described in " +
    "specifications/core/REM-registry-migration.md. The specification is the " +
    "source of truth; read it before writing anything.";
  return graduationChanged(run);
};

/**
 * GRU-FR-ZMHB: the time a run act (pause, continue, revert, discard, restart)
 * takes to answer, so the row can be read while the act is in flight. Asked for
 * on the URL: http://localhost:5199/?graduationActDelay=1500
 */
function graduationActDelay(): Promise<void> {
  const ms = Number(harnessValue("graduationActDelay")) || 0;
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

/** GRD-FR-EFAU: the two events every graduation surface re-reads on. */
function graduationChanged(run: any): any {
  run.updatedAt = new Date().toISOString();
  // GRD-FR-53: the stage is a function of the state, so a state change moves
  // the stage row too. Recomputed rather than written per transition, so the
  // two cannot disagree.
  applyObservability(run);
  fireBus("graduation-run-changed", { runId: run.id, state: run.state });
  // GRD-FR-EFAU: **every** queue-change event names a queue, so a consumer is
  // never told only that something in the project changed.
  fireBus("graduation-queue-changed", {
    projectKey: "/Users/demo/dev/acme",
    queue: run.queue ?? (IMPLEMENTATION_PART.includes(run.state)
      ? "implementation"
      : "graduation"),
  });
  return { ...run };
}

/**
 * GRD-FR-72: the implementation queue releasing a run, which is what turns an
 * `implementation_queued` run into a working one. The harness has no loop, so
 * the release is a console act. From the console:
 *
 *   window.__mockReleaseImplementation("g-5")
 */
(globalThis as Record<string, unknown>).__mockReleaseImplementation = (
  runId: string,
) => {
  const run = graduationRuns.find((r) => r.id === runId);
  if (!run || run.state !== "implementation_queued") return null;
  run.state = "implementing";
  run.implementationIteration = (run.implementationIteration ?? 0) + 1;
  run.implementationSource = implementationSource(run.id);
  run.implementationManifest = {
    kind: "implementation",
    computedAt: new Date().toISOString(),
    baseline: { kind: "git_revision", revision: SPECIFICATION_REVISION },
    entries: [implementationEntry("src/notifications/address.ts", "created")],
    passed: true,
    summary: "One created.",
  };
  return graduationChanged(run);
};

/**
 * Every command the frontend issues, newest last and capped, exposed as
 * `window.__invokeLog`. A Playwright check can then assert what an interaction
 * did *not* call — "clicking away made no second rename call" is otherwise
 * invisible from the page, since a refused call changes nothing either.
 */
const invokeLog: Array<{ cmd: string; args: Record<string, any> }> = [];
(globalThis as Record<string, unknown>).__invokeLog = invokeLog;


/* --- GitHub publication (GHP-github-publication.md, surfaced by
   `NAW-new-artifact.md` NAW-FR-CBUJ … NAW-FR-HZSW) -------------------------

   The New Artifact tab reads ONE view per draft — the record that stands, the
   attempt that stands, and whether the action is offered — and the publication
   tag states whichever condition holds. The four conditions are seeded on
   different drafts so each is reachable without a mutation, and any of them can
   be forced onto whichever draft is open:

     ?publication=published|publishing|choice|none
     ?publicationRetryFails — the retry answers a typed error, which is what
       puts the tag into `Publish failed` and the error into the band.
*/

type MockPublicationRecord = {
  provider: string;
  repositoryOwner: string;
  repositoryName: string;
  issueNumber: number;
  issueUrl: string;
  publishedAt: string;
  marker: string;
};

type MockPublicationAttempt = {
  marker: string;
  remoteName: string;
  remoteUrl: string;
  repositoryOwner: string;
  repositoryName: string;
  state: "open" | "awaiting_choice";
  startedAt: string;
  updatedAt: string;
};

type MockPublicationState = {
  current: MockPublicationRecord | null;
  history: MockPublicationRecord[];
  attempt: MockPublicationAttempt | null;
};

const publicationRecord = (
  issueNumber: number,
  publishedAt: string,
): MockPublicationRecord => ({
  provider: "github",
  repositoryOwner: "acme",
  repositoryName: "acme-app",
  issueNumber,
  issueUrl: `https://github.com/acme/acme-app/issues/${issueNumber}`,
  publishedAt,
  marker: `synthesis-draft:${issueNumber}`,
});

const publicationAttempt = (
  state: "open" | "awaiting_choice",
): MockPublicationAttempt => ({
  marker: "synthesis-attempt:9f2c",
  remoteName: "origin",
  remoteUrl: "git@github.com:acme/acme-app.git",
  repositoryOwner: "acme",
  repositoryName: "acme-app",
  state,
  startedAt: "2026-08-18T10:12:00Z",
  updatedAt: "2026-08-18T10:12:40Z",
});

/**
 * GHP-FR-JAWD / GHP-FR-RUYT: what each seeded draft's publication stands at.
 * `d-1` is published, `d-4` holds an attempt in flight, and `d-7` holds one
 * waiting on a recovery choice — so the tag's three unforced conditions are all
 * on screen somewhere in the seeded project.
 */
const publications: Record<string, MockPublicationState> = {
  "d-1": {
    current: publicationRecord(418, "2026-08-14T16:22:00Z"),
    history: [
      publicationRecord(418, "2026-08-14T16:22:00Z"),
      publicationRecord(402, "2026-08-02T09:05:00Z"),
    ],
    attempt: null,
  },
  "d-4": { current: null, history: [], attempt: publicationAttempt("open") },
  "d-7": {
    current: null,
    history: [],
    attempt: publicationAttempt("awaiting_choice"),
  },
};

/**
 * The state a draft starts at, which a `?publication=` flag overrides for every
 * draft. Applied ONCE per draft: the store is mutable, so a flag that rebuilt
 * it on every read would throw away the attempt a publish had just written.
 */
const publicationSeed = (draftId: string): MockPublicationState => {
  switch (harnessValue("publication")) {
    case "published":
      return {
        current: publicationRecord(418, "2026-08-14T16:22:00Z"),
        history: [publicationRecord(418, "2026-08-14T16:22:00Z")],
        attempt: null,
      };
    case "publishing":
      return { current: null, history: [], attempt: publicationAttempt("open") };
    case "choice":
      return {
        current: null,
        history: [],
        attempt: publicationAttempt("awaiting_choice"),
      };
    case "none":
      return { current: null, history: [], attempt: null };
    default:
      return (
        publications[draftId] ?? { current: null, history: [], attempt: null }
      );
  }
};

const publicationSeeded = new Set<string>();

const publicationState = (draftId: string): MockPublicationState => {
  if (!publicationSeeded.has(draftId)) {
    publicationSeeded.add(draftId);
    publications[draftId] = publicationSeed(draftId);
  }
  return publications[draftId];
};

/**
 * GHP-FR-CWTG: whether the action is offered, and the exact reason where it is
 * not. An archived draft is the one refusal the seeded project reaches; the
 * graduation lock is the tab's own, read from `get_draft_graduation`.
 */
function publicationView(draftId: string): any {
  const state = publicationState(draftId);
  const row: any = (drafts as any[]).find((d) => d.id === draftId);
  const archived = row?.status === "archived";
  return {
    current: state.current,
    history: state.history,
    attempt: state.attempt,
    eligibility: archived
      ? {
          publishable: false,
          reasonCode: "draft_archived",
          reason: "An archived draft cannot be published.",
          localAssets: [],
        }
      : {
          publishable: true,
          reasonCode: null,
          reason: null,
          localAssets: [],
        },
  };
}

/** GHP-FR-WKDE: every configured remote, classified, plus the standing choice. */
/** GHP-FR-KVRH: the project's publication settings, at their defaults. */
let publicationSettings: {
  parentIssueTypes: string[];
  subIssueType: string;
  subIssueMilestonePolicy: string;
} = {
  parentIssueTypes: ["Feature"],
  subIssueType: "Task",
  subIssueMilestonePolicy: "inherit_parent",
};

const publicationRemotes = {
  remotes: [
    {
      name: "origin",
      url: "git@github.com:acme/acme-app.git",
      kind: "github",
      repositoryOwner: "acme",
      repositoryName: "acme-app",
      eligibility: "eligible",
      reason: null,
    },
    {
      name: "mirror",
      url: "git@gitlab.com:acme/acme-app.git",
      kind: "other",
      repositoryOwner: null,
      repositoryName: null,
      eligibility: "not_github",
      reason: "This remote is not a GitHub repository.",
    },
  ],
  selection: "origin",
  origin: "persisted",
  persistedChoice: { name: "origin", url: "git@github.com:acme/acme-app.git" },
};

/** NAW-FR-TSQE: the tab follows this event rather than polling. */
function publicationChanged(draftId: string): void {
  queueMicrotask(() => {
    const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((name: string, payload?: unknown) => void)
      | undefined;
    fire?.("draft-publication-changed", { draftId });
  });
}

/**
 * GHP-FR-HRUN: the issue an `awaiting_choice` attempt's marker was found on.
 * One number, so a recovery answered with `update_existing` can be checked to
 * keep it.
 */
const PUBLICATION_RECOVERY_ISSUE = 431;

/**
 * GHP-FR-JAWD: an attempt that reached GitHub becomes a record.
 *
 * `issueNumber` names the issue the record is for. Omitted, a new issue is
 * created — which is what an ordinary publication and a `publish_new` recovery
 * both do; given, the found issue is updated and keeps its number.
 */
function publicationSettle(draftId: string, issueNumber?: number): any {
  const state = publicationState(draftId);
  const record = publicationRecord(
    issueNumber ?? 500 + Math.floor(Math.random() * 400),
    new Date().toISOString(),
  );
  if (issueNumber && state.attempt) record.marker = state.attempt.marker;
  state.current = record;
  state.history = [record, ...state.history];
  state.attempt = null;
  publicationChanged(draftId);
  return { kind: "published", record };
}

/**
 * The app's `invoke`: the unified discussion contract in front of the legacy
 * switch below (see `mock-unified-discussions.ts`).
 */
export async function invoke(cmd: string, args?: Record<string, any>): Promise<any> {
  return unifiedInvoke(cmd, args, legacyInvoke);
}

async function legacyInvoke(cmd: string, args?: Record<string, any>): Promise<any> {
  const a = args ?? {};
  invokeLog.push({ cmd, args: a });
  if (invokeLog.length > 500) invokeLog.shift();
  // GPP-github-polling.md: the polling commands live in their own module.
  const polled = githubPollingInvoke(cmd, a, (id, name, issue) => {
    const at = new Date().toISOString();
    (drafts as any[]).push({ id, name, status: "github_shadow", folder: "", githubIssue: issue, updatedAt: at });
    draftRecords[id] = {
      id,
      name,
      promptPath: `${name}.md`,
      status: "github_shadow",
      githubIssue: issue,
      createdAt: at,
      updatedAt: at,
    };
    draftFiles[id] = [`${name}.md`];
    queueMicrotask(() => {
      const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
        | ((name: string, payload?: unknown) => void)
        | undefined;
      fire?.("drafts-changed", {});
    });
  });
  if (polled !== NOT_HANDLED) return polled;
  // DCL-documents-collection.md: the Documents commands live in their own module.
  const documents = documentsInvoke(cmd, a);
  if (documents !== NOT_HANDLED) return documents;
  // GIT-git.md: the Git panel's history, branch and pull request commands.
  const gitPanel = gitPanelInvoke(cmd, a);
  if (gitPanel !== NOT_HANDLED) return gitPanel;
  switch (cmd) {
    // ---- project ---------------------------------------------------------
    case "list_recent_projects":
      return [
        {
          name: "acme",
          path: "/Users/demo/dev/acme",
          lastOpenedAt: "2026-07-28T09:00:00Z",
          pinned: true,
        },
        {
          name: "synthesis",
          path: "/Users/demo/develop/synthesis-june",
          lastOpenedAt: "2026-07-24T18:30:00Z",
        },
        {
          name: "old-thing",
          path: "/Users/demo/dev/old-thing",
          lastOpenedAt: "2026-03-01T12:00:00Z",
          missing: true,
        },
      ];
    case "open_project_at_path":
    case "open_project_from_git_url":
    case "create_project":
      return {
        name: "acme",
        path: "/Users/demo/dev/acme",
        activeWorktreePath: worktreeContext.activeWorktreePath,
      };
    case "close_project":
    case "finish_exit":
    case "set_save_menu_state":
    case "set_find_menu_state":
    case "remove_recent_project":
    case "clear_recent_projects":
    case "pin_recent_project":
    case "unpin_recent_project":
    case "center_picker":
      return undefined;

    // ---- settings child windows (SWN-settings-windows.md) ----------------
    //
    // The two settings surfaces are native child windows of the application
    // (SWN-FR-01), which a browser harness cannot create: their frame, their
    // fixed size, their modality, and the one-window rule are all the Rust
    // side's. What IS verifiable here is the window's BODY, because all three
    // windows boot one entry point — navigate to `/?settings=global` or
    // `/?settings=project` (optionally with `&section=`) and `main.tsx` renders
    // the settings window's root instead of the shell.
    //
    // So these three record the request rather than performing it: opening a
    // window from the shell is a no-op the harness reports through
    // `window.__settingsWindowRequests`, which is what lets a verification
    // assert that a control asked for the right window and section.
    case "open_settings_window":
      (
        (globalThis as Record<string, unknown>).__settingsWindowRequests as
          | unknown[]
          | undefined ??
        ((globalThis as Record<string, unknown>).__settingsWindowRequests = [])
      ).push({ kind: a.kind, section: a.section ?? null });
      return undefined;
    case "finish_settings_close":
      return undefined;
    case "get_settings_window_context":
      // SWN-FR-02: the open project behind the window, fixed for its whole
      // life. The harness's demo project, so the sections that name a project
      // — the GitHub token picker's title, the Notifications rehearsal's
      // address — have a real one.
      return {
        projectName: "acme",
        projectKey: "~/dev/acme",
        contentRoot: "~/dev/acme",
      };

    // ---- preferences -----------------------------------------------------
    case "load_app_preferences":
      return prefs;
    case "save_app_preferences":
      prefs = a.preferences;
      return undefined;
    // FNT-FR-01 / FNT-FR-02: the installed families the Appearance typography
    // controls offer. A fixed set standing in for the machine's own, carrying
    // both widths so the Source code role's grouping (GLS-FR-18) is
    // exercisable, and ordered case-insensitively as FNT-FR-03 returns them.
    case "list_system_fonts":
      return [
        { family: "Courier New", monospace: true },
        { family: "Fira Code", monospace: true },
        { family: "Georgia", monospace: false },
        { family: "Helvetica Neue", monospace: false },
        { family: "IBM Plex Sans", monospace: false },
        { family: "Menlo", monospace: true },
        { family: "Times New Roman", monospace: false },
      ];
    // ---- OS notifications (NTD-notification-delivery.md) -----------------
    // No notification centre exists behind a browser tab, so posting records
    // into `window.__notificationCentre` instead. Activation is driven from the
    // test side with `window.__fireBusEvent("notification-activated", …)`.
    case "get_notification_permission":
      return notificationPermission;
    // NTD-FR-03 / GLS-FR-26: asking grants, unless the harness was started with
    // `?refuse`, which is how the denied-after-asking branch is reached.
    case "request_notification_permission":
      notificationPermission = harnessFlag("refuse") ? "denied" : "granted";
      return notificationPermission;
    // NTD-FR-06: keyed, so a second post with the same key replaces the first.
    case "post_notification": {
      const req = a.request ?? {};
      const existing = notificationCentre.get(req.key);
      const id = existing?.id ?? `ntf-${++notificationSeq}`;
      notificationCentre.set(req.key, {
        id,
        key: req.key,
        title: req.title,
        body: req.body,
        payload: req.payload,
      });
      return { id };
    }
    case "withdraw_notification": {
      for (const [key, n] of notificationCentre) {
        if (n.id === a.id) notificationCentre.delete(key);
      }
      return undefined;
    }
    case "withdraw_all_notifications":
      notificationCentre.clear();
      return undefined;

    case "load_layout_preferences":
      return layout;
    case "save_layout_preferences":
      layout = a.preferences;
      return undefined;
    case "load_project_config":
      return projectConfig;
    // PSS-FR-17: a whole-store write that carries every section the payload
    // does not name through unchanged — which is what lets the status bar
    // persist a convention without disturbing a configured template, and the
    // Draft template section persist a template without disturbing the
    // convention. PSS-FR-21: an empty template CLEARS the key rather than
    // storing `""`, so unset and configured stay distinct.
    case "save_project_config": {
      if (projectConfigSaveRefusal) throw projectConfigSaveRefusal;
      const patch = a.config ?? {};
      projectConfig = {
        lineEndings: patch.lineEndings ?? projectConfig.lineEndings,
        draftTemplate:
          patch.draftTemplate === undefined
            ? projectConfig.draftTemplate
            : patch.draftTemplate === ""
              ? null
              : String(patch.draftTemplate),
        // PSS-FR-ZVSD: a payload that names no limit leaves it as it stands.
        graduationConcurrencyLimit:
          patch.graduationConcurrencyLimit ?? projectConfig.graduationConcurrencyLimit,
        executionTimeoutMs:
          patch.executionTimeoutMs === undefined
            ? projectConfig.executionTimeoutMs
            : patch.executionTimeoutMs,
      };
      return undefined;
    }

    // ---- plugins ---------------------------------------------------------
    case "list_installed_plugins":
      return [
        { id: "p-1", name: "Markdown lint", source: "github:acme/md-lint" },
        { id: "p-2", name: "Spec numbering", source: "/Users/demo/plugins/specnum" },
      ];
    case "list_agent_adapters":
      return [{ id: "a-1", name: "Claude adapter", source: "builtin" }];
    case "install_plugin":
      return { id: "p-3", name: "New plugin", source: a.source };
    case "install_adapter":
      return { id: "a-2", name: "New adapter", source: a.source };
    case "uninstall_plugin":
      return undefined;

    // ---- filesystem ------------------------------------------------------
    case "browse_for_folder":
    case "browse_for_file":
      return { selected: { path: "/Users/demo/dev/acme" } };
    // FSA-FR-16. `?cancelSave` returns the dismissal sentinel instead, so the
    // cancelled-dialog branch of LOG-FR-17 is reachable in the harness.
    case "browse_for_save_path":
      return harnessFlag("cancelSave")
        ? "cancelled"
        : { selected: { path: `/Users/demo/Desktop/${a.defaultName ?? "export"}` } };
    case "load_project_tree":
    case "rescan_project_tree":
    case "assign_artifact_type":
    case "clear_artifact_type":
    case "create_file":
    case "create_folder":
      return tree();
    // PST-FR-29 / NTA-new-typed-artifact.md: one empty file plus its file-scope
    // type assignment, as one transaction. The returned node carries the type
    // the caller chose with `type_source = "assigned"`, which is what routes the
    // new file to its natural surface and reveals it (NTA-FR-13). A name
    // already taken in the destination is the typed collision the window shows
    // inline (NTA-FR-16).
    case "create_typed_file": {
      const location = a.location ? String(a.location) : "";
      const path = location ? `${location}/${a.name}` : String(a.name);
      if (!a.artifactType) throw "an artifact type is required to create a typed file";
      if (findTreeNode(path) || createdTypedFiles[path])
        // eslint-disable-next-line no-throw-literal
        throw `already exists: ${path}`;
      const node = {
        id: path,
        name: String(a.name),
        path,
        nodeKind: "file",
        artifactType: String(a.artifactType),
        typeSource: "assigned",
      };
      createdTypedFiles[path] = node;
      // NTA-FR-11: zero bytes on disk, so a Flow opens on the empty graph its
      // empty body deserializes to (FLO-FR-04).
      artifactBodies[path] = { body: "", checksum: "sha-new" };
      // ASC-FR-10 / LIB-FR-10: the real backend's scanner notices the new file
      // and emits the (debounced) structural-change event, which is what makes
      // the Project panel reload and gives NTA-FR-13's reveal a row to land on.
      // Without it the panel keeps the tree it loaded at mount and the reveal
      // finds nothing — a harness gap that reads as a missing reveal.
      queueMicrotask(() => {
        const fire = (globalThis as Record<string, unknown>)
          .__fireBusEvent as ((n: string, p?: unknown) => void) | undefined;
        fire?.("project-tree-changed", { path });
      });
      return node;
    }
    // PST-FR-18 / LCM-FR-11: a non-recursive delete of a folder that still
    // holds entries removes nothing and returns the typed "not empty" error,
    // which is what raises the recursive-delete confirmation. `recursive = true`
    // succeeds. Files and empty folders succeed either way.
    case "delete_path": {
      const node = findTreeNode(String(a.path));
      const nonEmptyFolder =
        node?.nodeKind === "folder" && (node.children?.length ?? 0) > 0;
      if (nonEmptyFolder && !a.recursive) throw "directory not empty";
      return undefined;
    }
    // LCM-FR-02 / PST-FR-19: the rename lands in the tree the next scan serves,
    // and the (debounced) structural-change event is what makes the Project
    // panel go and get it. A name already taken beside the node is the typed
    // collision the panel surfaces inline; the backend rejects it the same way.
    case "rename_path": {
      const path = String(a.path);
      const newName = String(a.newName);
      const cut = path.lastIndexOf("/");
      const parentPath = cut === -1 ? "" : path.slice(0, cut);
      const parent = findTreeNode(parentPath);
      const taken = (parent?.children ?? []).some(
        (c: any) => c.path !== path && c.name === newName,
      );
      if (taken) throw `${newName} already exists`;
      renames.push({ path, newName });
      queueMicrotask(() => {
        const fire = (globalThis as Record<string, unknown>)
          .__fireBusEvent as ((n: string, p?: unknown) => void) | undefined;
        fire?.("project-tree-changed", { path });
      });
      return undefined;
    }
    case "copy_path_into_folder":
      return undefined;

    // ---- artifacts -------------------------------------------------------
    case "load_artifact_contents_by_id": {
      // PCR-FR-11: an artifact an acceptance rewrote reads as what it now holds,
      // with the checksum that write produced — so the session's reset adopts a
      // new baseline rather than the fixture's.
      const written = artifactBodies[String(a.id)];
      if (written !== undefined) return { ...written };
      // EDT-FR-73: a file whose name is not `.md` is a source file, and serving
      // it Markdown would hide the whole of what its surface does.
      const ext = String(a.id).split("/").pop()?.split(".").pop() ?? "";
      const source = SOURCE_BODIES[ext.toLowerCase()];
      return {
        body: String(a.id).endsWith(".flow.md")
          ? FLOW_BODY
          : source !== undefined
            ? source
            : String(a.id).endsWith("EDT-editor.md")
              ? MD_BODY_FM
              : String(a.id).endsWith("FID-fidelity.md")
                ? MD_BODY_FIDELITY
                : String(a.id).endsWith("SKILL.md")
                  ? MD_BODY_SKILL
                  : MD_BODY,
        checksum: "sha-1",
      };
    }
    case "open_artifact_by_id":
      return {
        key: a.id,
        kind: String(a.id).endsWith(".flow.md")
          ? "flow"
          : String(a.id).endsWith(".md")
            ? "markdown"
            : "text",
      };
    case "save_artifact_contents":
      return { checksum: "sha-2" };
    /**
     * NAW-FR-13 / NAW-FR-36: the author's own typing settles into the live
     * prompt and puts NO row in the History rail. It does move the live digest,
     * which is what makes the live row report "Modified since …" (NAW-FR-37).
     */
    case "save_draft_file_contents": {
      const id = String(a.id ?? "d-1");
      const body = String(a.body ?? "");
      draftBodies[String(a.path ?? "")] = body;
      liveDigests[id] = {
        sha256: `sha-live-${nextLiveDigest++}`,
        byteLen: body.length,
      };
      return { checksum: "sha-2" };
    }
    /**
     * FGV-FR-02 (`FGV-flow-graph-validation.md`): the backend's verdict on a
     * Flow body, which the Flow tab asks for on load (FLO-FR-46) and before a
     * write (FLO-FR-47).
     *
     * A cut-down stand-in for the Rust validator: it covers the rules a browser
     * pass can actually reach — the body parses, claims version 1, and joins
     * only elements that share a container — so the canvas's error state is
     * reachable here without a second copy of the whole rule set.
     */
    case "validate_flow_document":
      return validateFlow(String(a.body ?? ""));
    // ---- Dashboard widget loaders (PST-FR-31 .. PST-FR-34) ---------------
    // Each loader applies its own five-item limit and its own ordering; the
    // Dashboard renders what it is given (DSH-FR-09, DSH-FR-11, DSH-FR-13,
    // DSH-FR-14). There is no recently-edited MRU behind any of it — the order
    // below is the one a filesystem read would return (PSS-FR-12).
    case "list_recently_edited_artifacts":
      return dashboardAnswer("recently_edited", [
        {
          id: "specifications/ui/LIB-library.md",
          name: "LIB-library.md",
          kind: "markdown",
          modifiedAt: "2026-05-15T11:20:00.000Z",
        },
        {
          id: "flows/release.flow.md",
          name: "release.flow.md",
          kind: "flow",
          modifiedAt: "2026-05-15T10:05:00.000Z",
        },
        {
          id: "skills/analyst/SKILL.md",
          name: "analyst",
          kind: "markdown",
          modifiedAt: "2026-05-14T16:41:00.000Z",
        },
      ]);
    // DSH-FR-11: the project's `active` drafts, ordered by prompt-file
    // activity. Served from the seeded drafts so activating a row opens the
    // draft that actually exists here (DSH-FR-12).
    case "list_active_drafts":
      return dashboardAnswer(
        "active_drafts",
        (drafts as any[])
          .filter((d) => d.status === "active")
          .slice(0, 5)
          .map((d, i) => ({
            draftId: d.id,
            name: d.name,
            status: "active",
            activityAt: `2026-05-15T1${9 - i}:00:00.000Z`,
          })),
      );
    // DSH-FR-13: the five most recently updated runs, a `queued` one among
    // them where its update instant places it.
    case "list_recent_agent_runs":
      return dashboardAnswer("agent_activity", [
        {
          runId: "run-7e3",
          draftId: (drafts as any[])[0]?.id ?? "d1",
          draftName: (drafts as any[])[0]?.name ?? "checkout-v2",
          state: "running",
          stage: "authoring",
          stageCondition: "active",
          updatedAt: "2026-05-15T11:30:00.000Z",
        },
        {
          runId: "run-4a1",
          draftId: (drafts as any[])[1]?.id ?? "d2",
          draftName: (drafts as any[])[1]?.name ?? "onboarding",
          state: "queued",
          stage: "queued",
          stageCondition: "waiting",
          queuePosition: 1,
          updatedAt: "2026-05-15T11:05:00.000Z",
        },
        {
          runId: "run-2c9",
          draftId: "d-gone",
          draftName: undefined,
          state: "failed",
          stage: "validation",
          stageCondition: "stopped",
          updatedAt: "2026-05-14T09:12:00.000Z",
        },
      ]);
    // DSH-FR-14: four counts. `fetchableCommits` is deliberately `null` — the
    // branch has nothing to be behind — so the unavailable rendering is
    // reachable in the browser rather than only in a unit test.
    case "list_pending_git_activity":
      return dashboardAnswer("pending_git", {
        modifiedArtifacts: 3,
        modifiedSourceFiles: 4,
        unpushedCommits: 2,
        fetchableCommits: null,
      });
    case "list_due_reminders":
    case "list_project_health_signals":
      return [];

    // ---- drafts ----------------------------------------------------------
    // DRS-FR-08 / DRP-FR-20: one call carries the whole organisation — every
    // folder at every depth, empty ones included, and every draft with the
    // folder it is filed in. The panel renders the tree from this alone.
    case "list_drafts":
      // DRP-FR-19: the flag follows what the proposals actually say, so
      // deciding one clears the row's marker.
      for (const d of drafts as any[]) {
        d.hasPendingProposal = (proposalsByDraft[d.id] ?? []).some(
          (p) => p.state === "pending",
        );
        // DRS-FR-18 / DRP-FR-35: the row's graduation comes from the queue and
        // is never cached on the draft, so a row shows what the queue holds.
        d.graduation = draftGraduation(d.id);
      }
      return {
        // DRP-FR-21 orders folders in the panel, not here; the backend returns
        // them in whatever order the directory walk found, so the seed is
        // deliberately not pre-sorted.
        folders: draftFolders.map((f) => ({ ...f })),
        // DRS-FR-08: most-recent-activity first.
        drafts: [...(drafts as any[])].sort((a, b) =>
          String(b.updatedAt).localeCompare(String(a.updatedAt)),
        ),
      };

    // ---- the organisation of the drafts root (DRS-FR-29 … DRS-FR-33) -----
    //
    // A folder's identity is its path, so every one of these rewrites the paths
    // of the moved subtree and of the drafts filed beneath it, exactly as
    // moving a directory on disk would.
    case "create_drafts_folder": {
      const parent = String(a.parent ?? "");
      const name = String(a.name ?? "");
      const path = parent === "" ? name : `${parent}/${name}`;
      if (draftFolders.some((f) => f.path.toLowerCase() === path.toLowerCase()))
        // eslint-disable-next-line no-throw-literal
        throw `a folder called “${name}” is already here`;
      const folder = { path, parent };
      draftFolders.push(folder);
      return folder;
    }
    case "rename_drafts_folder": {
      const path = String(a.path ?? "");
      const name = String(a.name ?? "");
      const folder = draftFolders.find((f) => f.path === path);
      if (!folder)
        // eslint-disable-next-line no-throw-literal
        throw `no such folder: ${path}`;
      const renamed = folder.parent === "" ? name : `${folder.parent}/${name}`;
      if (
        renamed.toLowerCase() !== path.toLowerCase() &&
        draftFolders.some((f) => f.path.toLowerCase() === renamed.toLowerCase())
      )
        // eslint-disable-next-line no-throw-literal
        throw `a folder called “${name}” is already here`;
      reparentDraftsSubtree(path, renamed);
      return { path: renamed, parent: folder.parent };
    }
    // DRS-FR-31: the direct children move up into the deleted folder's own
    // parent, subtrees whole; nothing is deleted but the emptied folder itself.
    case "delete_drafts_folder": {
      const path = String(a.path ?? "");
      const folder = draftFolders.find((f) => f.path === path);
      if (!folder)
        // eslint-disable-next-line no-throw-literal
        throw `no such folder: ${path}`;
      const parent = folder.parent;
      for (const child of draftFolders.filter((f) => f.parent === path)) {
        const name = child.path.slice(path.length + 1);
        reparentDraftsSubtree(child.path, parent === "" ? name : `${parent}/${name}`);
      }
      for (const row of drafts as any[]) if (row.folder === path) row.folder = parent;
      draftFolders.splice(
        draftFolders.findIndex((f) => f.path === path),
        1,
      );
      return undefined;
    }
    // DRS-FR-32: changes exactly one thing about the draft — where it sits.
    case "move_draft_to_folder": {
      const row: any = (drafts as any[]).find((d) => d.id === a.draftId);
      if (!row)
        // eslint-disable-next-line no-throw-literal
        throw `no such draft: ${a.draftId}`;
      row.folder = String(a.folder ?? "");
      const record: any = draftRecords[String(a.draftId)];
      return record ?? row;
    }
    // DRS-FR-33: the folder moves with its whole subtree, and the drafts
    // beneath it keep their ids and their files.
    case "move_drafts_folder": {
      const path = String(a.path ?? "");
      const destination = String(a.destination ?? "");
      const folder = draftFolders.find((f) => f.path === path);
      if (!folder)
        // eslint-disable-next-line no-throw-literal
        throw `no such folder: ${path}`;
      const name = path.slice(path.lastIndexOf("/") + 1);
      const moved = destination === "" ? name : `${destination}/${name}`;
      if (draftFolders.some((f) => f.path.toLowerCase() === moved.toLowerCase()))
        // eslint-disable-next-line no-throw-literal
        throw `a folder called “${name}” is already there`;
      reparentDraftsSubtree(path, moved);
      return { path: moved, parent: destination };
    }
    // DRP-FR-13 / DRP-FR-17: the query is matched against each draft's name and
    // its files, and reports which of the two it was found in — so a query that
    // matches nothing narrows the list to nothing (SNV-FR-61) rather than
    // returning a standing hit whatever was typed.
    case "search_drafts": {
      const query = String(a.query ?? a.text ?? "").toLowerCase();
      if (query === "") return [];
      const hits: { draftId: string; matchedIn: string }[] = [];
      for (const row of drafts as any[]) {
        if (row.name.toLowerCase().includes(query)) {
          hits.push({ draftId: row.id, matchedIn: "name" });
          continue;
        }
        // The seeded drafts share one body (`MD_BODY`), so a draft's files stand
        // in for its contents here: a query found in a file's path is reported
        // as a contents hit, which is the annotation DRP-FR-17 renders.
        const inFiles = (draftFiles[row.id] ?? []).some((f) =>
          f.toLowerCase().includes(query),
        );
        if (inFiles) hits.push({ draftId: row.id, matchedIn: "contents" });
      }
      return hits;
    }
    // DRS-FR-06 / DRS-FR-27: an unnamed draft is `Untitled`, or the first
    // `Untitled N` free in the worktree, and it is created holding the one file
    // of that name.
    case "create_draft": {
      const taken = new Set(Object.values(draftRecords).map((r: any) => r.name));
      let name = a.name?.trim() || "Untitled";
      if (!a.name?.trim()) {
        for (let n = 2; taken.has(name); n += 1) name = `Untitled ${n}`;
      }
      const id = `d-${nextDraftId++}`;
      const file = primaryFileName(name);
      const now = new Date().toISOString();
      const record = {
        id,
        name,
        promptPath: file,
        status: "active",
        destinationRoot: a.destinationRoot,
        createdAt: now,
        updatedAt: now,
      };
      draftRecords[id] = record;
      draftFiles[id] = [file];
      // DRS-FR-39: the draft is created holding the project's template where
      // one is configured, and nothing at all where the store reports it unset.
      // A snapshot, taken once here: a template later changed reaches no draft
      // that already exists.
      if (projectConfig.draftTemplate !== null)
        draftBodies[file] = projectConfig.draftTemplate;
      // DHS-FR-07 / NAW-FR-36: creating a draft settles NO version. The live
      // prompt is the `Original` until a change is accepted against it, so a
      // freshly created draft opens on an empty history.
      draftHistories[id] = [];
      // DRP-FR-26 / DRS-FR-07: `folder` files the draft; `destinationRoot` is a
      // different question and neither is derived from the other.
      drafts.unshift({
        id,
        name,
        status: "active",
        folder: String(a.folder ?? ""),
        updatedAt: now,
      } as any);
      return { draft: record, file };
    }
    // DRS-FR-15 / NAW-FR-41: a draft whose storage is not the single prompt is
    // refused rather than opened on a guess, and the tab renders its blocked
    // state from exactly this rejection.
    case "open_draft": {
      const record: any = draftRecords[a.id] ?? draftRecords["d-1"];
      // eslint-disable-next-line no-throw-literal
      if (record.inconsistent) throw "draft_not_single_file";
      return record;
    }
    // DRS-FR-09 / DRS-FR-25: the rename carries the primary file with it, and a
    // name whose file is taken is refused whole (DRS-FR-26).
    case "rename_draft": {
      const record: any = draftRecords[a.id] ?? draftRecords["d-1"];
      const files = draftFiles[record.id] ?? [];
      const primary: string | null = record.promptPath ?? null;
      if (primary) {
        const cut = primary.lastIndexOf("/");
        const parent = cut < 0 ? "" : primary.slice(0, cut + 1);
        const target = `${parent}${primaryFileName(a.name)}`;
        if (target !== primary) {
          // A Tauri command's `Err(String)` reaches the frontend as a rejected
          // string, not an Error, so the refusal the UI renders reads exactly as
          // it does in the shipped app rather than carrying an `Error:` prefix.
          if (takenBySomeoneElse(files, target, primary))
            // eslint-disable-next-line no-throw-literal
            throw `already exists in this draft: ${target}`;
          draftFiles[record.id] = files.map((f) => (f === primary ? target : f));
          record.promptPath = target;
        }
      }
      record.name = a.name;
      const row: any = drafts.find((d: any) => d.id === record.id);
      if (row) row.name = a.name;
      return record;
    }
    // DRS-FR-10: the status is stored on the record, so a later `open_draft` or
    // `rename_draft` reports the status the draft actually carries rather than
    // resetting the tab's chrome to `active`.
    case "set_draft_status": {
      const record: any = draftRecords[a.id] ?? draftRecords["d-1"];
      record.status = a.status;
      const row: any = drafts.find((d: any) => d.id === record.id);
      if (row) row.status = a.status;
      return record;
    }
    // DRP-FR-12: the draft and its record go, so a worktree whose drafts are
    // all deleted reaches the empty state (DRP-FR-15 / SNV-FR-60) rather than
    // re-listing rows the backend no longer holds.
    case "delete_draft": {
      const at = drafts.findIndex((d: any) => d.id === a.id);
      if (at >= 0) drafts.splice(at, 1);
      delete draftRecords[a.id as string];
      delete draftFiles[a.id as string];
      delete draftHistories[a.id as string];
      delete liveDigests[a.id as string];
      return undefined;
    }
    /**
     * NAW-FR-40 / DHS-FR-10: the rail's list — every settled version oldest
     * first, plus the live prompt's own digest. It reads NO snapshot payload, so
     * a draft carrying fifty versions costs the tab what one carrying a single
     * version costs; that is why `content` is stripped here and served only by
     * `load_draft_history_entry`.
     */
    case "list_draft_history": {
      const id = String(a.draftId ?? a.id ?? "d-1");
      // DHS-FR-21 / NAW-FR-41: a history that could not be reconciled, so the
      // rail's recoverable state and its retry are reachable:
      //   http://localhost:5199/?historyRecoveryFails
      // eslint-disable-next-line no-throw-literal
      if (harnessFlag("historyRecoveryFails")) throw "history_recovery_failed";
      // NAW-FR-39: a draft with more versions than the rail is tall, so whether
      // the live-prompt row is really pinned at the rail's head is measurable
      // rather than assumed:
      //   http://localhost:5199/?manyVersions
      const list = (draftHistories[id] ??= []);
      if (harnessFlag("manyVersions") && list.length > 0 && list.length < 24) {
        const last = list[list.length - 1];
        while (list.length < 24) {
          list.push({
            ...last,
            id: `dh-pad-${id}-${list.length}`,
            seq: list.length + 1,
            createdAt: new Date(
              Date.parse(last.createdAt) + list.length * 3_600_000,
            ).toISOString(),
            sha256: `sha-hist-pad-${list.length}`,
          });
        }
      }
      const entries = (draftHistories[id] ?? []).map(
        ({ content: _content, ...row }) => row,
      );
      return { entries, live: liveOf(id) };
    }
    /**
     * NAW-FR-40 / DHS-FR-11: one version's text, together with the digest it was
     * verified against. Invoked when a version is actually selected and at no
     * other moment.
     */
    case "load_draft_history_entry": {
      const wanted = String(a.entryId ?? "");
      // DHS-FR-12 / NAW-FR-41: a snapshot whose bytes do not match its recorded
      // checksum, so the per-version failure state is reachable:
      //   http://localhost:5199/?snapshotCorrupt
      // eslint-disable-next-line no-throw-literal
      if (harnessFlag("snapshotCorrupt")) throw "snapshot_corrupt";
      for (const list of Object.values(draftHistories)) {
        const found = list.find((e) => e.id === wanted);
        if (found) return { content: found.content, sha256: found.sha256 };
      }
      // eslint-disable-next-line no-throw-literal
      throw `no such history entry: ${wanted}`;
    }
    case "load_draft_file_contents": {
      // DCP-FR-11: a file an accepted proposal rewrote reads as the accepted
      // text from then on, which is what lets the tab show the change land.
      const rewritten = draftBodies[String(a.path ?? "")];
      if (rewritten !== undefined) return { body: rewritten, checksum: "sha-d2" };
      // DRS-FR-06: a draft created here holds one empty Markdown file named for
      // the draft, so a freshly created draft opens on an empty buffer the way
      // the real backend serves it. Existing seeded drafts keep a body, so both
      // shapes are reachable.
      return String(a.path ?? "").startsWith("Untitled")
        ? { body: "", checksum: "sha-d0" }
        : { body: MD_BODY, checksum: "sha-d1" };
    }
    /* --- draft image assets (DAS-draft-assets.md) ---------------------- */
    /* DAS-FR-05: store one pasted or dropped image under the draft's own
       `assets/` folder and answer with the Markdown destination to insert. A
       refusal answers with nothing at all, so no reference reaches the prompt
       (NAW-FR-51). */
    case "store_draft_image": {
      if (imageStoreDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, imageStoreDelayMs));
      }
      // eslint-disable-next-line no-throw-literal
      if (imageStoreRefusal) throw imageStoreRefusal;
      const mediaType = String(a.mediaType ?? "image/png");
      // DAS-FR-03: the media types the backend accepts, and nothing else.
      if (!["image/png", "image/jpeg", "image/gif", "image/webp"].includes(mediaType)) {
        // eslint-disable-next-line no-throw-literal
        throw "unsupported_media_type";
      }
      const draftId = String(a.id ?? a.draftId ?? "d-1");
      const ext = mediaType === "image/jpeg" ? "jpg" : mediaType.slice(6);
      const path = `assets/mock${nextAssetOrdinal++}.${ext}`;
      const data = String(a.data ?? DRAFT_ASSET_PNG);
      (draftAssets[draftId] ??= {})[path] = {
        mediaType,
        filename: (a.filename ?? null) as string | null,
        // The pasted bytes are kept, so what is drawn back is the picture that
        // went in rather than the seeded checkerboard.
        data,
        bytes: Math.floor((data.length * 3) / 4),
      };
      return {
        path,
        reference: `../${path}`,
        mediaType,
        filename: a.filename ?? null,
        bytes: Math.floor((data.length * 3) / 4),
      };
    }
    /* DAS-FR-08: the bytes of one asset this draft owns, base64-encoded. A
       destination the draft does not own is refused, and the surface renders
       its non-blocking placeholder for it (NAW-FR-52). */
    case "read_draft_image": {
      const draftId = String(a.id ?? a.draftId ?? "d-1");
      const path = draftAssetPath(String(a.path ?? ""));
      const asset = path ? draftAssets[draftId]?.[path] : undefined;
      // eslint-disable-next-line no-throw-literal
      if (!asset) throw "asset_not_found";
      return {
        mediaType: asset.mediaType,
        filename: asset.filename,
        data: asset.data,
      };
    }
    /* DAS-FR-09: take back a store whose reference never reached the saved
       prompt. Idempotent, and it retains an asset a saved reference names. */
    case "discard_draft_image": {
      const draftId = String(a.id ?? a.draftId ?? "d-1");
      const path = draftAssetPath(String(a.path ?? "")) ?? String(a.path ?? "");
      const promptPath = (draftRecords[draftId] as any)?.promptPath ?? "";
      const saved = draftBodies[promptPath] ?? "";
      if (saved.includes(`../${path}`)) return { discarded: false, retained: true };
      if (draftAssets[draftId]) delete draftAssets[draftId][path];
      return { discarded: true, retained: false };
    }
    /* DAS-FR-15 / NAW-FR-55: a **trigger and not a query** — it says a pass was
       scheduled and carries no account of what any pass found. Nothing in the
       tab reads it, so the harness answers the acknowledgement and no more. */
    case "sweep_draft_assets":
      return { scheduled: true, coalesced: false };
    /* DSS-FR-RIDW / DFI-FR-XKRM: one settled foreground editing interval, sent
       by the draft observer rather than by a surface. Fire and forget — nothing
       reads what it answers — so the harness records nothing and acknowledges. */
    case "record_draft_editing_interval":
      return undefined;
    case "graduate_draft":
      return { written: (a.destinations ?? []).map((d: any) => d.destination) };

    // ---- graduation (GRD-graduation.md / GRU / GRV) ----------------------
    //
    // The thirteen commands of `src/api.ts`'s graduation block. The queue is
    // seeded above; these move runs through it and fire the two events every
    // graduation surface re-reads on (GRD-FR-EFAU), so a state change made in one
    // surface shows up in the others without a reload.
    // Copied on the way out, exactly as a deserialized IPC payload would be:
    // handing the surface the live objects makes `setState` bail on identity
    // and a re-list appear to do nothing.
    // ---- work streams (WKS-work-streams.md) ------------------------------
    //
    // WKS-FR-SGCM: every live stream, with what each holds. Both graduation
    // surfaces read it — the start dialog to offer a stream (GSD-FR-ZPWN) and
    // the restart confirmation to default to one (GRT-FR-IMRI). The counts are
    // derived from the queue rather than written down, so a run enqueued in
    // this session moves them.
    case "list_work_streams":
      return workStreams.map((stream) => ({
        stream: { ...stream },
        queuedRunCount: graduationRuns.filter(
          (r) => r.streamId === stream.id && r.state === "queued",
        ).length,
        aheadOfBase: stream.isMissing ? 0 : 2,
        // WKS-FR-RJPD / WKS-FR-KFVJ: the harness streams stand level with
        // their base, so Update is disabled on each row.
        behindBase: 0,
        baseTipRevision: "a91bc04",
        missingCommits: [],
        // WKS-FR-KHJS: the stream's newest merge run that is neither discarded
        // nor archived, read from the queue alone.
        mergeRun: mergeRunOf(stream.id),
      }));
    case "get_work_stream": {
      const stream = workStreams.find((s) => s.id === String(a.streamId ?? ""));
      // eslint-disable-next-line no-throw-literal
      if (!stream) throw "unknown_stream";
      return { ...stream };
    }

    /**
     * WKS-FR-GKPX / WKS-FR-QNHF: Git merges first.
     *
     * The call answers once the check settles, with one of three kinds. Choose
     * the kind on the URL (default `merged`):
     *
     *   http://localhost:5199/?mergeNothing    -> nothing_to_merge
     *   http://localhost:5199/?mergeConflicts  -> a merge run is enqueued, and
     *                                             the call answers `conflicted`
     *   http://localhost:5199/?mergeDelay      -> the call stays out for 4s, so
     *                                             the running state is visible
     *   http://localhost:5199/?mergeRefused=merge_branch_moved
     *
     * A conflict enqueues a real run in the seeded queue, so Open in Runs
     * lands on it.
     */
    case "merge_work_stream": {
      const id = String(a.streamId ?? "");
      const stream = workStreams.find((s) => s.id === id);
      // eslint-disable-next-line no-throw-literal
      if (!stream) throw "unknown_stream";
      // eslint-disable-next-line no-throw-literal
      if (stream.isMissing) throw "stream_missing";
      // eslint-disable-next-line no-throw-literal
      if (stream.busyRunId || mergeRunHolds(id)) throw "stream_busy";
      await new Promise((resolve) =>
        setTimeout(resolve, harnessFlag("mergeDelay") ? 4000 : 300),
      );
      const refused = new URLSearchParams(location.search).get("mergeRefused");
      // eslint-disable-next-line no-throw-literal
      if (refused) throw refused;
      if (harnessFlag("mergeNothing")) return { kind: "nothing_to_merge" };
      if (harnessFlag("mergeConflicts")) {
        const run = enqueueMergeRun(stream, a.publication);
        (globalThis as any).__fireBusEvent?.("work-streams-changed", {
          projectKey: "harness",
        });
        (globalThis as any).__fireBusEvent?.("graduation-queue-changed", {
          projectKey: "harness",
          streamId: stream.id,
          worktreePath: "",
        });
        return {
          kind: "conflicted",
          runId: run.id,
          conflictedPaths: run.merge.unresolvedPaths,
        };
      }
      return {
        kind: "merged",
        mergedPaths: ["specifications/ui/GRU-graduation-runs.md", "src/a.ts"],
        commit:
          a.publication?.kind === "commit"
            ? "9f2c1ab4d5e6f708192a3b4c5d6e7f8091a2b3c4"
            : null,
      };
    }

    /**
     * AGV-FR-10: one page of a run's activity, ascending by `seq`.
     *
     * The same command serves a graduation run and a stream merge's semantic
     * turn — RUN-FR-VKLS reads a merge's turn by its attempt id, so the mock
     * answers for any id rather than only for a seeded run.
     */
    case "read_agent_activity": {
      const runId = String(a.runId ?? "");
      const after = a.after == null ? 0 : Number(a.after);
      const records = agentActivityRecords(runId).filter((r) => r.seq > after);
      return {
        runId,
        total: agentActivityRecords(runId).length,
        dropped: 0,
        latestSeq: agentActivityRecords(runId).length,
        records,
      };
    }

    /** WKS-FR-EIBC: remove a stream, its branch and its working copy. */
    case "delete_work_stream": {
      const id = String(a.streamId ?? "");
      const index = workStreams.findIndex((s) => s.id === id);
      // eslint-disable-next-line no-throw-literal
      if (index < 0) throw "unknown_stream";
      const stream = workStreams[index];
      // eslint-disable-next-line no-throw-literal
      if (stream.busyRunId) throw "stream_busy";
      workStreams.splice(index, 1);
      return null;
    }

    /** WKS-FR-KDXF: create a stream, or create nothing. */
    case "create_work_stream": {
      const name = String(a.name ?? "").trim();
      // eslint-disable-next-line no-throw-literal
      if (!name) throw "stream_name_invalid";
      // eslint-disable-next-line no-throw-literal
      if (workStreams.some((s) => s.name === name)) throw "stream_name_taken";
      const created = {
        id: `s-${workStreams.length + 1}`,
        projectKey: "/Users/demo/dev/acme",
        name,
        branch: `synthesis/stream/${name}`,
        worktreePath: `/Users/demo/.synthesis/streams/${name}`,
        baseBranch: String(a.baseBranch ?? "main"),
        baseRevision: "a91bc04",
        createdAt: new Date().toISOString(),
        busyRunId: null,
        isMissing: false,
      };
      workStreams.push(created);
      return { ...created };
    }

    case "list_graduation_queue":
      return {
        projectKey: "/Users/demo/dev/acme",
        // GRU-FR-HKBD: a project with an empty queue, which the seeded one is
        // not: http://localhost:5199/?noGraduations
        runs: harnessFlag("noGraduations")
          ? []
          : graduationRuns.map((r) => ({ ...r })),
      };
    case "get_graduation_run":
      return { ...findRun(String(a.runId ?? "")) };
    case "read_graduation_logs":
      return readGraduationActivity(a);
    // GRD-FR-GRHC: the limit, the slots held, and the queued runs that wait for
    // a slot alone. The limit is the one the Graduation section saved.
    case "get_graduation_capacity": {
      const limit = projectConfig.graduationConcurrencyLimit;
      const inUse = graduationRuns.filter(
        (r) => r.state === "working" || r.state === "reviewing",
      ).length;
      const full = limit !== "unlimited" && inUse >= limit;
      const holding = graduationRuns.filter(
        (r) => r.state === "working" || r.state === "reviewing",
      );
      // A queued run behind a run of its own stream waits for that stream, not
      // for a slot (GRU-FR-KMNF).
      const behindOwnQueue = (r: any) =>
        holding.some((h) => (h.streamId ?? h.queue) === (r.streamId ?? r.queue));
      return {
        limit,
        inUse,
        waitingForSlot: full
          ? graduationRuns
              .filter((r) => r.state === "queued" && !behindOwnQueue(r))
              .map((r) => r.id)
          : [],
      };
    }
    case "get_draft_graduation":
      return draftGraduation(String(a.draftId ?? ""));
    /**
     * GRV-FR-04: a project outside Git refuses the first attempt with the typed
     * `in_place_acknowledgement_required` and queues nothing — the start
     * dialog's only mode probe. Asked for on the URL, the seeded project being
     * a repository: http://localhost:5199/?graduationInPlace
     */
    case "start_graduation": {
      const draftId = String(a.draftId ?? "");
      const record: any = draftRecords[draftId];
      // eslint-disable-next-line no-throw-literal
      if (!record) throw "draft_not_found";
      if (harnessFlag("graduationStartFails")) {
        // eslint-disable-next-line no-throw-literal
        throw "graduation_worktree_creation_failed";
      }
      if (harnessFlag("graduationInPlace") && !a.inPlaceAcknowledged) {
        // eslint-disable-next-line no-throw-literal
        throw "in_place_acknowledgement_required";
      }
      // GRD-FR-08 / GRD-FR-64: the start preflight. A dirty source worktree is
      // refused with the complete path set, which is what turns the start dialog
      // into its blocking preflight form (GRV-FR-31). Asked for on the URL:
      // http://localhost:5199/?graduationDirty
      //
      // GRD-FR-67: the identity travels round the loop, so a second attempt
      // carrying the reported worktree is served — the commit the author made in
      // between is what the harness takes the worktree to be clean after.
      if (harnessFlag("graduationDirty") && !graduationWorktreeCommitted) {
        // GRD-FR-65 / GRD-FR-67: an attempt naming a checkout that is not the
        // active one reads no status and creates nothing.
        if (
          a.expectedSourceWorktree &&
          a.expectedSourceWorktree !== GRADUATION_SOURCE_WORKTREE
        ) {
          // eslint-disable-next-line no-throw-literal
          throw `source_worktree_changed: ${JSON.stringify({
            expected: String(a.expectedSourceWorktree),
            active: GRADUATION_SOURCE_WORKTREE,
          })}`;
        }
        // eslint-disable-next-line no-throw-literal
        throw `source_worktree_dirty: ${JSON.stringify({
          sourceWorktreePath: GRADUATION_SOURCE_WORKTREE,
          paths: UNCOMMITTED_PATHS,
        })}`;
      }
      if (graduationRuns.some((r) => r.draftId === draftId)) {
        // eslint-disable-next-line no-throw-literal
        throw "draft_already_graduating";
      }
      const inPlace = harnessFlag("graduationInPlace");
      const id = `g-${graduationRuns.length + 1}`;
      // GSD-FR-VKLD / GSD-FR-WQPD / GSD-FR-MZTB: the four answers the start
      // carries — the stream, what the run does with work standing there, and
      // the message that commit takes.
      const startStream = workStreams.find(
        (s) => s.id === String(a.streamId ?? ""),
      );
      const run = {
        id,
        projectKey: "/Users/demo/dev/acme",
        draftId,
        mode: inPlace ? "in_place" : "git",
        state: "queued",
        streamId: startStream?.id ?? null,
        streamName: startStream?.name ?? null,
        standingWork: String(a.standingWork ?? "commit"),
        standingWorkMessage: a.standingWorkMessage ?? null,
        input: {
          draftId,
          draftName: String(record.name ?? draftId),
          prompt: MD_BODY,
          promptChecksum: "sha-d1",
          capturedAt: new Date().toISOString(),
        },
        source: inPlace
          ? { kind: "in_place", projectDirectory: "/Users/demo/dev/acme" }
          : gitSource(id),
        iteration: 0,
        manifest: null,
        escalation: null,
        lastFailure: null,
        handoffPrompt: null,
        publication: null,
        enqueuedAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      graduationRuns.push(run);
      return graduationChanged(run);
    }
    /**
     * GSU-FR-MZGD: what the active worktree says about a direct start. A dirty
     * worktree is asked for on the URL:
     * http://localhost:5199/?graduationDirectDirty
     */
    case "preflight_direct_graduation": {
      const active: any = worktrees[0];
      return {
        worktreePath: active.path,
        worktreeName: active.name,
        branch: active.branch ?? null,
        isDetached: Boolean(active.isDetached),
        dirtyPaths: harnessFlag("graduationDirectDirty") ? UNCOMMITTED_PATHS : [],
      };
    }
    /** GSU-FR-PVFP: a direct run pinned to the worktree and branch confirmed. */
    case "start_direct_graduation": {
      const draftId = String(a.draftId ?? "");
      const record: any = draftRecords[draftId];
      // eslint-disable-next-line no-throw-literal
      if (!record) throw "draft_not_found";
      const active: any = worktrees[0];
      const id = `g-${graduationRuns.length + 1}`;
      const run = {
        id,
        projectKey: "/Users/demo/dev/acme",
        draftId,
        state: "queued",
        streamId: "",
        streamName: "",
        directTarget: {
          worktreePath: String(a.expectedWorktree ?? active.path),
          worktreeName: active.name,
          branch: String(a.expectedBranch ?? active.branch),
        },
        standingWork: "keep",
        standingWorkMessage: null,
        input: {
          draftId,
          draftName: String(record.name ?? draftId),
          prompt: MD_BODY,
          promptChecksum: "sha-d1",
          capturedAt: new Date().toISOString(),
        },
        enqueuedAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      graduationRuns.push(run as any);
      return graduationChanged(run as any);
    }
    /** GCM-FR-47: one manifest path's text on each side of its change. */
    case "read_graduation_change": {
      const run = findRun(String(a.runId ?? ""));
      const path = String(a.path ?? "");
      const entry = (run.manifest?.entries ?? []).find(
        (e: any) => e.path === path,
      );
      // eslint-disable-next-line no-throw-literal
      if (!entry) throw "path_not_in_manifest";
      return {
        path,
        operation: entry.operation,
        // Null for a created path, and throughout an in-place run.
        baseline:
          entry.operation === "created" || run.mode === "in_place"
            ? null
            : graduationBaselineText,
        current: graduationCurrentText,
        isBinary: false,
      };
    }
    case "accept_graduation_result": {
      const run = findRun(String(a.runId ?? ""));
      // GCM-FR-27 / GRD-FR-70: in place there is no publication to choose, and
      // acceptance **is** the specification publication boundary — the run
      // leaves the graduation queue for the implementation queue at once, and
      // is not terminal. In Git it opens the publication choice.
      if (run.mode === "in_place") {
        run.state = "implementation_queued";
        run.queue = "implementation";
        run.publication = {
          publication: { kind: "uncommitted" },
          appliedPaths: (run.manifest?.entries ?? []).map((e: any) => e.path),
          commitId: null,
        };
      } else {
        run.state = "awaiting_publication_choice";
      }
      return graduationChanged(run);
    }
    case "reject_graduation_result": {
      const run = findRun(String(a.runId ?? ""));
      run.state = "rejected";
      return graduationChanged(run);
    }
    case "request_graduation_changes": {
      const run = findRun(String(a.runId ?? ""));
      run.state = "requested_changes";
      run.iteration += 1;
      return graduationChanged(run);
    }
    /**
     * GXD-FR-BJYT / GRU-FR-39: the whole ordered answer set, in one call. Whole
     * or nothing — an entry short of one per recorded question is refused, so
     * the gating of **Send answers** is checked against a backend that agrees
     * with it rather than against one that accepts anything.
     *
     * Refuses the whole set when asked to on the URL, so the inline refusal of
     * GRU-FR-XQVG and the surviving answer draft of GRU-FR-42 are both reachable:
     * http://localhost:5199/?graduationAnswerRefused
     */
    case "answer_graduation_escalation": {
      const run = findRun(String(a.runId ?? ""));
      const answers: any[] = Array.isArray(a.answers) ? a.answers : [];
      const recorded: number[] = (run.escalation?.questions ?? []).map(
        (q: any) => q.position,
      );
      const answered = answers.map((entry) => Number(entry?.position));
      const complete =
        recorded.length === answered.length &&
        recorded.every((position, index) => answered[index] === position) &&
        answers.every(
          (entry) => String(entry?.answer ?? "").trim().length > 0,
        );
      if (!complete) {
        // eslint-disable-next-line no-throw-literal
        throw "answer_set_incomplete";
      }
      if (harnessFlag("graduationAnswerRefused")) {
        run.lastFailure = {
          code: "run_state_not_permitted",
          message: "This run is no longer resting on those questions.",
          retryable: false,
        };
        // eslint-disable-next-line no-throw-literal
        throw "run_state_not_permitted";
      }
      run.escalation = null;
      run.state = "running";
      return graduationChanged(run);
    }
    case "continue_graduation_run": {
      await graduationActDelay();
      const run = findRun(String(a.runId ?? ""));
      run.state = "running";
      return graduationChanged(run);
    }
    /**
     * GRD-FR-MDQZ: the author's own stop. The run is interrupted, gives its
     * stream back and offers Resume (GRU-FR-PVXD).
     */
    case "pause_graduation_run": {
      await graduationActDelay();
      const run = findRun(String(a.runId ?? ""));
      if (!["working", "reviewing"].includes(run.state)) {
        // eslint-disable-next-line no-throw-literal
        throw `run_state_not_permitted: ${JSON.stringify({ state: run.state })}`;
      }
      run.state = "interrupted";
      run.interruption = {
        reason: "author_pause",
        at: new Date().toISOString(),
        detail: "The author paused the run.",
        streamReleased: true,
      };
      return graduationChanged(run);
    }
    /**
     * GRH-FR-DYNU: filing a run away, and bringing it back. A filing act and
     * nothing else: the run's state and the stream it holds do not change.
     */
    case "archive_graduation_run":
    case "unarchive_graduation_run": {
      await graduationActDelay();
      const run = findRun(String(a.runId ?? ""));
      const archive = cmd === "archive_graduation_run";
      run.archived = archive;
      run.archivedAt = archive ? new Date().toISOString() : null;
      return graduationChanged(run);
    }
    /** GRD-FR-34: the author's explicit choice, and what it applied. */
    case "publish_graduation_result": {
      const run = findRun(String(a.runId ?? ""));
      const publication = a.publication ?? { kind: "uncommitted" };
      if (harnessFlag("graduationPublishConflicts")) {
        run.state = "publication_conflict";
        run.lastFailure = {
          code: "publication_conflict",
          message:
            "specifications/core/CMS-comments.md has changed since this change set was written.",
          retryable: true,
        };
        graduationChanged(run);
        // eslint-disable-next-line no-throw-literal
        throw "publication_conflict";
      }
      run.publication = {
        publication,
        appliedPaths: (run.manifest?.entries ?? []).map((e: any) => e.path),
        commitId: publication.kind === "commit" ? SPECIFICATION_REVISION : null,
      };
      if (publication.kind === "commit") {
        // GRD-FR-70: a committed specification is in the repository, so the
        // implementation is enqueued automatically and nothing is asked of the
        // author.
        run.state = "implementation_queued";
        run.queue = "implementation";
        run.specificationRevision = SPECIFICATION_REVISION;
      } else {
        // GRD-FR-71: an uncommitted specification enqueues nothing. The run
        // holds neither queue and waits on the explicit **Implement**.
        run.state = "awaiting_implementation";
        run.queue = null;
      }
      return graduationChanged(run);
    }
    case "set_graduation_handoff_prompt": {
      const run = findRun(String(a.runId ?? ""));
      run.handoffPrompt = String(a.prompt ?? "");
      return run;
    }
    /**
     * GRD-FR-ZAMI / GSU-FR-IRAC: a new run over a discarded run's captured
     * prompt, on the stream the restart names, carrying the standing-work
     * choice the restart took rather than the discarded run's own.
     *
     * The discarded run is left exactly as it was (GRT-FR-VWHM). Refused for a
     * run that is not discarded, which is what the control's own guard means.
     * A refusal is asked for on the URL:
     * http://localhost:5199/?restartFails
     */
    case "restart_graduation_run": {
      await graduationActDelay();
      const source = findRun(String(a.runId ?? ""));
      if (harnessFlag("restartFails")) {
        // eslint-disable-next-line no-throw-literal
        throw "stream_busy";
      }
      if (source.state !== "discarded") {
        // eslint-disable-next-line no-throw-literal
        throw "run_not_discarded";
      }
      const stream = workStreams.find((s) => s.id === String(a.streamId ?? ""));
      // eslint-disable-next-line no-throw-literal
      if (!stream) throw "unknown_stream";
      const created = makeStreamRun({
        id: `g-${graduationRuns.length + 20}`,
        draftId: source.draftId,
        state: "queued",
        streamId: stream.id,
        streamName: stream.name,
        standingWork: String(a.standingWork ?? "commit"),
        standingWorkMessage: a.standingWorkMessage ?? null,
        restartedFromRunId: source.id,
        input: { ...source.input },
      });
      graduationRuns.push(created);
      return graduationChanged(created);
    }
    /**
     * GRD-FR-BLCR: the run's commits are reverted as new commits on the stream
     * branch. The run keeps its state. A refusal is asked for on the URL:
     * http://localhost:5199/?revertFails
     */
    case "revert_graduation_run": {
      await graduationActDelay();
      const run = findRun(String(a.runId ?? ""));
      if (harnessFlag("revertFails")) {
        // eslint-disable-next-line no-throw-literal
        throw "run_not_revertable";
      }
      // GRD-FR-BLCR: refused while the run holds its stream and while it is queued.
      if (["working", "reviewing", "blocked", "queued"].includes(run.state)) {
        // eslint-disable-next-line no-throw-literal
        throw `run_state_not_permitted: ${JSON.stringify({ state: run.state })}`;
      }
      // eslint-disable-next-line no-throw-literal
      if ((run.commits ?? []).length === 0) throw "run_not_revertable";
      // GRD-FR-BLCR: a run that has not ended ends with its work reverted.
      if (["interrupted", "awaiting_author"].includes(run.state)) {
        run.state = "discarded";
        run.queue = null;
      }
      return graduationChanged(run);
    }
    case "discard_graduation_run": {
      await graduationActDelay();
      const run = findRun(String(a.runId ?? ""));
      run.state = "discarded";
      run.queue = null;
      return graduationChanged(run);
    }
    /**
     * GRD-FR-71: the author's explicit **Implement**, accepted from
     * `awaiting_implementation` alone. It enqueues; it does not dispatch — the
     * clean-source and committed-specification preflight of GRD-FR-72 runs when
     * the implementation queue releases the run.
     *
     * The two refusals the start preflight stands for are asked for on the URL,
     * so 's blocking preflight and GRU-FR-25's inline refusal are both
     * reachable:
     *   http://localhost:5199/?implementDirty
     *   http://localhost:5199/?implementSpecsUncommitted
     */
    case "start_implementation": {
      const run = findRun(String(a.runId ?? ""));
      if (run.state !== "awaiting_implementation") {
        // eslint-disable-next-line no-throw-literal
        throw `run_state_not_permitted: ${JSON.stringify({ state: run.state })}`;
      }
      // GRD-FR-72 / GRD-FR-67: an attempt naming a checkout that is not the
      // active one reads no status and creates nothing.
      if (
        a.expectedSourceWorktree &&
        a.expectedSourceWorktree !== GRADUATION_SOURCE_WORKTREE
      ) {
        // eslint-disable-next-line no-throw-literal
        throw `source_worktree_changed: ${JSON.stringify({
          expected: String(a.expectedSourceWorktree),
          active: GRADUATION_SOURCE_WORKTREE,
        })}`;
      }
      if (harnessFlag("implementDirty") && !graduationWorktreeCommitted) {
        // eslint-disable-next-line no-throw-literal
        throw `source_worktree_dirty: ${JSON.stringify({
          sourceWorktreePath: GRADUATION_SOURCE_WORKTREE,
          paths: UNCOMMITTED_PATHS,
        })}`;
      }
      if (harnessFlag("implementSpecsUncommitted") && !graduationWorktreeCommitted) {
        // eslint-disable-next-line no-throw-literal
        throw `specification_not_committed: ${JSON.stringify({
          paths: (run.publication?.appliedPaths ?? []).slice(),
        })}`;
      }
      run.state = "implementation_queued";
      run.queue = "implementation";
      run.specificationRevision = SPECIFICATION_REVISION;
      run.implementationBlocker = null;
      return graduationChanged(run);
    }
    /**
     * GRD-FR-85: the implementation publication, Git mode only, on the author's
     * explicit choice. A refusal keeps the run non-terminal and offers
     * publication again:
     *   http://localhost:5199/?implementationPublishConflicts
     */
    case "publish_implementation_result": {
      const run = findRun(String(a.runId ?? ""));
      const publication = a.publication ?? { kind: "uncommitted" };
      if (harnessFlag("implementationPublishConflicts")) {
        run.state = "implementation_publication_conflict";
        run.lastFailure = {
          code: "publication_conflict",
          message:
            "src/diff/Unified.tsx has changes in that worktree that the patch cannot be applied over.",
          retryable: true,
        };
        graduationChanged(run);
        // eslint-disable-next-line no-throw-literal
        throw "publication_conflict";
      }
      run.state = "implemented";
      run.queue = null;
      run.implementationPublication = {
        publication,
        appliedPaths: (run.implementationManifest?.entries ?? []).map(
          (e: any) => e.path,
        ),
        commitId: publication.kind === "commit" ? "b40d9ce" : null,
      };
      return graduationChanged(run);
    }

    // ---- draft change proposals (DCP-draft-change-proposals.md) ----------
    case "list_draft_change_proposals":
      // DCR-FR-32 / CMT-FR-71: a reading that never answers, asked for on the
      // URL. `?proposalsUnreadable` refuses every read, which is what leaves a
      // rail's controls **unresolved** for long enough to look at — a state the
      // seeded project cannot otherwise be driven into, the mock's reads all
      // being instantaneous and successful. `?proposalsSlow` leaves the first
      // ask in flight instead, for the same reason from the other side.
      if (harnessFlag("proposalsUnreadable")) throw "proposals_unreadable";
      if (harnessFlag("proposalsSlow")) {
        await new Promise(() => {});
      }
      return proposalsByDraft[String(a.draftId ?? "")] ?? [];
    /**
     * DCP-FR-10 / DCR-FR-05: the proposal's changes in proposal order, each with
     * where it lands in the prompt **as it stands**.
     *
     * Resolved here the way the real backend resolves it — by finding the text
     * the change names (DCP-FR-VZTK) — so a change whose text the author has
     * since rewritten comes back `lost` (DCP-FR-BMLX) and the surface's
     * disabled Accept is actually reachable in a browser.
     */
    case "load_draft_change_proposal_hunks": {
      const id = String(a.proposalId ?? "");
      const hunks = proposalHunks[id];
      if (hunks === undefined) throw "proposal_not_found";
      const row = (proposalsByDraft["d-1"] ?? []).find((p) => p.id === id);
      const prompt = draftBodies[row?.path ?? ""] ?? ORIGINAL_PROMPT;
      const resolutions = hunks.map((h) => {
        const before = String(h.before ?? "");
        if (before === "") {
          const lead = String(h.anchor?.lead ?? "");
          const at = lead === "" ? 0 : prompt.indexOf(lead);
          return at === -1
            ? { kind: "lost" }
            : { kind: "resolved", start: at + lead.length, end: at + lead.length };
        }
        const at = prompt.indexOf(before);
        return at === -1
          ? { kind: "lost" }
          : { kind: "resolved", start: at, end: at + before.length };
      });
      return {
        hunks: hunks.map((h) => ({ ...h })),
        resolutions,
        checksum: candidateChecksum(id),
        legacy: row?.legacy === true,
      };
    }
    /**
     * DCP-FR-25 / DCR-FR-26: the author's rewrite of ONE change, written into
     * proposal storage alone. It deliberately touches nothing under the draft's
     * `files/` and refreshes no `updated_at`, so a verification can prove the
     * draft did not move — and it enforces the `candidate_stale` baseline check
     * (DCP-FR-27) so the surface's checksum handoff is actually exercised.
     */
    case "edit_draft_change_hunk": {
      const id = String(a.proposalId ?? "");
      const hunks = proposalHunks[id];
      if (hunks === undefined) throw "proposal_not_found";
      const row = (proposalsByDraft["d-1"] ?? []).find((p) => p.id === id);
      if (row && row.state !== "pending") throw "already_decided";
      if (harnessFlag("candidateWriteFails")) throw "write_failed";
      const target = hunks.find((h) => h.id === String(a.hunkId ?? ""));
      if (!target) throw "hunk_not_found";
      const ledgerRow = row?.ledger?.find((r: any) => r.id === target.id);
      if (ledgerRow && ledgerRow.state !== "pending" && ledgerRow.state !== "discussing") {
        throw "hunk_already_decided";
      }
      const baseline = String(a.baselineChecksum ?? "");
      if (baseline !== candidateChecksum(id)) throw "candidate_stale";
      target.after = String(a.after ?? "");
      if (ledgerRow) ledgerRow.edited = true;
      if (row) row.candidateEdited = true;
      candidateRevision[id] = (candidateRevision[id] ?? 0) + 1;
      return { checksum: candidateChecksum(id) };
    }
    /**
     * DCP-FR-PWSF: hold one change for discussion, or release it. Undecided
     * either way, so it goes on holding the draft's one pending slot.
     */
    case "set_draft_change_hunk_discussing": {
      const id = String(a.proposalId ?? "");
      const row = (proposalsByDraft["d-1"] ?? []).find((p) => p.id === id);
      if (!row) throw "proposal_not_found";
      const ledgerRow = row.ledger?.find((r: any) => r.id === String(a.hunkId ?? ""));
      if (!ledgerRow) throw "hunk_not_found";
      if (ledgerRow.state === "accepted" || ledgerRow.state === "rejected") {
        throw "hunk_already_decided";
      }
      ledgerRow.state = a.discussing ? "discussing" : "pending";
      recountProposal(row);
      fireBus("draft-change-proposals-changed", {
        draftId: row.draftId,
        proposal: { ...row },
      });
      return { ...row };
    }
    /**
     * DCP-FR-11 / DCP-FR-14 / DCR-FR-20: one decision about ONE change.
     *
     * An acceptance splices only the text that change names, through the
     * draft's own save path, so the seeded body follows it — which is what makes
     * the tab's editing surface show the change land with no external-change
     * dialog. Every other change of the proposal is left undecided.
     */
    case "accept_draft_change_hunk":
    case "reject_draft_change_hunk": {
      const id = String(a.proposalId ?? "");
      const accepted = cmd === "accept_draft_change_hunk";
      const row = (proposalsByDraft["d-1"] ?? []).find((p) => p.id === id);
      if (!row) throw "proposal_not_found";
      const hunkId = String(a.hunkId ?? "");
      const ledgerRow = row.ledger?.find((r: any) => r.id === hunkId);
      const hunk = (proposalHunks[id] ?? []).find((h) => h.id === hunkId);
      if (!ledgerRow || !hunk) throw "hunk_not_found";
      if (ledgerRow.state === "accepted" || ledgerRow.state === "rejected") {
        throw "hunk_already_decided";
      }
      if (accepted) {
        const prior = draftBodies[row.path] ?? ORIGINAL_PROMPT;
        const before = String(hunk.before ?? "");
        let next: string;
        if (before === "") {
          const lead = String(hunk.anchor?.lead ?? "");
          const at = lead === "" ? 0 : prior.indexOf(lead);
          // DCP-FR-BMLX: the text this change names is gone, so there is
          // nowhere to put it. Rejecting is what clears such a change.
          if (at === -1) throw "anchor_lost";
          const cut = at + lead.length;
          next = prior.slice(0, cut) + String(hunk.after ?? "") + prior.slice(cut);
        } else {
          const at = prior.indexOf(before);
          if (at === -1) throw "anchor_lost";
          next =
            prior.slice(0, at) + String(hunk.after ?? "") + prior.slice(at + before.length);
        }
        draftBodies[row.path] = next;
        recordAcceptedVersion(row, id, hunkId, prior, next);
      }
      ledgerRow.state = accepted ? "accepted" : "rejected";
      ledgerRow.decidedAt = new Date().toISOString();
      recountProposal(row);
      fireBus("draft-change-proposals-changed", {
        draftId: row.draftId,
        proposal: { ...row },
      });
      if (accepted) fireBus("drafts-changed", null);
      // DCP-FR-15: the comment that answers the agent is appended by the
      // decision that leaves nothing undecided, and by no other.
      const resolving = row.state !== "pending";
      return {
        proposal: { ...row },
        commentId: resolving ? `${row.threadId}-decision-${id}` : undefined,
        originKind: "draft_discussion",
      };
    }
    /**
     * DCP-FR-14: reject every change of the proposal that is still undecided.
     *
     * The escape hatch the draft needs: a change held for discussion still holds
     * the draft's one pending slot, and this is what releases it (DCP-FR-04).
     */
    case "decline_draft_change_proposal": {
      const id = String(a.proposalId ?? "");
      const row = (proposalsByDraft["d-1"] ?? []).find((p) => p.id === id);
      if (!row) throw "proposal_not_found";
      if (row.state !== "pending") throw "already_decided";
      for (const r of row.ledger ?? []) {
        if (r.state === "pending" || r.state === "discussing") {
          r.state = "rejected";
          r.decidedAt = new Date().toISOString();
        }
      }
      recountProposal(row);
      fireBus("draft-change-proposals-changed", {
        draftId: row.draftId,
        proposal: { ...row },
      });
      return {
        proposal: { ...row },
        commentId: `${row.threadId}-decision-${id}`,
        originKind: "draft_discussion",
      };
    }

    // ---- prompt change proposals (PCP-prompt-change-proposals.md) ---------
    /**
     * PCP-FR-10 / PCR-FR-27: every change proposed against this prompt artifact,
     * records only. `?promptProposalsUnreadable` refuses every read, which is the
     * one way to reach the reading-failed state of PCR-FR-27 in a browser.
     */
    case "list_prompt_change_proposals":
      if (harnessFlag("promptProposalsUnreadable")) throw "proposals_unreadable";
      return promptProposalsByArtifact[String(a.artifactId ?? "")] ?? [];
    // PCP-FR-11: the candidate's text plus the baseline its next save is checked
    // against (PCP-FR-24).
    case "load_prompt_change_proposal_content": {
      const id = String(a.proposalId ?? "");
      const content = promptProposalContent[id];
      if (content === undefined) throw "proposal_not_found";
      return { content, checksum: promptCandidateChecksum(id) };
    }
    /**
     * PCP-FR-22 / PCR-FR-22: the author's rewrite of a candidate, written into
     * **proposal storage alone**. It touches no file of the project — no
     * `artifactBodies` entry, no `artifact-changed-externally`, no recently-edited
     * row — so a verification can prove the file did not move; and it enforces the
     * `candidate_stale` baseline check (PCP-FR-24) so the checksum handoff is
     * actually exercised.
     */
    case "save_prompt_change_proposal_candidate": {
      const id = String(a.proposalId ?? "");
      if (promptProposalContent[id] === undefined) throw "proposal_not_found";
      const row = promptRow(id);
      if (row && row.state !== "pending") throw "already_decided";
      if (harnessFlag("promptCandidateWriteFails")) throw "write_failed";
      const baseline = String(a.baselineChecksum ?? "");
      if (baseline !== promptCandidateChecksum(id)) throw "candidate_stale";
      promptProposalContent[id] = String(a.content ?? "");
      promptCandidateRevision[id] = (promptCandidateRevision[id] ?? 0) + 1;
      if (row) row.candidateEdited = true;
      return { checksum: promptCandidateChecksum(id) };
    }
    /**
     * PCP-FR-12 through PCP-FR-16: the two decisions, one transaction with one
     * commit point. An acceptance is a whole-file replacement written through the
     * project's own artifact path, so the seeded body follows it — which is what
     * makes the tab's editing surface render the accepted text with no
     * external-change dialog (PCR-FR-11).
     *
     * Three flags reach the answers no acting in the UI can produce:
     * `?promptAcceptFails` (the typed `write_failed` of PCR-FR-15),
     * `?promptAcceptOwed` (the `acceptance_incomplete` past the commit point),
     * `?promptNotAPrompt` (the target no longer resolving as a prompt).
     */
    case "apply_prompt_change_proposal":
    case "decline_prompt_change_proposal": {
      const id = String(a.proposalId ?? "");
      const accepted = cmd === "apply_prompt_change_proposal";
      const found = promptRow(id);
      if (!found) throw "proposal_not_found";
      if (found.state !== "pending") throw "already_decided";
      if (accepted && harnessFlag("promptNotAPrompt")) throw "not_a_prompt_artifact";
      if (accepted && harnessFlag("promptAcceptFails")) throw "write_failed";
      found.state = accepted ? "accepted" : "rejected";
      found.decidedAt = new Date().toISOString();
      if (accepted) {
        // PCP-FR-13: a complete replacement of the artifact's contents as they
        // stand, with no baseline checksum consulted anywhere.
        artifactBodies[found.artifactId] = {
          body: promptProposalContent[id] ?? "",
          checksum: `sha-applied-${nextArtifactChecksum++}`,
        };
      }
      if (accepted && harnessFlag("promptAcceptOwed")) {
        // PCP-FR-14: past the commit point with the decision comment still owed.
        // The event is emitted at the commit point rather than at the append.
        found.commentOwed = true;
        fireBus("prompt-change-proposals-changed", {
          artifactId: found.artifactId,
          proposal: { ...found },
        });
        throw "acceptance_incomplete";
      }
      fireBus("prompt-change-proposals-changed", {
        artifactId: found.artifactId,
        proposal: { ...found },
      });
      // PCR-FR-14: the surface dispatches one fresh turn from these two values.
      return {
        proposal: { ...found },
        commentId: `${found.threadId}-decision-${id}`,
        originKind: "artifact_comment",
      };
    }
    /**
     * PCP-FR-14 / PCR-FR-15: finish an acceptance whose decision comment is still
     * owed. It decides nothing itself. `?promptCompleteFails` keeps owing it, for
     * the retry that fails again.
     */
    case "complete_prompt_change_decision": {
      const id = String(a.proposalId ?? "");
      const found = promptRow(id);
      if (!found) throw "proposal_not_found";
      if (harnessFlag("promptCompleteFails")) {
        return { proposal: { ...found }, originKind: "artifact_comment" };
      }
      found.commentOwed = false;
      fireBus("prompt-change-proposals-changed", {
        artifactId: found.artifactId,
        proposal: { ...found },
      });
      return {
        proposal: { ...found },
        commentId: `${found.threadId}-decision-${id}`,
        originKind: "artifact_comment",
      };
    }

    // ---- changes / git ---------------------------------------------------
    case "list_uncommitted_changes":
      // CHG-FR-22 / CHG-FR-23: neither the not-a-repository state nor an empty
      // change set can be produced by acting in the UI, so the harness reaches
      // them by flag — `?notARepo` and `?noChanges` on the URL.
      if (harnessFlag("notARepo")) throw "not a git repository";
      if (harnessFlag("noChanges"))
        return { comparison: { kind: "uncommitted" }, entries: [] };
      return { comparison: { kind: "uncommitted" }, entries: changeEntries };
    case "list_branch_changes":
      if (harnessFlag("notARepo")) throw "not a git repository";
      return {
        comparison: { kind: "branch", targetBranch: a.targetBranch ?? "main", mergeBase: "4f2a10c" },
        entries: harnessFlag("noChanges") ? [] : changeEntries.slice(0, 3),
      };
    case "get_default_branch":
      return "main";
    case "list_comparison_branches":
      return [
        { name: "main", isCurrent: false, isDefault: true },
        { name: "feature/new-artifact-window", isCurrent: true, isDefault: false },
        { name: "develop", isCurrent: false, isDefault: false },
      ];
    case "get_uncommitted_diff_totals":
      return { addedLines: 605, removedLines: 221, fileCount: 9 };
    case "load_changes_panel_state":
      return { mode: "uncommitted" };
    case "save_changes_panel_state":
      return undefined;
    case "get_diff":
      return diffPayload;
    case "get_file_revisions": {
      // DFV-FR-57: a source path is compared against a source revision, so both
      // sides of the viewer have a language to colour. Markdown paths keep the
      // prose pair the rest of the fixtures are built on.
      const path = String((a.scope as { path?: string } | undefined)?.path ?? "");
      const ext = extOf(path);
      const oldSource = SOURCE_OLD_BODIES[ext];
      const newSource = SOURCE_BODIES[ext];
      if (oldSource !== undefined && newSource !== undefined) {
        return { old: oldSource, new: newSource, isBinary: false };
      }
      return { old: OLD_TEXT, new: NEW_TEXT, isBinary: false };
    }
    case "list_branches":
      return [
        { name: "main", kind: "local", isCurrent: false },
        { name: "feature/new-artifact-window", kind: "local", isCurrent: true },
        { name: "develop", kind: "local", isCurrent: false },
        { name: "origin/main", kind: "remote", isCurrent: false },
      ];
    // GTC-FR-19: a commit reports its id *and* every path it recorded, which is
    // not the set submitted. The strip closes Diff tabs from `committedPaths`
    // (TAB-FR-22), so the harness echoes back what was asked for.
    case "commit_paths":
      // GTC-FR-31: a commit bound to a checkout that is no longer the active one
      // is refused ahead of every other check, having written nothing. Asked for
      // on the URL: http://localhost:5199/?graduationDirty&commitWorktreeMoved
      if (
        a.expectedWorktree &&
        (harnessFlag("commitWorktreeMoved") ||
          a.expectedWorktree !== GRADUATION_SOURCE_WORKTREE)
      ) {
        // eslint-disable-next-line no-throw-literal
        throw `worktree_identity_changed: ${JSON.stringify({
          expected: String(a.expectedWorktree),
          active: "/Users/demo/dev/acme-feature",
        })}`;
      }
      // GRV-FR-36: the commit that answers a start preflight is what the next
      // `start_graduation` finds the worktree clean after.
      if (a.expectedWorktree) graduationWorktreeCommitted = true;
      return {
        commitId: "a1b2c3d",
        committedPaths: Array.isArray(a.paths) ? a.paths.map(String) : [],
      };

    // GTC-FR-29 … GTC-FR-32: the single working-tree status primitive. No UI
    // consumer of its own — the graduation start preflight reads it through
    // `start_graduation` — so the harness answers it for completeness.
    case "get_working_tree_status":
      return {
        worktreePath: GRADUATION_SOURCE_WORKTREE,
        entries: graduationWorktreeCommitted ? [] : UNCOMMITTED_PATHS,
      };
    // GTC-FR-23 – GTC-FR-26: a rollback reports what became of each named path
    // on its own, never as one verdict for the selection (GTC-FR-25). The mock
    // mirrors the backend's classification so the panel's per-path application
    // (CHG-FR-63) can be driven for real: an untracked path is `removed`, a
    // renamed one reports both identities, and a path named `fails-rollback.md`
    // is the failure case a browser pass needs to reach.
    case "rollback_paths": {
      const paths = Array.isArray(a.paths) ? a.paths.map(String) : [];
      const entryFor = (p: string) => {
        const change = changeEntries.find((e) => e.path === p);
        if (p.includes("fails-rollback")) {
          return {
            id: p,
            path: p,
            previousPath: change?.previousPath ?? null,
            outcome: "failed",
            restoredPaths: [],
            removedPaths: [],
            failures: [{ path: p, kind: "permission_denied" }],
          };
        }
        if (change?.changeStatus === "untracked") {
          return {
            id: p,
            path: p,
            previousPath: null,
            outcome: "removed",
            restoredPaths: [],
            removedPaths: [p],
            failures: [],
          };
        }
        if (change?.changeStatus === "renamed" && change.previousPath) {
          return {
            id: p,
            path: p,
            previousPath: change.previousPath,
            outcome: "restored",
            restoredPaths: [change.previousPath],
            removedPaths: [p],
            failures: [],
          };
        }
        return {
          id: p,
          path: p,
          previousPath: null,
          outcome: "restored",
          restoredPaths: [p],
          removedPaths: [],
          failures: [],
        };
      };
      return { entries: paths.map(entryFor) };
    }
    case "get_upstream_sync_state":
      return { hasRemote: true, hasUpstream: true, ahead: 2, behind: 1 };
    case "push_current_branch":
      return undefined;

    // ---- worktrees -------------------------------------------------------
    case "list_worktrees_and_branches":
    case "activate_worktree":
    case "check_out_branch_in_active_worktree":
    case "create_worktree":
      return worktreeContext;
    case "get_active_worktree":
      return worktrees[0];
    case "propose_worktree_path":
      return `/Users/demo/dev/acme-${String(a.branch).replace(/\//g, "-")}`;
    case "refresh_worktrees_and_branches":
      return { context: worktreeContext, remoteState: "refreshed" };

    // ---- panels ----------------------------------------------------------
    case "load_library_panel_state":
      return { expandedPaths: ["specifications", "specifications/ui"], artifactTypeFilter: "all_artifacts", textFilter: "" };
    case "save_library_panel_state":
      return undefined;
    case "load_notes_panel_state":
      return { scopePosition: "all", textFilter: "" };
    case "save_notes_panel_state":
      return undefined;
    /**
     * PSS-FR-20 / DRP-FR-14: expansion is recorded rather than collapse, and a
     * path naming a folder that is not present is retained rather than pruned —
     * `archive/old` is seeded exactly to exercise that, being nowhere in
     * `draftFolders`. `research` is deliberately absent so an empty folder is
     * first seen collapsed.
     */
    case "load_drafts_panel_state":
      return {
        statusFilter: draftsPanelState.statusFilter,
        textFilter: draftsPanelState.textFilter,
        expandedFolders: [...draftsPanelState.expandedFolders],
      };
    case "save_drafts_panel_state": {
      const next: any = a.state ?? {};
      draftsPanelState.statusFilter = next.statusFilter ?? draftsPanelState.statusFilter;
      draftsPanelState.textFilter = next.textFilter ?? "";
      draftsPanelState.expandedFolders = [...(next.expandedFolders ?? [])];
      return undefined;
    }

    // ---- notes -----------------------------------------------------------
    case "list_notes_for_entity":
    case "list_project_notes":
    case "list_all_notes":
      return [
        {
          note: note("n-1", "Check whether the lens should reset on worktree switch."),
          entityName: "LIB-library.md",
          unresolved: false,
        },
        {
          note: note("n-2", "Ship the **drafts panel** before the editor rework.", {
            reminder: "2026-08-04T09:00:00Z",
            scope: { kind: "project" },
          }),
          unresolved: false,
          // NTC-FR-19: the note's one discussion, read from the comment store's
          // index. `n-1` and `n-3` carry none, so both branches of NTS-FR-28 are
          // reachable from the seeded panel.
          discussionThreadId: noteDiscussions("n-2")[0]?.id,
        },
        {
          note: note("n-3", "Orphaned note about a deleted spec.", {
            scope: {
              kind: "entity",
              entityId: "specifications/ui/GONE.md",
              entityPath: "specifications/ui/GONE.md",
            },
            revision: "4f2a10c",
          }),
          unresolved: true,
        },
      ];
    case "create_note":
      return note("n-new", a.body ?? "");
    case "update_note":
      return note(a.id, a.fields?.body ?? "");
    /** NTC-FR-21: the note and whatever discussion it carries, together. */
    case "delete_note":
      delete discussionsByTarget[`note:${String(a.id ?? "")}`];
      return undefined;

    // ---- comments --------------------------------------------------------
    case "list_comment_threads": {
      // The seeded anchors quote `LIB-library.md`'s body. `EDT-editor.md` is the
      // frontmatter-carrying artifact, so it gets its own set quoting passages
      // that actually occur in `MD_BODY_FM` — otherwise every thread on the one
      // file with a frontmatter region orphans, and the aligned column
      // (CMT-FR-27) is unreachable in exactly the case most worth checking.
      if (String(a.artifactId ?? "").endsWith("EDT-editor.md")) {
        const fm = (id: string, over: Record<string, unknown> = {}) =>
          thread(id, { artifactId: "specifications/ui/EDT-editor.md", ...over });
        const anchored = [
          fm("t-f1", { anchor: { start: 88, end: 133, quote: "The Editor is where an artifact is written." } }),
          fm("t-f2", { anchor: { start: 205, end: 217, quote: "Requirements" } }),
          fm("t-f3", { anchor: { start: 290, end: 330, quote: "the frontmatter region renders YAML by role." } }),
        ];
        /**
         * CMT-FR-63: what bounds the resolved footer is whether the margin has
         * anything ELSE to show. Neither state is reachable by acting in the UI
         * — the seeded set is all unresolved, and resolving is a write the mock
         * does not persist — so the two are asked for on the URL instead.
         *
         * `?allResolved` — every thread resolved and nothing else in the margin,
         * so the expanded disclosure may have it whole.
         * `?oneOpen` — the same set with one thread left unresolved, so the
         * footer must stop a card's height short of the top.
         */
        if (harnessFlag("allResolved")) {
          return anchored.map((t) => ({ ...t, resolved: true }));
        }
        if (harnessFlag("oneOpen")) {
          return anchored.map((t, i) => ({ ...t, resolved: i > 0 }));
        }
        return anchored;
      }
      // CMT-FR-48 / PCR-FR-03: the prompt the proposals are against carries the
      // conversation they were announced in, so the rail's control is the route
      // into the review it is meant to be.
      if (String(a.artifactId ?? "") === PROMPT_ARTIFACT) {
        return [promptProposalThread()];
      }
      return Object.entries(ANCHORED_OVERRIDES).map(([id, over]) => thread(id, over));
    }
    case "list_all_comment_threads":
      // CMP-FR-21: the panel writes nothing (CMP-FR-16), so a project with no
      // thread at all is not reachable by acting in the UI — `?noComments`.
      if (harnessFlag("noComments")) return [];
      return [
        { thread: thread("t-1"), unresolved: false },
        { thread: thread("t-2", { resolved: true }), unresolved: false },
        // CMP-FR-30: the one panel row whose *opening* comment is an agent's,
        // so the title line the panel renders between the author line and the
        // body is reachable at all — every other seeded thread opens with the
        // human, and a panel that showed nothing would be the fixture's doing
        // rather than the feature's. Long enough to exercise the row's clip.
        (() => {
          const t = thread("t-9", { artifactId: "specifications/ui/GONE.md" });
          return {
            thread: {
              ...t,
              comments: [
                { ...t.comments[0], author: agentLongTitle },
                ...t.comments.slice(1),
              ],
            },
            unresolved: true,
          };
        })(),
      ];
    /**
     * CVP-FR-53 / CMS-FR-59: read one conversation by its thread id alone, for
     * the presentations that have no owning surface to read it through — a
     * detached overlay or a conversation tab whose owning tab is closed. Every
     * seeded id has to resolve here, because a presentation that outlives its
     * rail reads through this and nothing else; falling through would look like
     * the overlay failing to render a conversation it holds.
     */
    case "read_comment_thread": {
      const id = String(a.threadId ?? "");
      const seeded = [
        ...allDiscussions(),
        ...Object.entries(ANCHORED_OVERRIDES).map(([tid, over]) => thread(tid, over)),
        thread("t-f1", {
          artifactId: "specifications/ui/EDT-editor.md",
          anchor: { start: 88, end: 133, quote: "The Editor is where an artifact is written." },
        }),
        thread("t-f2", {
          artifactId: "specifications/ui/EDT-editor.md",
          anchor: { start: 205, end: 217, quote: "Requirements" },
        }),
        thread("t-f3", {
          artifactId: "specifications/ui/EDT-editor.md",
          anchor: { start: 290, end: 330, quote: "the frontmatter region renders YAML by role." },
        }),
        thread("t-9", { artifactId: "specifications/ui/GONE.md" }),
        promptProposalThread(),
      ].find((t: any) => t.id === id);
      if (!seeded) throw "thread_not_found";
      return seeded;
    }
    case "resolve_comment_author_identity":
      return human;
    /**
     * CMS-FR-43: an append echoes the comment it was given back, attachments
     * included. Returning a static fixture instead would make the whole
     * attachment round trip (CMT-FR-46 → CMT-FR-47 → CMT-FR-48) invisible in the
     * browser: the pending strip would clear and the card would re-render
     * exactly as it was, which reads as a bug and is not one.
     */
    /**
     * CMS-FR-58: a draft's discussions, oldest first. The *only* route to one —
     * `list_comment_threads` and `list_all_comment_threads` above deliberately
     * return none, so the New Artifact tab's margin is the one place they show.
     */
    /** CMS-FR-JWVH: the set a discussion holds, or null where it holds none. */
    case "read_discussion_question_set":
      return questionSetsByThread[String(a.threadId ?? "")] ?? null;
    /**
     * CMS-FR-TXRB: the whole ordered set of answers, in one call. Appends two
     * comments per recorded question in the recorded order — the agent-authored
     * question, then the human-authored answer — and then deletes the set.
     */
    case "submit_discussion_question_answers": {
      const threadId = String(a.threadId ?? "");
      const set = questionSetsByThread[threadId];
      if (!set) throw "question_set_not_found";
      if (String(a.setId ?? "") !== set.setId) throw "question_set_not_found";
      const answers = Array.isArray(a.answers) ? (a.answers as any[]) : [];
      if (answers.length !== set.questions.length) throw "question_answers_incomplete";
      const held = allDiscussions().find((t: any) => t.id === threadId);
      if (!held) throw "thread_not_found";
      let finalAnswerCommentId = "";
      for (const question of set.questions) {
        const answer = answers.find(
          (entry) => entry.questionPosition === question.position,
        );
        if (!answer) throw "question_answers_incomplete";
        // CMS-FR-GNTB: a chosen option or the author's own words, never both
        // and never neither, and a note only beside a chosen option.
        const wroteOwn = typeof answer.ownAnswer === "string";
        const choseOption =
          answer.optionPosition !== undefined || answer.optionValue !== undefined;
        if (wroteOwn === choseOption) throw "question_answers_incomplete";
        let option: any = null;
        if (wroteOwn) {
          if (String(answer.ownAnswer).trim() === "") {
            throw "question_answers_incomplete";
          }
          if (typeof answer.note === "string" && answer.note.trim() !== "") {
            throw "question_answers_incomplete";
          }
        } else {
          option = question.options.find(
            (candidate: any) => candidate.position === answer.optionPosition,
          );
          if (!option || option.value !== answer.optionValue) {
            throw "question_answers_incomplete";
          }
        }
        // ADQ-FR-YQTB: the identities the backend derives, which is what makes
        // the pairs sort into the recorded order.
        const pad = String(question.position).padStart(2, "0");
        const options = question.options
          .map((candidate: any, index: number) => `${index + 1}. ${candidate.value}`)
          .join("\n");
        held.comments.push({
          id: `${set.setId}:${pad}-1q`,
          author: set.askedBy,
          body: `${question.text}\n\n${options}`,
          quotes: [],
          attachments: [],
          createdAt: new Date().toISOString(),
        });
        finalAnswerCommentId = `${set.setId}:${pad}-2a`;
        const note =
          !wroteOwn && typeof answer.note === "string" && answer.note.trim() !== ""
            ? `\n\n**Note:** ${answer.note}`
            : "";
        // ADQ-FR-RECR: the opening line says which kind of answer this is.
        const answerBody = wroteOwn
          ? `**Own answer:** ${String(answer.ownAnswer).trim()}`
          : `**Selected option:** ${answer.optionValue}${note}`;
        held.comments.push({
          id: finalAnswerCommentId,
          author: { kind: "human", login: "raver119" },
          body: answerBody,
          quotes: [],
          attachments: [],
          createdAt: new Date().toISOString(),
        });
      }
      // CMS-FR-OKMU: the set is deleted only after the append.
      delete questionSetsByThread[threadId];
      // The backend returns the thread as it **folds** — a fresh record with a
      // fresh comment list — rather than the one it was handed. The mock does
      // the same, because a consumer that memoises on the comment array would
      // otherwise never see the append.
      const folded = { ...held, comments: [...held.comments] };
      fireBus("comment-thread-changed", folded);
      fireBus("discussion-question-set-changed", { threadId, set: null });
      return { thread: folded, finalAnswerCommentId };
    }
    case "list_discussion_threads":
      // ACT-FR-22 / CMS-FR-58: keyed by target now, not by draft — a Flow tab, a
      // Diff tab and an Editor in raw-Markdown mode all read through here.
      return discussionsByTarget[discussionTargetKey(a.target)] ?? [];
    /**
     * CMS-FR-57: open a discussion over a draft as a whole and post its opening
     * message in one append. Appended to the draft's list so the margin's head
     * section grows as it would against the real backend (NAW-FR-32).
     */
    case "open_discussion_thread": {
      const target = a.target as any;
      const key = discussionTargetKey(target);
      if (!key) throw "target_required";
      if (target.kind === "draft" && !drafts.some((d) => d.id === target.draftId))
        throw "draft_not_found";
      const id = `disc-new-${++appendSeq}`;
      const opening = {
        comments: [
          {
            id: `${id}-c1`,
            author: human,
            body: String(a.body ?? ""),
            quotes: [],
            attachments: storeAttachments(a.attachments),
            createdAt: new Date().toISOString(),
          },
        ],
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      const created =
        target.kind === "draft"
          ? discussion(id, String(target.draftId), opening)
          : artifactDiscussion(id, String(target.artifactId), opening);
      discussionsByTarget[key] = [...(discussionsByTarget[key] ?? []), created];
      return created;
    }
    /**
     * CMS-FR-62 / NTS-FR-27: the note's one discussion — returned if it has one,
     * created with this opening message if it does not.
     *
     * Idempotent on purpose: a second call against a note that already has a
     * conversation returns the existing thread and appends **nothing**, so the
     * body is written only when it is genuinely the opening one. That is the
     * behaviour the panel's opening composer depends on, and a mock that
     * appended instead would make a benign double-post look like a defect.
     */
    case "get_or_create_note_discussion": {
      const noteId = String(a.noteId ?? "");
      if (!noteId) throw "note_required";
      const existing = noteDiscussions(noteId)[0];
      if (existing) return { ...existing };
      const id = `disc-note-new-${++appendSeq}`;
      const created = noteDiscussion(id, noteId, {
        comments: [
          {
            id: `${id}-c1`,
            author: human,
            body: String(a.body ?? ""),
            quotes: [],
            attachments: storeAttachments(a.attachments),
            createdAt: new Date().toISOString(),
          },
        ],
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      });
      discussionsByTarget[`note:${noteId}`] = [created];
      return created;
    }
    case "open_comment_thread":
    case "add_comment": {
      // CMS-FR-36: `draftId` alone names a discussion, so the append lands in
      // the draft's log rather than in the anchored fixtures.
      // ACT-FR-21: a reply in the floating panel lands in the *stored*
      // discussion whatever kind of target it hangs off, so the lookup is by
      // thread id across every target rather than by `draftId` alone.
      if (cmd === "add_comment") {
        const target = allDiscussions().find((t) => t.id === a.threadId);
        if (target) {
          if (target.locked) throw "thread_locked";
          const posted = {
            id: `${target.id}-a${++appendSeq}`,
            author: human,
            body: String(a.body ?? ""),
            quotes: a.quotes ?? [],
            attachments: storeAttachments(a.attachments),
            createdAt: new Date().toISOString(),
          };
          target.comments = [...target.comments, posted];
          target.updatedAt = posted.createdAt;
          return { ...target };
        }
      }
      const base =
        cmd === "open_comment_thread"
          ? thread(`t-new-${++appendSeq}`, {
              artifactId: a.artifactId,
              anchor: a.anchor,
              comments: [],
            })
          : thread(String(a.threadId ?? "t-1"));
      const appended = appendedComments[base.id] ?? [];
      const fresh = {
        id: `${base.id}-a${++appendSeq}`,
        author: human,
        body: String(a.body ?? ""),
        quotes: a.quotes ?? [],
        // CMS-FR-43: an `inline` input becomes a stored `blob`, so the card
        // renders what was posted rather than what was uploaded.
        attachments: storeAttachments(a.attachments),
        createdAt: new Date().toISOString(),
      };
      appendedComments[base.id] = [...appended, fresh];
      return {
        ...base,
        comments: [...base.comments, ...appendedComments[base.id]],
        updatedAt: fresh.createdAt,
      };
    }
    /** CMS-FR-48: the stored bytes of one blob, base64-encoded. */
    case "read_comment_attachment": {
      const blob = ATTACHMENT_BLOBS[String(a.digest ?? "")];
      // CMS-FR-48: a digest naming nothing in scope is refused, which is what
      // the card's "could not be loaded" chip renders (CMT-FR-49).
      if (!blob) throw "attachment_not_found";
      return blob;
    }
    case "set_comment_thread_lock":
    case "set_comment_thread_resolution":
    case "reanchor_comment_thread": {
      // CMS-FR-36: a `draftId` without an `artifactId` names a discussion, whose
      // stored record has to actually change — CMT-FR-56 is the resolved one
      // leaving the head section for the disclosure at the foot, and a returned
      // fixture would leave it where it was.
      const stored = allDiscussions().find((t) => t.id === a.threadId);
      if (stored) {
        if (cmd === "reanchor_comment_thread") throw "not_anchored"; // CMS-FR-59
        if (a.locked !== undefined) stored.locked = a.locked;
        if (a.resolved !== undefined) stored.resolved = a.resolved;
        stored.updatedAt = new Date().toISOString();
        return { ...stored };
      }
      return thread(String(a.threadId ?? "t-1"), {
        ...(a.locked === undefined ? {} : { locked: a.locked }),
        ...(a.resolved === undefined ? {} : { resolved: a.resolved }),
      });
    }

    // ---- progress --------------------------------------------------------
    case "list_in_flight_operations":
      return [
        { id: "op-1", kind: "scan", label: "Scanning project", state: "running", completed: 34, total: 120, sequence: 1 },
        { id: "op-2", kind: "push", label: "Pushing feature/new-artifact-window", state: "running", sequence: 2 },
      ];

    // ---- search ----------------------------------------------------------
    case "start_search": {
      const id = `search-${++searchSeq}`;
      setTimeout(() => {
        const fire = (globalThis as any).__fireBusEvent;
        fire?.("search-results", { searchId: id, hits: searchHits });
        fire?.("search-ended", { searchId: id, reason: "completed", total: searchHits.length });
      }, 30);
      return id;
    }
    case "cancel_search":
      return undefined;

    // ---- github tokens ---------------------------------------------------
    case "list_github_tokens":
      return githubTokens;
    case "add_github_token":
      return { ...githubTokens[0], id: "tok-3", label: a.label };
    case "validate_github_token":
    case "rename_github_token":
      return { ...githubTokens[0], label: a.label ?? githubTokens[0].label };
    case "remove_github_token":
    case "open_github_token_creation_page":
      return undefined;
    case "get_project_github_token_binding":
      return { tokenId: "tok-1", resolution: "bound" };
    case "set_project_github_token_binding":
      return { tokenId: a.tokenId, resolution: a.tokenId ? "bound" : "selection_required" };

    // ---- agentic integrations -------------------------------------------
    case "list_agentic_integrations":
      return agentics;
    case "set_active_agentic_integration":
      // AIC-FR-13: exactly one record is active at a time.
      for (const i of agentics) i.active = i.vendor === a.vendor;
      return agentics;
    case "clear_agentic_integration": {
      // AII-FR-26: the backend drops the record entirely and rebuilds an
      // unconfigured one from the vendor descriptor, so the credential goes
      // with it. Returning the list untouched — which this case used to do —
      // left `keyState: "set"` in place and made the "empty field, no stored
      // token" branch of AII-FR-51 unreachable in the harness.
      const record = agentics.find((i) => i.vendor === a.vendor);
      if (record) {
        record.binaryPath = null;
        record.pathOrigin = "unset";
        // `baseUrl` is left alone: for a named API vendor it is the
        // descriptor's prefilled default (AII-FR-19), which survives a clear.
        record.keyState = "unset";
        record.maskedHint = null;
        record.state = "unconfigured";
        record.version = null;
        record.verifiedAt = null;
        record.models = [];
        record.modelsOrigin = "catalog";
        record.selectedModel = null;
        record.reasoningEfforts = [];
        record.selectedEffort = null;
        // AIC-FR-10 / AIC-FR-24: clearing drops the whole record in the real
        // backend, so **both** override maps go with it. A cleared and
        // reconfigured vendor that kept its per-task selections would follow
        // choices the author can no longer see.
        record.modelOverrides = {};
        record.effortOverrides = {};
        record.active = false;
      }
      return agentics;
    }
    case "detect_agentic_cli_binary":
      return { path: "/opt/homebrew/bin/claude" };
    case "verify_agentic_integration": {
      // AIC-FR-26 / AIC-FR-27: the typed refusals, so the surface can be driven
      // through them. A malformed or missing token is rejected before anything
      // would have been stored, exactly as the backend does.
      const config = (a.config ?? {}) as Record<string, unknown>;
      const record = agentics.find((i) => i.vendor === a.vendor) ?? agentics[0];
      if (record.kind === "cli" && record.keyRequired) {
        const token = typeof config.oauthToken === "string" ? config.oauthToken.trim() : null;
        if (token === null && record.keyState !== "set") throw "token_missing";
        if (token !== null && !/^sk-ant-oat01-[A-Za-z0-9-]+$/.test(token)) throw "token_malformed";
        // A new token replaces the stored one; the hint is the only trace of it
        // that ever comes back (AIC-FR-20 / AIC-FR-28).
        if (token !== null) {
          record.keyState = "set";
          record.maskedHint = token.slice(-4);
        }
      }
      if (record.kind === "cli" && typeof config.path === "string") {
        record.binaryPath = config.path;
      }
      record.state = "verified";
      return record;
    }
    // AIC-FR-10: the model is held per turn kind over one default, on exactly
    // the terms the effort below it is — a null `turnKind` sets the default, a
    // named one sets that kind's override, and a null `modelId` beside a named
    // kind clears the entry rather than storing a null.
    //
    // The turn kind is checked **before** the value, which is the order
    // `set_model_impl` checks them in: a call carrying both an unknown kind and
    // an unknown id is `unknown_turn_kind` here as it is there, so a surface
    // tested against this mock renders the error the application would send.
    case "set_agentic_integration_model": {
      const record = agentics.find((i) => i.vendor === a.vendor) ?? agentics[0];
      const turnKind = (a.turnKind as string | null) ?? null;
      const modelId = (a.modelId as string | null) ?? null;
      if (turnKind !== null && !AGENTIC_TURN_KINDS.includes(turnKind)) {
        throw "unknown_turn_kind";
      }
      if (modelId !== null && !record.models.some((m) => m.id === modelId)) {
        throw "unknown_model";
      }
      if (turnKind === null) {
        record.selectedModel = modelId;
      } else if (modelId === null) {
        delete record.modelOverrides[turnKind];
      } else {
        record.modelOverrides[turnKind] = modelId;
      }
      return record;
    }
    // AIC-FR-10: a null `turnKind` sets the default every kind falls back to; a
    // named one sets that kind's override, and a null `effortId` beside it
    // clears the entry rather than storing a null. The turn kind is checked
    // first here for the same reason it is in the model case above.
    case "set_agentic_integration_effort": {
      const record = agentics.find((i) => i.vendor === a.vendor) ?? agentics[0];
      const turnKind = (a.turnKind as string | null) ?? null;
      const effortId = (a.effortId as string | null) ?? null;
      if (turnKind !== null && !AGENTIC_TURN_KINDS.includes(turnKind)) {
        throw "unknown_turn_kind";
      }
      if (effortId !== null && !record.reasoningEfforts.some((e) => e.id === effortId)) {
        throw "unknown_effort";
      }
      if (turnKind === null) {
        record.selectedEffort = effortId;
      } else if (effortId === null) {
        delete record.effortOverrides[turnKind];
      } else {
        record.effortOverrides[turnKind] = effortId;
      }
      return record;
    }
    case "get_project_agentic_integration":
      return { vendor: "claude_code", resolution: "inherited", overrideVendor: null };
    case "set_project_agentic_integration":
      return { vendor: a.vendor ?? null, resolution: a.vendor ? "overridden" : "inherited", overrideVendor: a.vendor ?? null };

    // ---- ai api integrations --------------------------------------------
    case "list_ai_api_integrations":
    case "set_active_ai_api_integration":
    case "clear_ai_api_integration":
      return aiApis;
    // The keychain-free view the agent surfaces read: same providers and
    // models, nothing key-derived, and `state` computed with the key assumed
    // present — so a provider whose key cannot be read is still `verified` here.
    case "list_ai_api_catalogs":
      return aiApis.map((i: Record<string, unknown>) => ({
        provider: i.provider,
        displayName: i.displayName,
        state: i.state === "key_unavailable" ? "verified" : i.state,
        models: i.models,
      }));
    // AAP-FR-APRV: the catalog of the provider active for the open project.
    case "get_active_ai_api_catalog":
      return activeAiApiResolution();
    case "verify_ai_api_integration":
    case "set_ai_api_model":
    case "set_ai_api_reasoning":
      return aiApis[0];
    // AAP-FR-FGNK: the provider's own record carries the stored value, and a
    // value outside 30 s to 3600 s is refused as the backend refuses it.
    case "set_ai_api_turn_timeout": {
      const ms = a.timeoutMs ?? null;
      if (ms !== null && (ms < 30_000 || ms > 3_600_000)) throw "turn_timeout_out_of_range";
      const record = aiApis.find((i: Record<string, unknown>) => i.provider === a.provider);
      if (!record) throw "unknown_provider";
      record.turnTimeoutMs = ms;
      return record;
    }
    case "get_project_ai_api_integration":
      return { provider: "anthropic", resolution: "inherited", overrideProvider: null };
    case "set_project_ai_api_integration":
      return { provider: a.provider ?? null, resolution: a.provider ? "overridden" : "inherited", overrideProvider: a.provider ?? null };

    // ---- conversational agents (AGR / AGC) ------------------------------
    case "list_agents":
      return sortedAgents();
    case "create_agent": {
      const draft = a.draft ?? {};
      const nickname = String(draft.nickname ?? "");
      if (
        agentsRegistry.some(
          (x) => x.nickname.toLowerCase() === nickname.toLowerCase(),
        )
      ) {
        // AGR-FR-05: knowable only to the backend, so it arrives as a typed
        // refusal rather than being caught client-side.
        throw "nickname_taken";
      }
      checkAgentModel(String(draft.modelId ?? ""));
      const created: MockAgent = {
        id: `agt-${Math.random().toString(36).slice(2, 8)}`,
        nickname,
        // AGR-FR-23: the backend trims; an absent field stores as "".
        title: String(draft.title ?? "").trim(),
        modelId: draft.modelId,
        instructions: draft.instructions ?? "",
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        reasoning: draft.reasoning ?? null,
      };
      agentsRegistry = [...agentsRegistry, created];
      return created;
    }
    case "update_agent": {
      const draft = a.draft ?? {};
      const nickname = String(draft.nickname ?? "");
      if (
        agentsRegistry.some(
          (x) =>
            x.id !== a.id &&
            x.nickname.toLowerCase() === nickname.toLowerCase(),
        )
      ) {
        throw "nickname_taken";
      }
      checkAgentModel(String(draft.modelId ?? ""));
      let updated: MockAgent | undefined;
      agentsRegistry = agentsRegistry.map((x) => {
        if (x.id !== a.id) return x;
        updated = {
          ...x,
          nickname,
          title: String(draft.title ?? "").trim(),
          modelId: draft.modelId,
          instructions: draft.instructions ?? "",
          reasoning: draft.reasoning ?? null,
          updatedAt: new Date().toISOString(),
        };
        return updated;
      });
      if (!updated) throw "agent_not_found";
      return updated;
    }
    case "delete_agent":
      // AGR-FR-11: the definition and every project's enrolment of it, in one.
      agentsRegistry = agentsRegistry.filter((x) => x.id !== a.id);
      enrolledAgentIds = enrolledAgentIds.filter((id) => id !== a.id);
      agentTurns = agentTurns.filter((t) => t.agentId !== a.id);
      return sortedAgents();
    case "list_project_agents":
      return projectAgents();
    case "enrol_project_agent":
      if (!enrolledAgentIds.includes(a.agentId)) {
        enrolledAgentIds = [...enrolledAgentIds, a.agentId];
      }
      return projectAgents();
    case "remove_project_agent":
      enrolledAgentIds = enrolledAgentIds.filter((id) => id !== a.agentId);
      agentTurns = agentTurns.filter((t) => t.agentId !== a.agentId);
      return projectAgents();
    /* AGC-FR-55: the recovery registry — at most one entry per conversation,
       each `failed` and each carrying `retryPermitted`. What a card reads when
       it mounts, so a conversation reopened while a failure stands renders its
       failed contribution and Retry again (CMT-FR-75). */
    /* AGC-FR-39 / CMT-FR-82: the turns whose images the provider and model
       could not take, most recent first. What a conversation reads when it is
       mounted or remounted, so an instance opened after the turn ended still
       says that the pictures were not sent. */
    case "list_agent_turn_image_notices":
      return a.origin
        ? imageNoticeTurns.filter(
            (t) => JSON.stringify(t.origin) === JSON.stringify(a.origin),
          )
        : imageNoticeTurns;
    case "list_recoverable_agent_turn_failures":
      return a.origin
        ? recoverableTurns.filter(
            (t) => JSON.stringify(t.origin) === JSON.stringify(a.origin),
          )
        : recoverableTurns;
    /* AGC-FR-56: a **new** turn for the failed one's agent, origin, and trigger
       comment. Refuses anything that is not the conversation's current entry,
       and appends nothing — the human comment is neither reposted nor changed. */
    case "retry_agent_turn": {
      const failed = recoverableTurns.find((t) => t.id === a.turnId);
      if (!failed) throw "turn_not_found";
      /* CMT-FR-74: the control is disabled only while the dispatch is being
         initiated, which against a synchronous mock is a single microtask and
         so unobservable in a browser. `window.__mockRetryDelayMs = 1500` holds
         the dispatch open long enough to see (and screenshot) the disabled
         control and to press it again while it is in flight. */
      if (retryDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, retryDelayMs));
      }
      if (retryRefusal) throw retryRefusal;
      recoverableTurns = recoverableTurns.filter((t) => t.id !== a.turnId);
      const turn = {
        id: `turn-${Math.random().toString(36).slice(2, 8)}`,
        agentId: failed.agentId,
        nickname: failed.nickname,
        origin: failed.origin,
        triggerCommentId: failed.triggerCommentId,
        state: "running",
        failure: null,
        retryPermitted: false,
        // AGC-FR-33: a turn that has made no tool call carries an empty list,
        // so a fresh pending contribution reads **Thinking…** (CMT-FR-80).
        activeToolCalls: [],
        startedAt: new Date().toISOString(),
        endedAt: null,
      };
      agentTurns = [turn, ...agentTurns];
      fireBus("agent-turn-state-changed", { ...turn });
      return turn;
    }
    case "list_agent_turns":
      // AGC-FR-22: a null origin returns every turn in flight anywhere, which
      // is what the chrome roster reads (AGT-FR-05).
      return a.origin
        ? agentTurns.filter(
            (t) => JSON.stringify(t.origin) === JSON.stringify(a.origin),
          )
        : agentTurns;
    case "dispatch_agent_turn": {
      // AGC-FR-05: Tauri reads `origin` with serde before the command runs, so
      // an origin in another shape fails the call with an argument error.
      const o = a.origin as Record<string, any> | undefined;
      const idField = { draft: "draftId", artifact: "artifactId", note: "noteId" }[
        String(o?.target?.kind) as "draft" | "artifact" | "note"
      ];
      if (typeof o?.discussionId !== "string" || !idField || typeof o.target[idField] !== "string") {
        throw "invalid args `origin` for command `dispatch_agent_turn`: missing field `discussionId`";
      }
      const agent = agentsRegistry.find(
        (x) => x.nickname.toLowerCase() === String(a.nickname).toLowerCase(),
      );
      if (!agent || !enrolledAgentIds.includes(agent.id)) {
        throw "agent_not_found";
      }
      if (availabilityOf(agent) !== "ready") throw "agent_unavailable";
      const turn = {
        id: `turn-${Math.random().toString(36).slice(2, 8)}`,
        agentId: agent.id,
        nickname: agent.nickname,
        origin: a.origin,
        triggerCommentId: a.triggerCommentId,
        state: "running",
        failure: null,
        retryPermitted: false,
        // AGC-FR-33: a turn that has made no tool call carries an empty list,
        // so a fresh pending contribution reads **Thinking…** (CMT-FR-80).
        activeToolCalls: [],
        startedAt: new Date().toISOString(),
        endedAt: null,
      };
      // CMT-FR-65: the question has now been answered, so the `awaiting_reply`
      // turn that obliged this dispatch stops being outstanding — otherwise the
      // agent would be re-dispatched by every later comment in the thread.
      agentTurns = agentTurns.filter(
        (t) =>
          !(
            t.state === "awaiting_reply" &&
            t.agentId === agent.id &&
            JSON.stringify(t.origin) === JSON.stringify(a.origin)
          ),
      );
      agentTurns = [turn, ...agentTurns];
      // AGC-FR-22: the backend publishes the registration event before the
      // dispatch call resolves, so the frontend sees the turn twice in no fixed
      // order. The mock does the same, so that race is reachable in a browser.
      fireBus("agent-turn-state-changed", { ...turn });
      return turn;
    }
    case "cancel_agent_turn": {
      const turn = agentTurns.find((t) => t.id === a.turnId);
      agentTurns = agentTurns.filter((t) => t.id !== a.turnId);
      if (!turn) throw "agent_not_found";
      const cancelled = {
        ...turn,
        state: "cancelled",
        endedAt: new Date().toISOString(),
      };
      // AGC-FR-22: `cancel_impl` publishes the terminated turn as well.
      fireBus("agent-turn-state-changed", { ...cancelled });
      return cancelled;
    }

    /* GitHub publication (GHP-github-publication.md) — the commands the New
       Artifact tab's publication tag and band read and act through. See the
       seed above for the URL flags that force a condition. */
    case "get_draft_publication":
      return publicationView(String(a.draftId ?? a.id ?? "d-1"));
    case "list_publication_remotes":
      return publicationRemotes;
    /* GHP-FR-MDLD: the chooser's read. `?publicationParentsFail`,
       `?publicationTypesFail`, and `?publicationMilestonesEmpty` force the
       failed and empty states of NAW-FR-WMEP. */
    case "load_publication_metadata": {
      const loaded = (items: unknown[]) => ({
        state: "loaded",
        items,
        errorCode: null,
        error: null,
      });
      const failed = (errorCode: string, error: string) => ({
        state: "failed",
        items: [],
        errorCode,
        error,
      });
      return {
        repositoryOwner: "acme",
        repositoryName: "acme-app",
        settings: publicationSettings,
        parents: harnessFlag("publicationParentsFail")
          ? failed("parent_issues_unreadable", "The open issues of this repository could not be read.")
          : loaded([
              {
                number: 412,
                title: "Window chrome",
                issueType: "Feature",
                url: "https://github.com/acme/acme-app/issues/412",
                milestone: { number: 7, title: "v1.2" },
              },
              {
                number: 398,
                title: "Release pipeline",
                issueType: "Feature",
                url: "https://github.com/acme/acme-app/issues/398",
                milestone: null,
              },
            ]),
        issueTypes: harnessFlag("publicationTypesFail")
          ? failed("issue_types_unreadable", "The issue Types of this repository could not be read.")
          : loaded([{ name: "Feature" }, { name: "Task" }, { name: "Bug" }]),
        milestones: harnessFlag("publicationMilestonesEmpty")
          ? loaded([])
          : loaded([
              { number: 7, title: "v1.2" },
              { number: 8, title: "v1.3" },
            ]),
        subIssueType: { name: publicationSettings.subIssueType, resolved: publicationSettings.subIssueType },
      };
    }
    /* GHP-FR-KVRH / GHP-FR-NQWX / GHP-FR-PTYL: the Publication group of the
       GitHub Project settings section. */
    case "get_github_publication_settings":
      return publicationSettings;
    case "set_github_publication_settings":
      if (!Array.isArray(a.parentIssueTypes) || a.parentIssueTypes.length === 0 || !a.subIssueType) {
        // eslint-disable-next-line no-throw-literal
        throw "invalid_publication_settings";
      }
      publicationSettings = {
        parentIssueTypes: a.parentIssueTypes,
        subIssueType: a.subIssueType,
        subIssueMilestonePolicy: a.subIssueMilestonePolicy,
      };
      return publicationSettings;
    case "list_github_issue_types":
      return harnessFlag("publicationTypesFail")
        ? {
            state: "failed",
            items: [],
            errorCode: "issue_types_unreadable",
            error: "The issue Types of this repository could not be read.",
          }
        : {
            state: "loaded",
            items: [{ name: "Feature" }, { name: "Task" }, { name: "Bug" }],
            errorCode: null,
            error: null,
          };
    case "publish_draft_to_github": {
      const id = String(a.draftId ?? a.id ?? "d-1");
      // GHP-FR-RUYT: the attempt is on disk BEFORE the first GitHub request, so
      // a request that fails leaves an attempt standing — which is what keeps
      // the band's retry affordance in place (NAW-FR-HZSW).
      publicationState(id).attempt ??= publicationAttempt("open");
      if (harnessFlag("publicationRetryFails")) {
        // eslint-disable-next-line no-throw-literal
        throw "github_unreachable";
      }
      return publicationSettle(id);
    }
    /* GHP-FR-HRUN: a retry over an attempt that is waiting on a choice searches
       for the marker again and answers `recoveryRequired`, which is what opens
       the recovery dialog. */
    case "retry_draft_publication": {
      const id = String(a.draftId ?? a.id ?? "d-1");
      if (harnessFlag("publicationRetryFails")) {
        // eslint-disable-next-line no-throw-literal
        throw "github_unreachable";
      }
      const state = publicationState(id);
      if (state.attempt?.state === "awaiting_choice") {
        return {
          kind: "recoveryRequired",
          issueNumber: PUBLICATION_RECOVERY_ISSUE,
          issueUrl: `https://github.com/acme/acme-app/issues/${PUBLICATION_RECOVERY_ISSUE}`,
          marker: state.attempt.marker,
          mismatches: ["body"],
        };
      }
      return publicationSettle(id);
    }
    /**
     * GHP-FR-YPGL: the two answers reach different issues. `update_existing`
     * edits the issue the marker was found on, so the record keeps that number
     * and that marker; `publish_new` leaves it alone and creates another issue.
     * A harness that settled both the same way would make the choice
     * unverifiable, the two buttons being the only thing that differs.
     */
    case "resolve_draft_publication_conflict":
      return publicationSettle(
        String(a.draftId ?? a.id ?? "d-1"),
        a.choice === "update_existing" ? PUBLICATION_RECOVERY_ISSUE : undefined,
      );
    /* GHP-FR-EBSA: cancelling the choice leaves the attempt recoverable and
       adds no history entry. */
    case "cancel_draft_publication_conflict":
      return undefined;
    case "cancel_draft_publication_attempt": {
      const id = String(a.draftId ?? a.id ?? "d-1");
      publicationState(id).attempt = null;
      publicationChanged(id);
      return undefined;
    }
    /* GHP-FR-MJTB: the issue opens outside the application, so nothing in the
       window changes — the harness records the call and no more. */
    case "open_publication_issue":
      return undefined;

    /* Session logging (LGC-logging.md). Filtering, searching and paging all
       happen here, exactly as they do in Rust — a harness that narrowed in the
       panel instead would make LOG-FR-06 untestable in the browser. */
    case "append_log_records": {
      for (const r of (a.records ?? []) as Omit<MockLogRecord, "sequence">[]) {
        if (!r.domains?.length) continue;
        logBuffer.push({ ...r, sequence: logSequence++ });
      }
      while (logBuffer.length > 20000) {
        logBuffer.shift();
        logDropped += 1;
      }
      // `__fireBusEvent` is the harness's own emitter seam (see mock-event.ts),
      // so an append here reaches a subscribed panel exactly as the Tauri event
      // bus would.
      queueMicrotask(() => {
        const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
          | ((name: string, payload?: unknown) => void)
          | undefined;
        fire?.("log-records-appended", {
          generation: logGeneration,
          bufferTotal: logBuffer.length,
          droppedTotal: logDropped,
          highestSequence: logBuffer.at(-1)?.sequence ?? null,
        });
      });
      return undefined;
    }
    case "query_logs": {
      const matched = logBuffer.filter((r) => logMatches(r, a.filter ?? {}));
      const limit: number = a.limit ?? 200;
      const cursor = a.cursor;
      let records: MockLogRecord[];
      if (cursor && "after" in cursor) {
        records = matched.filter((r) => r.sequence > cursor.after).slice(0, limit);
      } else if (cursor && "before" in cursor) {
        const below = matched.filter((r) => r.sequence < cursor.before);
        records = below.slice(Math.max(0, below.length - limit));
      } else {
        records = matched.slice(Math.max(0, matched.length - limit));
      }
      return {
        records,
        generation: logGeneration,
        matchedTotal: matched.length,
        bufferTotal: logBuffer.length,
        droppedTotal: logDropped,
        highestSequence: logBuffer.at(-1)?.sequence ?? null,
      };
    }
    case "export_logs":
      return logBuffer.filter((r) => logMatches(r, a.filter ?? {})).length;

    default:
      // eslint-disable-next-line no-console
      console.warn("[uiaudit] unmocked command:", cmd, a);
      return undefined;
  }
}

export const convertFileSrc = (p: string) => p;
export const transformCallback = () => 0;
export class Channel {}
export class PluginListener {}
export async function addPluginListener() {
  return new PluginListener();
}
void now;
