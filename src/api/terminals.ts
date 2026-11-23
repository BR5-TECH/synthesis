/**
 * The terminal session commands (`../../specifications/core/TRS-terminal-sessions.md`).
 */
import { Channel } from "@tauri-apps/api/core";
import { invoke } from "./invoke";
import type {
  CliVendor,
  TargetRef,
  TerminalAttachment,
  TerminalKind,
  TerminalSummary,
  TerminalTarget,
  TerminalThemeId,
} from "../types";

/** TRS-FR-AVNH: the most bytes one input call carries. */
export const TERMINAL_INPUT_LIMIT = 64 * 1024;

export const listTerminalTargets = () => invoke<TerminalTarget[]>("list_terminal_targets");

export const listTerminals = () => invoke<TerminalSummary[]>("list_terminals");

export const createTerminal = (target: TargetRef, kind: TerminalKind, vendor: CliVendor | null) =>
  invoke<TerminalSummary>("create_terminal", { target, kind, vendor });

/**
 * TRS-FR-HIIK: attaches a channel to the session. Each message is one frame of
 * raw output bytes: the history first, then the live output.
 */
export const attachTerminal = (terminalId: string, onFrame: (frame: Uint8Array) => void) => {
  const output = new Channel<ArrayBuffer | number[]>();
  output.onmessage = (frame) => {
    onFrame(frame instanceof Uint8Array ? frame : new Uint8Array(frame as ArrayBuffer));
  };
  return invoke<TerminalAttachment>("attach_terminal", { terminalId, output });
};

export const detachTerminal = (terminalId: string) => invoke<void>("detach_terminal", { terminalId });

export const writeTerminalInput = (terminalId: string, data: string) =>
  invoke<void>("write_terminal_input", { terminalId, data });

export const resizeTerminal = (terminalId: string, cols: number, rows: number) =>
  invoke<void>("resize_terminal", { terminalId, cols, rows });

export const acknowledgeTerminalOutput = (terminalId: string, bytes: number) =>
  invoke<void>("acknowledge_terminal_output", { terminalId, bytes });

export const setTerminalPinned = (terminalId: string, pinned: boolean) =>
  invoke<TerminalSummary>("set_terminal_pinned", { terminalId, pinned });

/** TRS-FR-BTMT. */
export const setTerminalLabel = (terminalId: string, label: string) =>
  invoke<TerminalSummary>("set_terminal_label", { terminalId, label });

/** TRS-FR-DOMD: null clears the override. */
export const setTerminalTheme = (terminalId: string, theme: TerminalThemeId | null) =>
  invoke<TerminalSummary>("set_terminal_theme", { terminalId, theme });

export const closeTerminal = (terminalId: string) => invoke<void>("close_terminal", { terminalId });
