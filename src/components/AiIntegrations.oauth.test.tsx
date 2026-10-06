import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  AgenticAiIntegrations,
  canVerifyAgentic,
  oauthTokenValidation,
  rendersOauthTokenField,
  type AgenticDraft,
} from "./AiIntegrations";
import { type AgenticVendorId, AI_ERRORS, isValidClaudeOauthToken } from "../types";
import {
  AGENTIC_ALL,
  SAMPLE_TOKEN,
  agenticIntegration,
  agenticLevel,
  argsOfFor,
  backendFor,
  cliVerified,
} from "../test/aiIntegrationsFixtures";

// The section reaches the backend only through `invoke`; mock the bridge so
// jsdom never needs the Tauri runtime — and so no test can spawn a real CLI or
// make a real request.
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// Claude Code's OAuth token (AII-FR-49..53)
// ---------------------------------------------------------------------------

describe("the Claude Code OAuth token field", () => {
  const tokenField = () => agenticLevel().getByTestId("agentic-oauth-token");
  const tokenNote = () => agenticLevel().getByTestId("agentic-token-note");
  const verifyButton = () =>
    agenticLevel().getByRole("button", { name: /^Verify/ });

  /**
   * Everything the level *renders as text*. Deliberately `textContent` rather
   * than `innerHTML`: the token input's own value is the one place a typed
   * token legitimately lives, and serialising the markup would flag it there.
   * What must never happen is the value escaping into a label, a status line, a
   * validation message, or a second field — all of which this catches.
   */
  const levelText = () => screen.getByTestId("agentic-level").textContent ?? "";

  /** Every input in the level except the token field, by value. */
  const otherInputValues = () =>
    Array.from(
      screen.getByTestId("agentic-level").querySelectorAll("input"),
    )
      .filter((el) => el.dataset.testid !== "agentic-oauth-token")
      .map((el) => el.value);

  it("is rendered by the Claude Code tab and by no other (AII-FR-16, AII-FR-49)", async () => {
    backend();
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");

    expect(tokenField()).toBeInTheDocument();
    expect(agenticLevel().getByLabelText("Binary")).toBeInTheDocument();

    // AII-FR-16 / AII-FR-49: and it sits between the binary field and the
    // verification status line, which is a positional claim the spec makes
    // twice. Presence alone would pass with the row anywhere in the panel.
    const binary = agenticLevel().getByLabelText("Binary");
    const status = agenticLevel().getByTestId("agentic-status");
    const after = Node.DOCUMENT_POSITION_FOLLOWING;
    expect(binary.compareDocumentPosition(tokenField()) & after).toBeTruthy();
    expect(tokenField().compareDocumentPosition(status) & after).toBeTruthy();

    for (const [tab, label] of [
      [/Codex/, "Binary"],
      [/OpenCode/, "Binary"],
      [/Claude Agent API/, "Base URL"],
      [/Custom agent API/, "Base URL"],
    ] as const) {
      await user.click(agenticLevel().getByRole("tab", { name: tab }));
      await waitFor(() =>
        expect(agenticLevel().getByLabelText(label)).toBeInTheDocument(),
      );
      expect(
        agenticLevel().queryByTestId("agentic-oauth-token"),
      ).not.toBeInTheDocument();
      expect(
        agenticLevel().queryByLabelText("OAuth token"),
      ).not.toBeInTheDocument();
    }
  });

  it("keeps Verify unavailable and complains inline for a value that is not a token (AII-FR-50, AII-FR-51)", async () => {
    backend();
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");

    await user.type(agenticLevel().getByLabelText("Binary"), "/usr/bin/claude");
    expect(verifyButton()).toBeDisabled();

    await user.type(tokenField(), "oat01-abc");
    expect(tokenNote()).toHaveTextContent(/starts with sk-ant-oat01-/);
    expect(verifyButton()).toBeDisabled();
    // AII-FR-50: the complaint names the shape, never the value.
    expect(tokenNote().textContent).not.toContain("oat01-abc");
    expect(levelText()).not.toContain("oat01-abc");
    expect(otherInputValues()).not.toContain("oat01-abc");

    // No backend operation was invoked by any of it.
    expect(
      invokeMock.mock.calls.some(
        ([cmd]) => cmd === "verify_agentic_integration",
      ),
    ).toBe(false);

    // Clearing the field withdraws the complaint and leaves Verify unavailable:
    // an empty field with nothing stored is not an error, just not enough.
    await user.clear(tokenField());
    expect(tokenNote()).not.toHaveTextContent(/starts with sk-ant-oat01-/);
    expect(verifyButton()).toBeDisabled();
  });

  it("submits the path and the new token together, then shows only the redacted state (AII-FR-20, AII-FR-50, AII-FR-51, AII-FR-52)", async () => {
    backend(
      {},
      {
        verify_agentic_integration: (args) =>
          cliVerified(
            args?.vendor as AgenticVendorId,
            (args?.config as { path: string }).path,
          ),
      },
    );
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");

    await user.type(agenticLevel().getByLabelText("Binary"), "/usr/bin/claude");
    await user.type(tokenField(), SAMPLE_TOKEN);
    expect(tokenNote()).not.toHaveTextContent(/starts with sk-ant-oat01-/);
    expect(verifyButton()).toBeEnabled();

    await user.click(verifyButton());
    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /verified · 2\.1\.4/,
      ),
    );

    expect(argsOf("verify_agentic_integration")).toEqual({
      vendor: "claude_code",
      config: { path: "/usr/bin/claude", oauthToken: SAMPLE_TOKEN },
    });
    // AII-FR-52 / FR-53: the field empties, the masked hint stands in for the
    // token, and the token itself is nowhere in the rendered section.
    expect(tokenField()).toHaveValue("");
    expect(tokenField()).toHaveAttribute("placeholder", "•••• ygAA");
    expect(levelText()).not.toContain(SAMPLE_TOKEN);
    expect(levelText()).not.toContain("sk-ant-oat01-");
    expect(otherInputValues()).not.toContain(SAMPLE_TOKEN);
  });

  it("re-verifies with the stored token without ever reading it (AII-FR-51, AII-FR-52)", async () => {
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/usr/bin/claude"),
          ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
        ],
      },
      {
        verify_agentic_integration: () =>
          cliVerified("claude_code", "/usr/bin/claude"),
      },
    );
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");

    // The tab opens with an empty field, a masked hint, and Verify available.
    expect(tokenField()).toHaveValue("");
    expect(tokenField()).toHaveAttribute("placeholder", "•••• ygAA");
    expect(tokenNote()).toHaveTextContent(/Leave empty to re-verify/);
    expect(verifyButton()).toBeEnabled();

    await user.click(verifyButton());
    await waitFor(() =>
      expect(argsOf("verify_agentic_integration")).toEqual({
        vendor: "claude_code",
        config: { path: "/usr/bin/claude" },
      }),
    );
    // The payload carries no token key at all, not a null one.
    const config = (argsOf("verify_agentic_integration") as {
      config: Record<string, unknown>;
    }).config;
    expect("oauthToken" in config).toBe(false);
    expect(tokenField()).toHaveAttribute("placeholder", "•••• ygAA");
  });

  it("keeps the old token's hint when a new one fails to verify (AII-FR-52, AII-FR-21)", async () => {
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/usr/bin/claude"),
          ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
        ],
      },
      {
        verify_agentic_integration: () => {
          throw AI_ERRORS.notFound;
        },
      },
    );
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");

    await user.type(tokenField(), "sk-ant-oat01-a-different-token-1234");
    await user.click(verifyButton());

    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /Nothing exists at that path/,
      ),
    );
    // The record is untouched, so the tab still reports the token it had.
    expect(tokenField()).toHaveAttribute("placeholder", "•••• ygAA");
    expect(levelText()).not.toContain("a-different-token-1234");

    // AII-FR-52, AII-FR-21, second half: when the retry succeeds, only the new token's
    // hint remains. The failed attempt left the typed token in the field, so
    // the retry is one more click — and this is what proves the hint follows
    // the record rather than being latched on first render.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "verify_agentic_integration")
        return cliVerified("claude_code", "/usr/bin/claude", {
          maskedHint: "1234",
        });
      if (cmd === "list_agentic_integrations") return [];
      throw new Error(`unexpected ${cmd}`);
    });
    await user.click(verifyButton());
    await waitFor(() =>
      expect(tokenField()).toHaveAttribute("placeholder", "•••• 1234"),
    );
    expect(levelText()).not.toContain("ygAA");
    expect(levelText()).not.toContain("a-different-token-1234");
  });

  it("offers Clear for a stored credential the registry has no record of", () => {
    // A keychain entry the registry does not know about — an older
    // `synthesis.toml` restored over a newer one, say. Gating Clear on `state`
    // alone would hide the only control that can purge it, leaving the author
    // no route short of the OS keychain. The note and the placeholder key on
    // the same condition, so the tab cannot contradict itself either.
    backend({
      agentic: [
        agenticIntegration("claude_code", {
          state: "unconfigured",
          keyState: "set",
          maskedHint: null,
        }),
        ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
      ],
    });
    render(<AgenticAiIntegrations />);
    return waitFor(() => {
      expect(
        agenticLevel().getByRole("button", { name: "Clear" }),
      ).toBeInTheDocument();
      expect(tokenField()).toHaveAttribute("placeholder", "a token is stored");
      expect(tokenNote()).toHaveTextContent(/Leave empty to re-verify/);
    });
  });

  it("holds a typed token no longer than the submission that carries it (AII-FR-49, AII-FR-53, AII-FR-30)", async () => {
    backend();
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");

    await user.type(tokenField(), SAMPLE_TOKEN);
    expect(tokenField()).toHaveValue(SAMPLE_TOKEN);

    // Away to another tab and back: the candidate is discarded (AII-FR-21),
    // and the token with it.
    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    expect(
      agenticLevel().queryByTestId("agentic-oauth-token"),
    ).not.toBeInTheDocument();
    expect(levelText()).not.toContain(SAMPLE_TOKEN);

    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    await waitFor(() => expect(tokenField()).toHaveValue(""));
    expect(levelText()).not.toContain(SAMPLE_TOKEN);
    expect(verifyButton()).toBeDisabled();

    // AII-FR-53: and it is in no persisted UI state either. Today the section
    // unmounts when another one is selected, which is what saves us — so this
    // asserts the outcome rather than the mechanism, and would catch a future
    // switch to keep-alive rendering.
    for (const store of [window.localStorage, window.sessionStorage]) {
      const dump = Array.from({ length: store.length }, (_, i) => {
        const key = store.key(i);
        return key === null ? "" : `${key}=${store.getItem(key)}`;
      }).join("\n");
      expect(dump).not.toContain("sk-ant-oat01-");
    }
  });

  it("renders no validation message on a fresh empty field (AII-FR-29, AII-FR-50)", async () => {
    // An empty field with nothing configured is not an error state — it is the
    // level's ordinary empty presentation, which must render clean.
    backend();
    render(<AgenticAiIntegrations />);
    await screen.findByTestId("agentic-level");

    expect(tokenNote()).not.toHaveTextContent(/starts with sk-ant-oat01-/);
    expect(tokenNote()).toHaveTextContent(/Required —/);
    expect(tokenField()).not.toHaveAttribute("aria-invalid");
    expect(
      agenticLevel().getByTestId("agentic-empty"),
    ).toHaveTextContent(/give it an OAuth token/);
  });

  it("returns to an unconfigured tab when cleared (AII-FR-26, AII-FR-51)", async () => {
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/usr/bin/claude"),
          ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
        ],
      },
      {
        clear_agentic_integration: () =>
          AGENTIC_ALL.map((v) => agenticIntegration(v)),
      },
    );
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await screen.findByTestId("agentic-level");
    expect(tokenField()).toHaveAttribute("placeholder", "•••• ygAA");

    await user.click(agenticLevel().getByRole("button", { name: "Clear" }));

    await waitFor(() =>
      expect(tokenField()).toHaveAttribute("placeholder", "required"),
    );
    expect(tokenField()).toHaveValue("");
    expect(agenticLevel().getByLabelText("Binary")).toHaveValue("");
    expect(verifyButton()).toBeDisabled();
    // AII-FR-26: no token-derived value survives the clear.
    expect(levelText()).not.toContain("ygAA");
  });

  it("reports an unreadable stored token without offering activation (AII-FR-27)", async () => {
    backend({
      agentic: [
        cliVerified("claude_code", "/usr/bin/claude", {
          state: "key_unavailable",
          keyState: "unavailable",
        }),
        ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
      ],
    });
    render(<AgenticAiIntegrations />);
    await screen.findByTestId("agentic-level");

    expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
      /OAuth token can no longer be read/,
    );
    // The configuration is kept, not cleared.
    expect(agenticLevel().getByLabelText("Binary")).toHaveValue(
      "/usr/bin/claude",
    );
    expect(
      agenticLevel().getByRole("button", { name: "Use this integration" }),
    ).toBeDisabled();
  });
});

describe("the OAuth token rules", () => {
  const draft = (over: Partial<AgenticDraft> = {}): AgenticDraft => ({
    path: "",
    baseUrl: "",
    apiKey: "",
    oauthToken: "",
    ...over,
  });

  it("AII-FR-50, AIC-FR-27: matches the anchored pattern and nothing else", () => {
    // The same table the Rust side checks against `is_valid_oauth_token`,
    // because two implementations of one pattern are exactly where a drift
    // would hide. This is the raw predicate: unanchored candidates fail here,
    // and the trimming that precedes it is a separate layer, asserted below.
    for (const bad of [
      "",
      "oat01-abc",
      "sk-ant-oat01-",
      "sk-ant-oat01-abc def",
      " sk-ant-oat01-abc",
      "sk-ant-oat01-abc\n",
      "xsk-ant-oat01-abc",
      "sk-ant-oat02-abc",
      "sk-ant-oat01-abc!",
      // `[A-Za-z0-9]` is ASCII on both sides. The likeliest drift is the Rust
      // predicate being "simplified" to `char::is_alphanumeric`, which would
      // accept these while this regex still refuses them.
      "sk-ant-oat01-abc\u00e9",
      "sk-ant-oat01-\u0661\u0662\u0663",
      "sk-ant-oat01-\u03a9",
    ]) {
      expect(isValidClaudeOauthToken(bad), bad).toBe(false);
    }
    for (const good of [
      SAMPLE_TOKEN,
      "sk-ant-oat01-a",
      "sk-ant-oat01-A1-b2-C3",
      "sk-ant-oat01----",
      "sk-ant-oat01-abc_def",
      "sk-ant-oat01-_",
    ]) {
      expect(isValidClaudeOauthToken(good), good).toBe(true);
    }
  });

  it("AII-FR-50: accepts only sk-ant-oat01- followed by letters, digits, hyphens, or underscores", () => {
    const rejections = [
      "oat01-abc",
      "sk-ant-oat01-",
      "sk-ant-oat01-abc def",
      "xsk-ant-oat01-abc",
      "sk-ant-oat02-abc",
      "sk-ant-oat01-abc!",
    ].map((bad) => {
      const message = oauthTokenValidation(bad);
      expect(message, bad).not.toBe("");
      return message;
    });
    // AII-FR-50: one constant message for every rejection. Asserting that the
    // text does not *vary* with the input is stronger than asserting it does
    // not contain any particular value — a message that cannot differ cannot
    // quote what it rejected, whatever that was.
    expect(new Set(rejections).size).toBe(1);
    for (const good of [
      SAMPLE_TOKEN,
      "sk-ant-oat01-a",
      "sk-ant-oat01-A1-b2-C3",
      "sk-ant-oat01----",
      "sk-ant-oat01-abc_def",
    ]) {
      expect(oauthTokenValidation(good)).toBe("");
    }
    // An empty field is not a complaint — it means "keep the stored token".
    expect(oauthTokenValidation("")).toBe("");
    expect(oauthTokenValidation("   ")).toBe("");
    // A token pasted with stray whitespace is trimmed rather than refused, and
    // the backend trims identically before applying the same pattern — so what
    // the field accepts and what the keychain will take never disagree.
    expect(oauthTokenValidation(`  ${SAMPLE_TOKEN}\n`)).toBe("");
  });

  it("gates Verify on a usable configuration (AII-FR-51)", () => {
    const unconfigured = agenticIntegration("claude_code");
    const stored = cliVerified("claude_code", "/usr/bin/claude");

    // Nothing stored: only a valid token opens the action.
    expect(canVerifyAgentic(unconfigured, draft())).toBe(false);
    expect(canVerifyAgentic(unconfigured, draft({ oauthToken: "nope" }))).toBe(
      false,
    );
    expect(
      canVerifyAgentic(unconfigured, draft({ oauthToken: SAMPLE_TOKEN })),
    ).toBe(true);

    // Something stored: an empty field re-verifies with it, but a value that is
    // not a token still blocks — a bad token is not a request to keep the old.
    expect(canVerifyAgentic(stored, draft())).toBe(true);
    expect(canVerifyAgentic(stored, draft({ oauthToken: "nope" }))).toBe(false);
    expect(canVerifyAgentic(stored, draft({ oauthToken: SAMPLE_TOKEN }))).toBe(
      true,
    );

    // An unreadable token is not a stored one: the author must supply another.
    const tokenGone = cliVerified("claude_code", "/usr/bin/claude", {
      state: "key_unavailable",
      keyState: "unavailable",
    });
    expect(canVerifyAgentic(tokenGone, draft())).toBe(false);
  });

  it("never gates a vendor that holds no credential (AII-FR-49)", () => {
    for (const vendor of ["codex", "opencode"] as AgenticVendorId[]) {
      const i = agenticIntegration(vendor);
      expect(rendersOauthTokenField(i)).toBe(false);
      expect(canVerifyAgentic(i, draft())).toBe(true);
    }
    // An API-kind vendor requiring a key is still not a token vendor.
    const api = agenticIntegration("claude_agent_api");
    expect(api.keyRequired).toBe(true);
    expect(rendersOauthTokenField(api)).toBe(false);
    expect(rendersOauthTokenField(agenticIntegration("claude_code"))).toBe(true);
  });
});
