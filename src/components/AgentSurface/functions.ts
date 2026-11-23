/**
 * The functions of the discussion catalog, evaluated by the panel itself
 * (`ASF-agent-surfaces.md` ASF-FR-GHUZ; `AUI-agent-ui-surfaces.md`
 * AUI-FR-PXGX).
 *
 * Each takes its resolved arguments. A function that the catalog does not
 * declare, or one that fails, gives `undefined`, and the panel draws an empty
 * value in its place.
 */
import { getAt, absolute } from "./pointer";

/** What a function reads besides its arguments. */
export interface Scope {
  model: Record<string, unknown>;
  /** The pointer of the template item, or `""` outside a template. */
  base: string;
  /** The index of the template item, or `undefined` outside a template. */
  index?: number;
}

type Args = Record<string, unknown>;

function text(value: unknown): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function finite(value: unknown): number | undefined {
  const n = typeof value === "number" ? value : typeof value === "string" ? Number(value) : NaN;
  return Number.isFinite(n) ? n : undefined;
}

/** `formatString`: `${/pointer}` and `${relative}` read from the model; `\${` is literal. */
function formatString(args: Args, scope: Scope): string {
  const template = text(args.value);
  return template.replace(/\\\$\{|\$\{([^}]*)\}/g, (match, pointer: string | undefined) => {
    if (match === "\\${") return "${";
    const path = (pointer ?? "").trim();
    if (path.includes("(")) return "";
    return text(getAt(scope.model, absolute(path, scope.base)));
  });
}

function formatNumber(args: Args): string | undefined {
  const value = finite(args.value);
  if (value === undefined) return undefined;
  const decimals = finite(args.decimals);
  const options: Intl.NumberFormatOptions = { useGrouping: args.grouping !== false };
  if (decimals !== undefined && decimals >= 0 && decimals <= 20) {
    options.minimumFractionDigits = decimals;
    options.maximumFractionDigits = decimals;
  }
  return new Intl.NumberFormat(undefined, options).format(value);
}

const MONTHS_LONG = Array.from({ length: 12 }, (_, m) =>
  new Intl.DateTimeFormat(undefined, { month: "long" }).format(new Date(2000, m, 1)),
);
const MONTHS_SHORT = Array.from({ length: 12 }, (_, m) =>
  new Intl.DateTimeFormat(undefined, { month: "short" }).format(new Date(2000, m, 1)),
);
const DAYS_LONG = Array.from({ length: 7 }, (_, d) =>
  new Intl.DateTimeFormat(undefined, { weekday: "long" }).format(new Date(2000, 0, 2 + d)),
);
const DAYS_SHORT = Array.from({ length: 7 }, (_, d) =>
  new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(new Date(2000, 0, 2 + d)),
);

/** `formatDate`: the common fields of a Unicode TR35 pattern. */
function formatDate(args: Args): string | undefined {
  const raw = args.value;
  const date =
    typeof raw === "number" ? new Date(raw) : typeof raw === "string" ? new Date(raw) : undefined;
  if (!date || Number.isNaN(date.getTime())) return undefined;
  const pad = (n: number, width = 2) => String(n).padStart(width, "0");
  const fields: Record<string, () => string> = {
    yyyy: () => String(date.getFullYear()),
    yy: () => pad(date.getFullYear() % 100),
    MMMM: () => MONTHS_LONG[date.getMonth()],
    MMM: () => MONTHS_SHORT[date.getMonth()],
    MM: () => pad(date.getMonth() + 1),
    M: () => String(date.getMonth() + 1),
    dd: () => pad(date.getDate()),
    d: () => String(date.getDate()),
    EEEE: () => DAYS_LONG[date.getDay()],
    EEE: () => DAYS_SHORT[date.getDay()],
    HH: () => pad(date.getHours()),
    H: () => String(date.getHours()),
    hh: () => pad(((date.getHours() + 11) % 12) + 1),
    h: () => String(((date.getHours() + 11) % 12) + 1),
    mm: () => pad(date.getMinutes()),
    ss: () => pad(date.getSeconds()),
    a: () => (date.getHours() < 12 ? "AM" : "PM"),
  };
  const pattern = text(args.format);
  return pattern.replace(/'([^']*)'|yyyy|yy|MMMM|MMM|MM|M|dd|d|EEEE|EEE|HH|H|hh|h|mm|ss|a/g, (m, quoted) =>
    quoted !== undefined ? quoted : (fields[m]?.() ?? m),
  );
}

function pluralize(args: Args): string | undefined {
  const value = finite(args.value);
  if (value === undefined) return undefined;
  if (value === 0 && typeof args.zero === "string") return args.zero;
  const category = new Intl.PluralRules().select(value);
  const chosen = args[category];
  return typeof chosen === "string" ? chosen : text(args.other);
}

/** `required`: a value that is not absent, not blank, and not an empty list. */
export function isPresent(value: unknown): boolean {
  if (value === undefined || value === null) return false;
  if (typeof value === "string") return value.trim() !== "";
  if (Array.isArray(value)) return value.length > 0;
  return true;
}

function within(n: number, args: Args): boolean {
  const min = finite(args.min);
  const max = finite(args.max);
  return (min === undefined || n >= min) && (max === undefined || n <= max);
}

const FUNCTIONS: Record<string, (args: Args, scope: Scope) => unknown> = {
  formatString,
  formatNumber,
  formatDate,
  pluralize,
  required: (args) => isPresent(args.value),
  length: (args) => within(text(args.value).length, args),
  numeric: (args) => {
    const n = finite(args.value);
    return n !== undefined && within(n, args);
  },
  and: (args) => Array.isArray(args.values) && args.values.every((v) => v === true),
  or: (args) => Array.isArray(args.values) && args.values.some((v) => v === true),
  not: (args) => args.value !== true,
  "@index": (args, scope) =>
    scope.index === undefined ? undefined : scope.index + (finite(args.offset) ?? 0),
};

/** AUI-FR-PXGX: the names of the functions the panel evaluates. */
export const FUNCTION_NAMES: readonly string[] = Object.keys(FUNCTIONS);

/** ASF-FR-GHUZ: call one function, or give `undefined` where it can not. */
export function callFunction(name: string, args: Args, scope: Scope): unknown {
  const fn = Object.prototype.hasOwnProperty.call(FUNCTIONS, name) ? FUNCTIONS[name] : undefined;
  if (!fn) return undefined;
  try {
    return fn(args, scope);
  } catch {
    return undefined;
  }
}

/** ASF-FR-ENRG / ASF-FR-GHUZ: a dynamic value: a literal, a binding, or a call. */
export function resolveValue(value: unknown, scope: Scope): unknown {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return Array.isArray(value) ? value.map((v) => resolveValue(v, scope)) : value;
  }
  const object = value as Record<string, unknown>;
  if (typeof object["@path"] === "string") {
    return getAt(scope.model, absolute(object["@path"], scope.base));
  }
  if (typeof object["@call"] === "string") {
    const args = (object.args ?? {}) as Record<string, unknown>;
    const resolved: Args = {};
    for (const [key, arg] of Object.entries(args)) resolved[key] = resolveValue(arg, scope);
    return callFunction(object["@call"], resolved, scope);
  }
  return value;
}

/** The text of a resolved value, as a panel shows it. */
export function asText(value: unknown): string {
  return text(value);
}
