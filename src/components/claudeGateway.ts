// The fields of Claude Code's Custom Gateway sub-tab and the Environment
// variables field, and the local checks on them
// (`specifications/ui/AII-ai-integrations.md` AII-FR-PHFX, AII-FR-EJMG,
// AII-FR-HOKG). Every check is structural and local: it invokes no operation
// and reaches no service, and no message quotes what the author typed.

import type { AgenticIntegration, GatewayApi } from "../types";

/** AII-FR-PHFX: the token variable name a fresh Custom Gateway sub-tab shows. */
export const DEFAULT_GATEWAY_TOKEN_VAR = "ANTHROPIC_AUTH_TOKEN";

const VARIABLE_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
/** A token travels in an HTTP header, so it is a run of visible ASCII. */
const VISIBLE_ASCII = /^[\x21-\x7e]+$/;

/** The fields both sub-tabs of the Claude Code tab keep beyond the OAuth token. */
export interface GatewayFields {
  /** AII-FR-PHFX: the API shape chosen in the Custom Gateway sub-tab. */
  gatewayApi: GatewayApi;
  gatewayUrl: string;
  gatewayTokenVar: string;
  /** Lives in the field it was typed into, and only until a submission resolves (AII-FR-53). */
  gatewayToken: string;
  /** One `NAME=value` entry on each line (AII-FR-EJMG). */
  envText: string;
}

/** AII-FR-HOKG: does `value` have the shape of a variable name? */
export function isValidVariableName(value: string): boolean {
  return VARIABLE_NAME.test(value);
}

/** AII-FR-HOKG: the inline complaint about the token variable name, or `""`. */
export function gatewayTokenVarValidation(value: string): string {
  return isValidVariableName(value.trim())
    ? ""
    : "A variable name uses letters, digits, and underscores, and does not start with a digit.";
}

/** AII-FR-HOKG: the inline complaint about a typed gateway token, or `""`. */
export function gatewayTokenValidation(value: string): string {
  const typed = value.trim();
  if (typed === "") return "";
  return VISIBLE_ASCII.test(typed)
    ? ""
    : "A token has no spaces or control characters.";
}

/** AII-FR-HOKG: the inline complaint about the base URL, or `""`. */
export function gatewayUrlValidation(value: string): string {
  return value.trim() === "" ? "Enter the gateway's base URL." : "";
}

export interface EnvEntries {
  /** The valid entries, in the order written. Blank lines are dropped. */
  entries: string[];
  /** The complaint about the first bad line, naming its number; `""` when all are valid. */
  error: string;
}

/**
 * AII-FR-EJMG / AII-FR-HOKG: read the Environment variables field.
 *
 * A line is trimmed. A blank line is ignored. Any other line must hold a valid
 * name, an `=`, and then a value that may be empty. A complaint names the line
 * number and never the line.
 */
export function parseEnvText(text: string): EnvEntries {
  const entries: string[] = [];
  const lines = text.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index].trim();
    if (line === "") continue;
    const equals = line.indexOf("=");
    if (equals < 1 || !isValidVariableName(line.slice(0, equals))) {
      return {
        entries,
        error: `Line ${index + 1} is not written as NAME=value.`,
      };
    }
    entries.push(line);
  }
  return { entries, error: "" };
}

/** The field's text for a record's stored entries. */
export function envTextOf(entries: string[] | undefined): string {
  return (entries ?? []).join("\n");
}

/** AII-FR-EJMG: do the typed entries differ from the stored ones? */
export function envEdited(i: AgenticIntegration, envText: string): boolean {
  const typed = parseEnvText(envText).entries;
  const stored = i.envVars ?? [];
  return typed.length !== stored.length || typed.some((e, k) => e !== stored[k]);
}

/**
 * AII-FR-HOKG: may Verify be activated in the Custom Gateway sub-tab?
 *
 * The base URL is not empty, the variable name is valid, every environment line
 * is valid, and a token is typed or one is stored.
 */
export function canVerifyGateway(
  i: AgenticIntegration,
  fields: GatewayFields,
): boolean {
  if (gatewayUrlValidation(fields.gatewayUrl) !== "") return false;
  if (gatewayTokenVarValidation(fields.gatewayTokenVar) !== "") return false;
  if (parseEnvText(fields.envText).error !== "") return false;
  const typed = fields.gatewayToken.trim();
  if (typed !== "") return gatewayTokenValidation(typed) === "";
  return i.gatewayKeyState === "set";
}

/**
 * AII-FR-DKDC: how many models the gateway listed. A gateway that listed none
 * leaves the record on the bundled list, which counts as zero.
 */
export function gatewayModelCount(i: AgenticIntegration): number {
  return i.modelsOrigin === "probed" ? i.models.length : 0;
}
