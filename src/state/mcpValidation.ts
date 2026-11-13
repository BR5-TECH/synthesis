/**
 * The form of the MCP server dialog and its checks (`MSS-mcp-servers-settings.md`
 * MSS-FR-MWAV through MSS-FR-GHDO).
 *
 * The dialog checks the fields as the core does (MCS-FR-LLQD, MCS-FR-KALQ,
 * MCS-FR-KLQS, MCS-FR-DINZ, MCS-FR-XDGN, MCS-FR-FJLR, MCS-FR-UAAN), so a record
 * that the core would refuse is not sent. The core stays the authority: a
 * refusal it returns shows under its field too (MSS-FR-UWZQ).
 *
 * Pure: no render, no backend.
 */
import type {
  McpAuthMode,
  McpConfigError,
  McpSecretPresence,
  McpServer,
} from "../types";
import { CONFIG_ERROR_TEXT, USE_HTTPS_TEXT } from "./mcpServerText";

/** MSS-FR-BHNZ: one row of the environment list or of the header list. */
export interface McpListRow {
  /** A key of the dialog alone, so React keeps a row when its name changes. */
  key: string;
  name: string;
  /** A plain value, or the typed value of a secret. Empty keeps a stored secret. */
  value: string;
  secret: boolean;
  /** MSS-FR-IUMU: clear the stored value when the dialog saves. */
  clear: boolean;
}

/** MSS-FR-IUMU: a secret field that is not a list row. */
export interface McpSecretInput {
  value: string;
  clear: boolean;
}

export type McpTransportKind = "stdio" | "http";

/** The fields of the dialog, as the author typed them. */
export interface McpServerForm {
  name: string;
  id: string;
  /** MSS-FR-DVDX: the author edited `ID`, so it no longer follows `Name`. */
  idEdited: boolean;
  transport: McpTransportKind;
  command: string;
  /** MSS-FR-XMNV: one argument on each line. */
  argsText: string;
  cwd: string;
  env: McpListRow[];
  url: string;
  auth: McpAuthMode;
  clientId: string;
  /** MSS-FR-HIWH: separated by spaces. */
  scopesText: string;
  callbackPortText: string;
  headers: McpListRow[];
  bearer: McpSecretInput;
  clientSecret: McpSecretInput;
}

/**
 * The field a problem shows under: a top-level field, or one row of a list
 * (`env:<row key>`, `header:<row key>`).
 */
export type McpFieldKey = string;

export type McpFormProblems = Record<McpFieldKey, string>;

const ID_PATTERN = /^[a-z0-9][a-z0-9-]{0,31}$/;
const ENV_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;
/** RFC 9110 token: the shape of an HTTP header name. */
const HEADER_NAME_PATTERN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]", "::1"]);

let rowCounter = 0;

/** A new empty list row. */
export function newRow(over: Partial<McpListRow> = {}): McpListRow {
  rowCounter += 1;
  return { key: `row-${rowCounter}`, name: "", value: "", secret: false, clear: false, ...over };
}

const NO_SECRET: McpSecretInput = { value: "", clear: false };

/** MSS-FR-MWAV: the empty dialog of `Add server`. */
export function emptyForm(): McpServerForm {
  return {
    name: "",
    id: "",
    idEdited: false,
    transport: "stdio",
    command: "",
    argsText: "",
    cwd: "",
    env: [],
    url: "",
    auth: "none",
    clientId: "",
    scopesText: "",
    callbackPortText: "",
    headers: [],
    bearer: NO_SECRET,
    clientSecret: NO_SECRET,
  };
}

function rowsOf(plain: Record<string, string>, secretNames: string[]): McpListRow[] {
  return [
    ...Object.entries(plain).map(([name, value]) => newRow({ name, value })),
    ...secretNames.map((name) => newRow({ name, secret: true })),
  ];
}

/** MSS-FR-MWAV: the dialog of `Edit`, filled with the record of the row. */
export function formFromRecord(record: McpServer): McpServerForm {
  const form = { ...emptyForm(), name: record.name, id: record.id, idEdited: true };
  const t = record.transport;
  if (t.kind === "stdio") {
    return {
      ...form,
      transport: "stdio",
      command: t.command,
      argsText: t.args.join("\n"),
      cwd: t.cwd ?? "",
      env: rowsOf(t.env, t.secretEnv),
    };
  }
  return {
    ...form,
    transport: "http",
    url: t.url,
    auth: t.auth,
    clientId: t.clientId ?? "",
    scopesText: t.scopes.join(" "),
    callbackPortText: t.callbackPort === null ? "" : String(t.callbackPort),
    headers: rowsOf(t.headers, t.secretHeaders),
  };
}

/** MSS-FR-DVDX: the slug of a name, as an ID that MCS-FR-LLQD accepts where it can. */
export function slugify(name: string): string {
  return name
    .normalize("NFKD")
    .toLowerCase()
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+/, "")
    .slice(0, 32)
    .replace(/-+$/, "");
}

/** A row that the author left with no name and no value says nothing. */
const isBlank = (row: McpListRow) => row.name.trim() === "" && row.value === "" && !row.secret;

const usedRows = (rows: McpListRow[]) => rows.filter((row) => !isBlank(row));

function argsOf(text: string): string[] {
  return text
    .split(/\r?\n/)
    .filter((line) => line.trim() !== "");
}

function scopesOf(text: string): string[] {
  return text.split(/\s+/).filter((scope) => scope !== "");
}

function cwdOf(text: string): string | null {
  const trimmed = text.trim();
  return trimmed === "" ? null : trimmed;
}

/** MCS-FR-UAAN: an empty field is no port. */
function portOf(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  // Digits alone: `Number` would also take "0x400" or "1e3".
  return /^\d+$/.test(trimmed) ? Number(trimmed) : Number.NaN;
}

/**
 * MSS-FR-BHNZ: the record the dialog saves. `base` is the record that was
 * edited, so `enabled` and `disabledTools` stay as they were.
 */
export function buildRecord(form: McpServerForm, base: McpServer | null): McpServer {
  const plain = (rows: McpListRow[]) =>
    Object.fromEntries(usedRows(rows).filter((r) => !r.secret).map((r) => [r.name.trim(), r.value]));
  const secretNames = (rows: McpListRow[]) =>
    usedRows(rows).filter((r) => r.secret).map((r) => r.name.trim());
  const common = {
    id: form.id.trim(),
    name: form.name.trim(),
    enabled: base?.enabled ?? true,
    disabledTools: base?.disabledTools ?? [],
  };
  if (form.transport === "stdio") {
    return {
      ...common,
      transport: {
        kind: "stdio",
        command: form.command.trim(),
        args: argsOf(form.argsText),
        cwd: cwdOf(form.cwd),
        env: plain(form.env),
        secretEnv: secretNames(form.env),
      },
    };
  }
  const oauth = form.auth === "oauth2";
  const clientId = form.clientId.trim();
  return {
    ...common,
    transport: {
      kind: "http",
      url: form.url.trim(),
      headers: plain(form.headers),
      secretHeaders: secretNames(form.headers),
      auth: form.auth,
      clientId: oauth && clientId !== "" ? clientId : null,
      scopes: oauth ? scopesOf(form.scopesText) : [],
      callbackPort: oauth ? portOf(form.callbackPortText) : null,
    },
  };
}

/** MCS-FR-KLQS: a relative path that stays inside the worktree. */
export function cwdStaysInside(cwd: string): boolean {
  if (cwd.startsWith("/") || cwd.startsWith("\\") || /^[A-Za-z]:/.test(cwd)) return false;
  let depth = 0;
  for (const part of cwd.split(/[/\\]/)) {
    if (part === "..") depth -= 1;
    else if (part !== "" && part !== ".") depth += 1;
    if (depth < 0) return false;
  }
  return true;
}

/**
 * MCS-FR-XDGN / MSS-FR-GHDO: the problem of a URL, or null. A plain HTTP URL to
 * another computer gets the text of MSS-FR-GHDO.
 */
export function urlProblem(text: string): string | null {
  const trimmed = text.trim();
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    return CONFIG_ERROR_TEXT.invalid_url;
  }
  if (url.protocol === "https:" && url.hostname !== "") return null;
  if (url.protocol === "http:") {
    return LOCAL_HOSTS.has(url.hostname) ? null : USE_HTTPS_TEXT;
  }
  return CONFIG_ERROR_TEXT.invalid_url;
}

/**
 * MCS-FR-KALQ, MCS-FR-DINZ, MCS-FR-FJLR: the problems of each row of a list.
 * A header name is compared in any letter case, as HTTP does.
 */
function rowProblems(
  rows: McpListRow[],
  prefix: "env" | "header",
  authorizationConflict: boolean,
): McpFormProblems {
  const problems: McpFormProblems = {};
  const seen = new Set<string>();
  const pattern = prefix === "env" ? ENV_NAME_PATTERN : HEADER_NAME_PATTERN;
  const invalid: McpConfigError = prefix === "env" ? "invalid_env_name" : "invalid_header_name";
  for (const row of usedRows(rows)) {
    const name = row.name.trim();
    const field = `${prefix}:${row.key}`;
    const same = prefix === "env" ? name : name.toLowerCase();
    if (!pattern.test(name)) problems[field] = CONFIG_ERROR_TEXT[invalid];
    else if (seen.has(same)) problems[field] = CONFIG_ERROR_TEXT.duplicate_name;
    else if (authorizationConflict && same === "authorization") {
      problems[field] = CONFIG_ERROR_TEXT.authorization_header_conflict;
    } else if (prefix === "header" && /[\r\n]/.test(row.value)) {
      // MCS-FR-IXHD for a secret value, MCS-FR-DINZ for a plain one.
      problems[field] = row.secret
        ? SECRET_LINE_BREAK_TEXT
        : CONFIG_ERROR_TEXT.invalid_header_value;
    }
    seen.add(same);
  }
  return problems;
}

/** MCS-FR-IXHD: a header or bearer value with a line break is `invalid_secret`. */
export const SECRET_LINE_BREAK_TEXT = "The value cannot hold a line break.";

export interface ValidateContext {
  /** The IDs of the other servers of the project (MCS-FR-LLQD). */
  otherIds: string[];
  /** MSS-FR-DVDX: on edit, the ID is fixed and is not checked again. */
  isNew: boolean;
}

/** MSS-FR-BEQD: every problem of the form, under its field. Empty when it can be saved. */
export function validateForm(form: McpServerForm, ctx: ValidateContext): McpFormProblems {
  const problems: McpFormProblems = {};
  const name = form.name.trim();
  if (name.length < 1 || [...name].length > 80) problems.name = CONFIG_ERROR_TEXT.invalid_name;
  if (ctx.isNew) {
    const id = form.id.trim();
    if (!ID_PATTERN.test(id)) problems.id = CONFIG_ERROR_TEXT.invalid_id;
    else if (ctx.otherIds.includes(id)) problems.id = CONFIG_ERROR_TEXT.duplicate_id;
  }
  if (form.transport === "stdio") {
    if (form.command.trim() === "") problems.command = CONFIG_ERROR_TEXT.invalid_command;
    const cwd = cwdOf(form.cwd);
    if (cwd !== null && !cwdStaysInside(cwd)) problems.cwd = CONFIG_ERROR_TEXT.invalid_cwd;
    Object.assign(problems, rowProblems(form.env, "env", false));
    return problems;
  }
  const url = urlProblem(form.url);
  if (url) problems.url = url;
  const authorizing = form.auth === "bearer" || form.auth === "oauth2";
  Object.assign(problems, rowProblems(form.headers, "header", authorizing));
  if (form.auth === "bearer" && /[\r\n]/.test(form.bearer.value)) {
    problems.bearer = SECRET_LINE_BREAK_TEXT;
  }
  if (form.auth === "oauth2") {
    const port = portOf(form.callbackPortText);
    if (port !== null && !(Number.isInteger(port) && port >= 1024 && port <= 65535)) {
      problems.callbackPort = CONFIG_ERROR_TEXT.invalid_callback_port;
    }
  }
  return problems;
}

/** MSS-FR-UWZQ: one secret step after the record is saved. */
export interface McpSecretStep {
  kind: "set" | "clear";
  /** MCS-FR-QBFA: `env:<NAME>`, `header:<NAME>`, `bearer`, or `oauth_client_secret`. */
  slot: string;
  /** The field that shows a refusal of this step. */
  field: McpFieldKey;
  value: string;
}

/** The secret fields of the form, each with its slot and its field. */
export function secretFields(
  form: McpServerForm,
): { slot: string; field: McpFieldKey; input: McpSecretInput }[] {
  if (form.transport === "stdio") {
    return usedRows(form.env)
      .filter((row) => row.secret)
      .map((row) => ({ slot: `env:${row.name.trim()}`, field: `env:${row.key}`, input: row }));
  }
  const fields: { slot: string; field: McpFieldKey; input: McpSecretInput }[] = usedRows(form.headers)
    .filter((row) => row.secret)
    .map((row) => ({ slot: `header:${row.name.trim()}`, field: `header:${row.key}`, input: row }));
  if (form.auth === "bearer") fields.push({ slot: "bearer", field: "bearer", input: form.bearer });
  if (form.auth === "oauth2" && form.clientId.trim() !== "") {
    fields.push({ slot: "oauth_client_secret", field: "clientSecret", input: form.clientSecret });
  }
  return fields;
}

/**
 * MSS-FR-IUMU, MSS-FR-UWZQ: the secret steps of a save, the typed values first
 * and then the cleared ones. An empty field keeps the stored value, and a
 * clear of a slot with no value is no step.
 */
export function secretSteps(form: McpServerForm, presence: McpSecretPresence[]): McpSecretStep[] {
  const present = new Set(presence.filter((p) => p.present).map((p) => p.slot));
  const fields = secretFields(form);
  const sets = fields
    .filter((f) => f.input.value !== "")
    .map((f) => ({ kind: "set" as const, slot: f.slot, field: f.field, value: f.input.value }));
  const clears = fields
    .filter((f) => f.input.value === "" && f.input.clear && present.has(f.slot))
    .map((f) => ({ kind: "clear" as const, slot: f.slot, field: f.field, value: "" }));
  return [...sets, ...clears];
}

/**
 * MSS-FR-UWZQ: the field a refusal of `save_mcp_server` shows under, or null
 * for the top of the dialog. A code that names a list shows under the first
 * row of that list.
 */
export function refusalField(code: string, form: McpServerForm): McpFieldKey | null {
  const firstRow = (prefix: "env" | "header") => {
    const rows = usedRows(prefix === "env" ? form.env : form.headers);
    return rows.length > 0 ? `${prefix}:${rows[0].key}` : null;
  };
  switch (code) {
    case "invalid_id":
    case "duplicate_id":
      return "id";
    case "invalid_name":
      return "name";
    case "invalid_command":
      return "command";
    case "invalid_cwd":
      return "cwd";
    case "invalid_url":
      return "url";
    case "invalid_callback_port":
      return form.auth === "oauth2" ? "callbackPort" : null;
    case "invalid_auth":
      return "auth";
    case "invalid_env_name":
      return firstRow("env");
    case "invalid_header_name":
    case "invalid_header_value":
    case "authorization_header_conflict":
      return firstRow("header");
    case "duplicate_name":
      return firstRow(form.transport === "stdio" ? "env" : "header");
    default:
      return null;
  }
}
