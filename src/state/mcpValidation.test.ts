import { describe, expect, it } from "vitest";

import type { McpServer } from "../types";
import { CONFIG_ERROR_TEXT, USE_HTTPS_TEXT } from "./mcpServerText";
import {
  buildRecord,
  cwdStaysInside,
  emptyForm,
  formFromRecord,
  newRow,
  refusalField,
  secretSteps,
  slugify,
  urlProblem,
  validateForm,
  type McpServerForm,
} from "./mcpValidation";

const NEW = { otherIds: [], isNew: true };

function stdioForm(over: Partial<McpServerForm> = {}): McpServerForm {
  return { ...emptyForm(), name: "Docs", id: "docs", command: "npx", ...over };
}

function httpForm(over: Partial<McpServerForm> = {}): McpServerForm {
  return {
    ...emptyForm(),
    name: "Tracker",
    id: "tracker",
    transport: "http",
    url: "https://mcp.example.com/mcp",
    ...over,
  };
}

describe("MSS-FR-DVDX: the ID slug", () => {
  it("MSS-FR-DVDX: lowercases, joins words with hyphens, and cuts to 32 characters", () => {
    expect(slugify("Issue tracker")).toBe("issue-tracker");
    expect(slugify("  --Docs  Index!! ")).toBe("docs-index");
    expect(slugify("Ünïcode Café")).toBe("unicode-cafe");
    expect(slugify("a".repeat(40))).toHaveLength(32);
    // A cut that ends on a hyphen drops it.
    expect(slugify(`${"a".repeat(31)} b`)).toBe("a".repeat(31));
    expect(slugify("!!!")).toBe("");
  });
});

describe("MSS-FR-BEQD: the field checks of the core", () => {
  it("MSS-FR-BEQD: a valid local record has no problem", () => {
    expect(validateForm(stdioForm(), NEW)).toEqual({});
  });

  it("MSS-FR-BEQD / MCS-FR-LLQD: the name is 1 to 80 characters after trimming", () => {
    expect(validateForm(stdioForm({ name: "   " }), NEW).name).toBe(CONFIG_ERROR_TEXT.invalid_name);
    expect(validateForm(stdioForm({ name: "x".repeat(81) }), NEW).name).toBe(
      CONFIG_ERROR_TEXT.invalid_name,
    );
    expect(validateForm(stdioForm({ name: ` ${"x".repeat(80)} ` }), NEW).name).toBeUndefined();
  });

  it("MSS-FR-BEQD / MCS-FR-LLQD: the ID matches the pattern and is unique on add only", () => {
    for (const bad of ["", "-docs", "Docs", "docs_1", "a".repeat(33)]) {
      expect(validateForm(stdioForm({ id: bad }), NEW).id).toBe(CONFIG_ERROR_TEXT.invalid_id);
    }
    expect(validateForm(stdioForm({ id: "a".repeat(32) }), NEW).id).toBeUndefined();
    expect(validateForm(stdioForm({ id: "0-docs" }), NEW).id).toBeUndefined();
    expect(validateForm(stdioForm(), { otherIds: ["docs"], isNew: true }).id).toBe(
      CONFIG_ERROR_TEXT.duplicate_id,
    );
    // MSS-FR-DVDX: on edit, the ID is fixed and is not checked.
    expect(validateForm(stdioForm({ id: "Bad" }), { otherIds: [], isNew: false }).id).toBeUndefined();
  });

  it("MSS-FR-BEQD / MCS-FR-KALQ: a local command is not empty", () => {
    expect(validateForm(stdioForm({ command: "  " }), NEW).command).toBe(
      CONFIG_ERROR_TEXT.invalid_command,
    );
  });

  it("MSS-FR-BEQD / MCS-FR-KLQS: the working folder is relative and stays inside the project", () => {
    expect(cwdStaysInside("tools/docs")).toBe(true);
    expect(cwdStaysInside("./a/../b")).toBe(true);
    expect(cwdStaysInside("a/../..")).toBe(false);
    expect(cwdStaysInside("../other")).toBe(false);
    expect(cwdStaysInside("/abs")).toBe(false);
    expect(cwdStaysInside("C:\\x")).toBe(false);
    expect(cwdStaysInside("a\\..\\..\\x")).toBe(false);
    expect(validateForm(stdioForm({ cwd: "../x" }), NEW).cwd).toBe(CONFIG_ERROR_TEXT.invalid_cwd);
    expect(validateForm(stdioForm({ cwd: "" }), NEW).cwd).toBeUndefined();
  });

  it("MSS-FR-BEQD / MCS-FR-KALQ: a variable name has the shell shape, and is in the list one time", () => {
    const good = newRow({ name: "API_KEY", value: "x" });
    const bad = newRow({ name: "1KEY", value: "x" });
    const twice = newRow({ name: "API_KEY", secret: true });
    const problems = validateForm(stdioForm({ env: [good, bad, twice] }), NEW);
    expect(problems[`env:${good.key}`]).toBeUndefined();
    expect(problems[`env:${bad.key}`]).toBe(CONFIG_ERROR_TEXT.invalid_env_name);
    expect(problems[`env:${twice.key}`]).toBe(CONFIG_ERROR_TEXT.duplicate_name);
    // Names of variables are case-sensitive.
    const lower = newRow({ name: "api_key", value: "y" });
    expect(validateForm(stdioForm({ env: [good, lower] }), NEW)).toEqual({});
    // A row with no name and no value says nothing.
    expect(validateForm(stdioForm({ env: [newRow()] }), NEW)).toEqual({});
  });

  it("MSS-FR-BEQD / MCS-FR-XDGN, MSS-FR-GHDO: https anywhere, http only to this computer", () => {
    expect(urlProblem("https://mcp.example.com/mcp")).toBeNull();
    expect(urlProblem("http://localhost:8080/mcp")).toBeNull();
    expect(urlProblem("http://127.0.0.1/mcp")).toBeNull();
    expect(urlProblem("http://[::1]:3000/mcp")).toBeNull();
    expect(urlProblem("http://example.com/mcp")).toBe(USE_HTTPS_TEXT);
    expect(urlProblem("http://127.0.0.2/mcp")).toBe(USE_HTTPS_TEXT);
    expect(urlProblem("ftp://example.com")).toBe(CONFIG_ERROR_TEXT.invalid_url);
    expect(urlProblem("not a url")).toBe(CONFIG_ERROR_TEXT.invalid_url);
    expect(urlProblem("")).toBe(CONFIG_ERROR_TEXT.invalid_url);
    expect(validateForm(httpForm({ url: "http://example.com" }), NEW).url).toBe(
      "Use https for a server on another computer.",
    );
  });

  it("MSS-FR-BEQD / MCS-FR-DINZ: a header name is an HTTP token, and is in the list one time in any letter case", () => {
    const good = newRow({ name: "X-Team", value: "a" });
    const bad = newRow({ name: "X Team", value: "a" });
    const twice = newRow({ name: "x-team", secret: true });
    const problems = validateForm(httpForm({ headers: [good, bad, twice] }), NEW);
    expect(problems[`header:${good.key}`]).toBeUndefined();
    expect(problems[`header:${bad.key}`]).toBe(CONFIG_ERROR_TEXT.invalid_header_name);
    expect(problems[`header:${twice.key}`]).toBe(CONFIG_ERROR_TEXT.duplicate_name);
  });

  it("MSS-FR-BEQD / MCS-FR-DINZ, MSS-FR-OKDG: a plain header value cannot hold a line break", () => {
    const plain = newRow({ name: "X-Team", value: "a\nb" });
    const problems = validateForm(httpForm({ headers: [plain] }), NEW);
    expect(problems[`header:${plain.key}`]).toBe(CONFIG_ERROR_TEXT.invalid_header_value);
  });

  it("MSS-FR-BEQD / MCS-FR-FJLR: an Authorization header in any case conflicts with bearer and oauth2 only", () => {
    const header = newRow({ name: "authorization", value: "x" });
    for (const auth of ["bearer", "oauth2"] as const) {
      expect(validateForm(httpForm({ auth, headers: [header] }), NEW)[`header:${header.key}`]).toBe(
        CONFIG_ERROR_TEXT.authorization_header_conflict,
      );
    }
    expect(validateForm(httpForm({ auth: "none", headers: [header] }), NEW)).toEqual({});
  });

  it("MSS-FR-BEQD / MCS-FR-UAAN: the callback port is empty or from 1024 to 65535", () => {
    const port = (text: string) =>
      validateForm(httpForm({ auth: "oauth2", callbackPortText: text }), NEW).callbackPort;
    expect(port("")).toBeUndefined();
    expect(port("1024")).toBeUndefined();
    expect(port("65535")).toBeUndefined();
    for (const bad of ["1023", "65536", "80.5", "abc", "-1"]) {
      expect(port(bad)).toBe(CONFIG_ERROR_TEXT.invalid_callback_port);
    }
    // Not OAuth 2: the port is not part of the record, so it is not checked.
    expect(validateForm(httpForm({ auth: "none", callbackPortText: "1" }), NEW)).toEqual({});
  });

  it("MSS-FR-BEQD / MCS-FR-IXHD: a bearer token or a secret header with a line break is refused", () => {
    expect(validateForm(httpForm({ auth: "bearer", bearer: { value: "a\nb", clear: false } }), NEW).bearer).toBe(
      "The value cannot hold a line break.",
    );
    const header = newRow({ name: "X-Key", value: "a\r\nb", secret: true });
    expect(validateForm(httpForm({ headers: [header] }), NEW)[`header:${header.key}`]).toBe(
      "The value cannot hold a line break.",
    );
  });
});

describe("MSS-FR-BHNZ: the record the dialog saves", () => {
  it("MSS-FR-XMNV / MSS-FR-BHNZ: a local record splits the arguments by line and the rows by Secret", () => {
    const form = stdioForm({
      name: "  Docs  ",
      argsText: "-y\n\ndocs-mcp-server\r\n--flag value",
      cwd: " tools ",
      env: [
        newRow({ name: "MODE", value: "fast" }),
        newRow({ name: "DOCS_API_KEY", value: "s3cret", secret: true }),
        newRow(),
      ],
    });
    const record = buildRecord(form, null);
    expect(record).toEqual({
      id: "docs",
      name: "Docs",
      enabled: true,
      disabledTools: [],
      transport: {
        kind: "stdio",
        command: "npx",
        args: ["-y", "docs-mcp-server", "--flag value"],
        cwd: "tools",
        env: { MODE: "fast" },
        secretEnv: ["DOCS_API_KEY"],
      },
    });
    // No secret value is in the record.
    expect(JSON.stringify(record)).not.toContain("s3cret");
  });

  it("MSS-FR-HIWH: a remote record keeps the OAuth fields for oauth2 alone", () => {
    const oauth = buildRecord(
      httpForm({ auth: "oauth2", clientId: " app ", scopesText: " read  write ", callbackPortText: "8765" }),
      null,
    );
    expect(oauth.transport).toEqual({
      kind: "http",
      url: "https://mcp.example.com/mcp",
      headers: {},
      secretHeaders: [],
      auth: "oauth2",
      clientId: "app",
      scopes: ["read", "write"],
      callbackPort: 8765,
    });
    const bearer = buildRecord(
      httpForm({ auth: "bearer", clientId: "app", scopesText: "read", callbackPortText: "8765" }),
      null,
    );
    expect(bearer.transport).toMatchObject({ auth: "bearer", clientId: null, scopes: [], callbackPort: null });
  });

  it("MSS-FR-MWAV: an edit keeps enabled and disabledTools, and the form round-trips the record", () => {
    const record: McpServer = {
      id: "tracker",
      name: "Tracker",
      enabled: false,
      disabledTools: ["delete_issue"],
      transport: {
        kind: "http",
        url: "https://mcp.example.com/mcp",
        headers: { "X-Team": "platform" },
        secretHeaders: ["X-Api-Key"],
        auth: "oauth2",
        clientId: null,
        scopes: ["issues:read"],
        callbackPort: null,
      },
    };
    const form = formFromRecord(record);
    expect(form.idEdited).toBe(true);
    expect(form.headers.map((r) => [r.name, r.value, r.secret])).toEqual([
      ["X-Team", "platform", false],
      ["X-Api-Key", "", true],
    ]);
    expect(buildRecord(form, record)).toEqual(record);
  });
});

describe("MSS-FR-UWZQ, MSS-FR-IUMU: the secret steps of a save", () => {
  it("MSS-FR-IUMU: a typed value sets, an empty field keeps, a Clear clears a stored value", () => {
    const typed = newRow({ name: "TYPED", value: "v", secret: true });
    const kept = newRow({ name: "KEPT", secret: true });
    const cleared = newRow({ name: "CLEARED", secret: true, clear: true });
    const clearedMissing = newRow({ name: "NONE", secret: true, clear: true });
    const plain = newRow({ name: "PLAIN", value: "p" });
    const steps = secretSteps(stdioForm({ env: [cleared, typed, kept, clearedMissing, plain] }), [
      { slot: "env:TYPED", present: false },
      { slot: "env:KEPT", present: true },
      { slot: "env:CLEARED", present: true },
      { slot: "env:NONE", present: false },
    ]);
    // The typed values first, then the cleared ones.
    expect(steps).toEqual([
      { kind: "set", slot: "env:TYPED", field: `env:${typed.key}`, value: "v" },
      { kind: "clear", slot: "env:CLEARED", field: `env:${cleared.key}`, value: "" },
    ]);
  });

  it("MCS-FR-QBFA: the bearer slot for bearer, the client secret slot with a client ID only", () => {
    const bearer = secretSteps(httpForm({ auth: "bearer", bearer: { value: "tok", clear: false } }), []);
    expect(bearer.map((s) => s.slot)).toEqual(["bearer"]);
    // The bearer field of a form that is not bearer is not a slot of the record.
    expect(secretSteps(httpForm({ auth: "none", bearer: { value: "tok", clear: false } }), [])).toEqual([]);
    const noClient = httpForm({ auth: "oauth2", clientSecret: { value: "cs", clear: false } });
    expect(secretSteps(noClient, [])).toEqual([]);
    const withClient = httpForm({
      auth: "oauth2",
      clientId: "app",
      clientSecret: { value: "cs", clear: false },
      headers: [newRow({ name: "X-Key", value: "k", secret: true })],
    });
    expect(secretSteps(withClient, []).map((s) => [s.slot, s.field])).toEqual([
      ["header:X-Key", `header:${withClient.headers[0].key}`],
      ["oauth_client_secret", "clientSecret"],
    ]);
  });
});

describe("MSS-FR-UWZQ: the field a refusal shows under", () => {
  it("MSS-FR-UWZQ: a code that names a field goes under it; any other goes to the top", () => {
    const env = newRow({ name: "1X", value: "a" });
    const form = stdioForm({ env: [env] });
    expect(refusalField("duplicate_id", form)).toBe("id");
    expect(refusalField("invalid_name", form)).toBe("name");
    expect(refusalField("invalid_command", form)).toBe("command");
    expect(refusalField("invalid_cwd", form)).toBe("cwd");
    expect(refusalField("invalid_env_name", form)).toBe(`env:${env.key}`);
    expect(refusalField("duplicate_name", form)).toBe(`env:${env.key}`);
    expect(refusalField("invalid_url", httpForm())).toBe("url");
    expect(refusalField("invalid_callback_port", httpForm({ auth: "oauth2" }))).toBe("callbackPort");
    expect(refusalField("invalid_header_name", httpForm())).toBeNull();
    expect(refusalField("no_project_open", form)).toBeNull();
    expect(refusalField("malformed_project_config", form)).toBeNull();
  });
});
