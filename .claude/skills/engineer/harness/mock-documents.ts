/*
 * Documents collection stand-in (DCL-documents-collection.md), for UI auditing
 * only.
 *
 * The collection starts empty, so the Documents panel shows its empty state and
 * the Add documents choice (DPN-FR-UKXW, DPN-FR-ZMBQ). A pick adds one seeded
 * source: Files adds one Markdown file, Folder adds a folder with a Markdown
 * file, a text file and a PDF. `?documentsSeeded` starts with the folder.
 */

import { NOT_HANDLED } from "./mock-github-polling";

type Source = { kind: "file" | "folder"; path: string; status: "available" };
type Entry = {
  id: string;
  path: string;
  name: string;
  format: "pdf" | "markdown" | "text";
  status: "available";
  revision: string;
};

const FILE_SOURCE: Source = {
  kind: "file",
  path: "/Users/demo/notes/meeting.md",
  status: "available",
};
const FOLDER_SOURCE: Source = {
  kind: "folder",
  path: "/Users/demo/reference",
  status: "available",
};

function entry(path: string, format: Entry["format"], hex: string): Entry {
  return {
    id: `doc-${hex.repeat(32 / hex.length)}`,
    path,
    name: path.split("/").pop() ?? path,
    format,
    status: "available",
    revision: "r1",
  };
}

const BY_SOURCE: Record<string, Entry[]> = {
  [FILE_SOURCE.path]: [entry(FILE_SOURCE.path, "markdown", "a1")],
  [FOLDER_SOURCE.path]: [
    entry(`${FOLDER_SOURCE.path}/guide.md`, "markdown", "b2"),
    entry(`${FOLDER_SOURCE.path}/notes.txt`, "text", "c3"),
    entry(`${FOLDER_SOURCE.path}/paper.pdf`, "pdf", "d4"),
  ],
};

const sources: Source[] = new URLSearchParams(location.search).has(
  "documentsSeeded",
)
  ? [FOLDER_SOURCE]
  : [];

function snapshot() {
  return {
    sources: sources.map((s) => ({ ...s })),
    documents: sources.flatMap((s) => BY_SOURCE[s.path] ?? []),
  };
}

export function documentsInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_documents":
      return snapshot();
    case "pick_document_sources": {
      const source = a.mode === "folder" ? FOLDER_SOURCE : FILE_SOURCE;
      if (!sources.some((s) => s.path === source.path)) sources.push(source);
      return { cancelled: false, ignored_count: 0, snapshot: snapshot() };
    }
    case "remove_document_source": {
      const at = sources.findIndex((s) => s.path === a.path);
      if (at >= 0) sources.splice(at, 1);
      return snapshot();
    }
    case "read_document": {
      const doc = snapshot().documents.find((d) => d.id === a.id);
      if (!doc) throw "unknown_document";
      return {
        id: doc.id,
        name: doc.name,
        format: doc.format,
        text: `# ${doc.name}\n\nSeeded content of the harness.\n`,
        revision: doc.revision,
      };
    }
    default:
      return NOT_HANDLED;
  }
}
