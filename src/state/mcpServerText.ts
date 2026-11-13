/**
 * The words of the MCP servers section (`MSS-mcp-servers-settings.md`): the
 * status chip, the failure lines, the validation texts, and the texts of a
 * refusal.
 *
 * Pure, so each text is testable without a render. The words belong to the
 * section: no text is composed from a URL, a command, a header, or a secret.
 */
import type {
  McpAuthMode,
  McpConfigError,
  McpFailure,
  McpScope,
  McpServerEntry,
  McpServerState,
  McpServerStatus,
  McpTestResult,
} from "../types";

/** MSS-FR-PEGO: the text of the status chip. */
export function statusChipText(status: McpServerStatus): string {
  switch (status.state) {
    case "disabled":
      return "Off";
    case "invalid":
      return "Invalid";
    case "untrusted":
      return "Not trusted";
    case "needs_secret":
      return "Needs a secret";
    case "needs_sign_in":
      return "Sign in needed";
    case "signing_in":
      return "Signing in…";
    case "connecting":
      return "Connecting…";
    case "ready":
      return `Connected · ${status.toolCount} tools`;
    case "failed":
      return "Failed";
  }
}

/**
 * The colour of the status chip (MSS layout notes): green for ready, amber for
 * a state that needs the author, red for failed, and grey for off. A server
 * that connects needs nothing from the author, so it is grey too.
 */
export type ChipTone = "ok" | "warn" | "danger" | "neutral";

export function statusChipTone(state: McpServerState): ChipTone {
  switch (state) {
    case "ready":
      return "ok";
    case "failed":
      return "danger";
    case "disabled":
    case "connecting":
      return "neutral";
    default:
      return "warn";
  }
}

/** MSS-FR-VUWB: the text of each failure class, exactly. */
export const FAILURE_TEXT: Readonly<Record<McpFailure, string>> = {
  command_not_found: "The command was not found on this computer.",
  process_exited: "The server process stopped.",
  unreachable: "The server cannot be reached.",
  tls_untrusted: "The server certificate is not trusted.",
  rejected: "The server refused the credentials.",
  protocol_error: "The server did not answer as an MCP server.",
  timed_out: "The server did not answer in time.",
  oauth_failed: "The sign-in did not complete.",
  keychain_unavailable: "The system keychain is not available.",
};

/** MSS-FR-OKDG, MSS-FR-YIMR, MSS-FR-BEQD: the text of each validation code. */
export const CONFIG_ERROR_TEXT: Readonly<Record<McpConfigError, string>> = {
  invalid_id:
    "Use 1 to 32 lowercase letters, digits, and hyphens. Start with a letter or a digit.",
  duplicate_id: "Another server of this project has this ID.",
  invalid_name: "Enter a name of 1 to 80 characters.",
  invalid_transport: "Choose a transport.",
  invalid_command: "Enter a command.",
  invalid_cwd: "Use a relative path to a folder in the project.",
  invalid_url: "Enter an https URL.",
  invalid_env_name: "Use letters, digits, and underscores. Do not start with a digit.",
  invalid_header_name: "Use a valid HTTP header name.",
  invalid_header_value: "A header value cannot hold a line break.",
  duplicate_name: "This name is in the list more than one time.",
  authorization_header_conflict:
    "Remove this header. The authentication sets the Authorization header.",
  invalid_callback_port: "Enter a port from 1024 to 65535.",
  invalid_auth: "Choose an authentication.",
};

/** MSS-FR-GHDO: a plain HTTP URL to another computer. */
export const USE_HTTPS_TEXT = "Use https for a server on another computer.";

/** MSS-FR-YIMR: an invalid record that names no validation code. */
export const INVALID_RECORD_TEXT = "The server record is not valid.";

/** MSS-FR-UVWX: the trust dialog showed a record that is not the record in the file now. */
export const RECORD_CHANGED_TEXT = "The record changed. Close this and look at it again.";

/** MSS-FR-HIWH, MSS-FR-SCFN: the words of each authentication mode. */
export const AUTH_LABEL: Readonly<Record<McpAuthMode, string>> = {
  none: "None",
  bearer: "Bearer token",
  oauth2: "OAuth 2",
};

/** MSS-FR-PDVK: the line of a server that needs a secret. */
export const NEEDS_SECRET_TEXT = "Add the secrets this server needs on this computer.";

/** MSS-FR-HXVP: the line of a server while the browser sign-in runs. */
export const WAITING_FOR_BROWSER_TEXT = "Waiting for the browser…";

/**
 * MSS-FR-TMUX, MSS-FR-VWKQ: the text of a test that the backend refused for the status of
 * the server (MCS-FR-ERQP). No connection was opened.
 */
const TEST_STATE_TEXT: Partial<Record<McpServerState, string>> = {
  // MSS-FR-TMUX: exactly these.
  disabled: "The server is off.",
  invalid: INVALID_RECORD_TEXT,
  untrusted: "Trust this server before it runs.",
  needs_secret: NEEDS_SECRET_TEXT,
  needs_sign_in: "Sign in to this server first.",
  signing_in: "The sign-in is not complete.",
  connecting: "The server is connecting.",
};

/** The code the backend refuses with, as text. */
export function errorCode(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return String(e);
}

/**
 * MSS-FR-VSMZ: the code of an error for a log record. A text that is not a
 * code of the contract surface can hold a path, so it is logged as `error`.
 */
export function logCode(code: string): string {
  return /^[a-z_]{1,40}$/.test(code) ? code : "error";
}

const isFailure = (code: string): code is McpFailure => code in FAILURE_TEXT;
const isConfigError = (code: string): code is McpConfigError => code in CONFIG_ERROR_TEXT;

/** The text of a refused command: a code of the contract surface, or the raw text. */
export function refusalText(code: string): string {
  if (isConfigError(code)) return CONFIG_ERROR_TEXT[code];
  if (isFailure(code)) return FAILURE_TEXT[code];
  switch (code) {
    case "no_project_open":
      return "No project is open.";
    case "malformed_project_config":
      return "The project settings file cannot be read. Correct .synthesis/project.toml.";
    case "not_found":
      return "This server is not in the project settings now.";
    case "unknown_slot":
      return "This server does not use this secret.";
    case "invalid_secret":
      return "The value cannot hold a line break.";
    case "record_changed":
      // MSS-FR-UVWX
      return RECORD_CHANGED_TEXT;
    case "no_credentials":
    case "trust_not_needed":
      return "This server needs no trust step.";
    case "untrusted":
      return "Trust this server before you sign in.";
    case "not_oauth2":
      return "This server does not use OAuth 2.";
    case "sign_in_in_progress":
      return "A sign-in is already in progress.";
    default:
      return code;
  }
}

/** MSS-FR-VWKQ: the text of a test result. */
export function testResultText(result: McpTestResult): string {
  if (result.ok) return `Connected. ${result.tools.length} tools.`;
  const failure = result.failure;
  if (isFailure(failure)) return FAILURE_TEXT[failure];
  return TEST_STATE_TEXT[failure] ?? failure;
}

/** MSS-FR-YIMR: the line under a failed or an invalid row, or null. */
export function rowProblemText(entry: McpServerEntry): string | null {
  if (entry.status.state === "failed") return FAILURE_TEXT[entry.status.failure];
  // MSS-FR-RAYC: a personal record whose ID another server has.
  if (entry.status.state === "invalid" && entry.scope !== "team" && entry.invalid === "duplicate_id") {
    return SHADOWED_TEXT;
  }
  if (entry.status.state === "invalid") {
    return entry.invalid ? CONFIG_ERROR_TEXT[entry.invalid] : INVALID_RECORD_TEXT;
  }
  return null;
}

/** MSS-FR-QMTG: the first line of a tool description. */
export function firstLine(text: string): string {
  return text.split(/\r?\n/).find((line) => line.trim() !== "")?.trim() ?? "";
}

/** MSS-FR-RAYC */
export const SHADOWED_TEXT =
  "A team server, or another server of yours, has this ID. Remove this one and add it again with another ID.";

/** MSS-FR-POYD: the headings, the sentences, and the empty lines of the two groups. */
export const TEAM_GROUP = {
  heading: "Team servers",
  sentence:
    "Saved in the project and shared with everyone who works on it. You can turn each one off on this computer.",
  empty: "No team servers.",
} as const;

export const PERSONAL_GROUP = {
  heading: "Your servers",
  sentence: "Saved on this computer only. Nobody else sees them.",
  empty: "No servers of your own.",
} as const;

/** MSS-FR-SAKG: the scope badge of a personal row. */
export const SCOPE_BADGE: Readonly<Record<Exclude<McpScope, "team">, string>> = {
  personal_project: "This project",
  personal_all: "All projects",
};

/** MSS-FR-SCDK: the `Who uses it` choice, and the text of the scope on edit. */
export const SCOPE_LABEL: Readonly<Record<McpScope, string>> = {
  team: "The team (saved in the project)",
  personal_project: "Only me, in this project",
  personal_all: "Only me, in all my projects",
};

/**
 * MSS-FR-OOUI: the line of a team row whose switch on this computer is not
 * the team's value. Null when they are the same, or when the row is not a
 * team row with a record.
 */
export function switchNoteText(entry: McpServerEntry): string | null {
  if (entry.scope !== "team" || !entry.record) return null;
  if (entry.enabledHere === entry.record.enabled) return null;
  return entry.enabledHere
    ? "On on this computer. The team has it off."
    : "Off on this computer. The team has it on.";
}

/** MSS-FR-UYFX: the text of the remove confirmation. */
export function removeConfirmText(scope: McpScope, name: string): string {
  const tail = "Its secrets and its sign-in on this computer are removed too.";
  switch (scope) {
    case "team":
      return `Remove ${name} from the project for everyone? ${tail}`;
    case "personal_all":
      return `Remove ${name} from all your projects? ${tail}`;
    default:
      return `Remove ${name}? ${tail}`;
  }
}
