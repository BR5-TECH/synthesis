import { createHistory } from "../editHistory";
import { createFindState } from "../findState";
import { DEFAULT_INDENTATION } from "../indentation";
import { isMarkdownFile } from "../syntaxHighlight";
import type { EditSession } from "./types";

export function newSession(artifactId: string): EditSession {
  return {
    artifactId,
    history: createHistory(""),
    buffer: "",
    baseline: null,
    pending: null,
    dirty: false,
    conflict: false,
    // ESH-FR-SSDV: the file's name decides which surface it is edited on. A
    // Markdown file opens in WYSIWYG the first time (EDT-FR-17); a source file
    // has one surface and starts — and stays — on it. Recording that here rather
    // than in the Editor is what lets everything downstream of the record read
    // one answer: the rail's placement (CMT-FR-02), where discussions are read
    // (ACT-FR-19), and which surface a history step names (EDT-FR-25).
    mode: isMarkdownFile(artifactId) ? "wysiwyg" : "text",
    indentation: DEFAULT_INDENTATION,
    indentationOverridden: false,
    find: createFindState(),
    railOpen: null,
    resolvedOpen: false,
    error: null,
    loaded: false,
    revalidate: false,
    tabOpen: false,
    heldWrite: false,
    pendingLoad: null,
    writeChain: Promise.resolve(),
    seedToken: 0,
    quiesced: false,
  };
}
