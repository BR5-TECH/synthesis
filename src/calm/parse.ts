/**
 * CLM-FR-ATLN: the parse step of a read.
 */
import { CALM_ERROR, type CalmError, type JsonObject } from "./types";

export type ParseOutcome = { ok: true; document: JsonObject } | { ok: false; error: CalmError };

/** The 1-based line and column of a character offset in `text`. */
export function lineAndColumn(text: string, offset: number): { line: number; column: number } {
  let line = 1;
  let column = 1;
  const end = Math.min(Math.max(offset, 0), text.length);
  for (let i = 0; i < end; i++) {
    if (text.charCodeAt(i) === 10) {
      line++;
      column = 1;
    } else {
      column++;
    }
  }
  return { line, column };
}

/**
 * The offset of the first JSON syntax error of `text`, or null when it has
 * none. The engines give the place of an error in different forms, or not at
 * all, so this scanner finds it the same way on each of them.
 */
export function syntaxErrorOffset(text: string): number | null {
  let at = 0;
  const fail = (): never => {
    throw at;
  };
  const space = () => {
    while (at < text.length && " \t\n\r".includes(text[at])) at++;
  };
  const literal = (word: string) => {
    for (const char of word) {
      if (text[at] !== char) fail();
      at++;
    }
  };
  const string = () => {
    at++;
    while (at < text.length) {
      const char = text[at];
      if (char === '"') {
        at++;
        return;
      }
      if (char < " ") fail();
      if (char === "\\") {
        at++;
        const escape = text[at];
        if (escape === "u") {
          for (let i = 1; i <= 4; i++) if (!/[0-9a-fA-F]/.test(text[at + i] ?? "")) fail();
          at += 5;
          continue;
        }
        if (!'"\\/bfnrt'.includes(escape ?? "x")) fail();
      }
      at++;
    }
    fail();
  };
  const number = () => {
    const match = /^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/.exec(text.slice(at));
    if (!match) fail();
    at += match![0].length;
  };
  const value = (): void => {
    space();
    const char = text[at];
    if (char === "{") {
      at++;
      space();
      if (text[at] === "}") {
        at++;
        return;
      }
      for (;;) {
        space();
        if (text[at] !== '"') fail();
        string();
        space();
        if (text[at] !== ":") fail();
        at++;
        value();
        space();
        if (text[at] === ",") {
          at++;
          continue;
        }
        if (text[at] !== "}") fail();
        at++;
        return;
      }
    }
    if (char === "[") {
      at++;
      space();
      if (text[at] === "]") {
        at++;
        return;
      }
      for (;;) {
        value();
        space();
        if (text[at] === ",") {
          at++;
          continue;
        }
        if (text[at] !== "]") fail();
        at++;
        return;
      }
    }
    if (char === '"') return string();
    if (char === "t") return literal("true");
    if (char === "f") return literal("false");
    if (char === "n") return literal("null");
    if (char === "-" || (char !== undefined && char >= "0" && char <= "9")) return number();
    fail();
  };
  try {
    value();
    space();
    return at < text.length ? at : null;
  } catch (offset) {
    if (typeof offset === "number") return offset;
    throw offset;
  }
}

/** CLM-FR-ATLN: the source as one JSON object, or the parse error. */
export function parseCalm(source: string): ParseOutcome {
  let value: unknown;
  try {
    value = JSON.parse(source);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const offset = syntaxErrorOffset(source);
    const place = offset === null ? null : lineAndColumn(source, offset);
    const detail = place
      ? `${message.trim()}\nLine ${place.line}, column ${place.column}.`
      : message.trim();
    return {
      ok: false,
      error: {
        state: "error",
        step: "parse",
        summary: CALM_ERROR.parse,
        detail: detail || undefined,
        line: place?.line,
      },
    };
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    const kind = value === null ? "null" : Array.isArray(value) ? "an array" : typeof value;
    return {
      ok: false,
      error: {
        state: "error",
        step: "parse",
        summary: CALM_ERROR.parse,
        detail: `The top-level value is ${kind}, not an object.`,
        line: 1,
      },
    };
  }
  return { ok: true, document: value as JsonObject };
}
