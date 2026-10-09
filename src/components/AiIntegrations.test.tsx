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
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  agenticStatus,
  aiErrorMessage,
  apiStatus,
  type AgenticDraft,
} from "./AiIntegrations";
import { emptyDraft } from "./agenticDraft";
import { AI_ERRORS } from "../types";
import {
  AGENTIC_ALL,
  API_ALL,
  BothLevels,
  SAMPLE_TOKEN,
  agenticIntegration,
  agenticLevel,
  apiIntegration,
  apiLevel,
  apiVerified,
  backendFor,
  cliVerified,
  commandsFor,
  openApiTab,
  pick,
} from "../test/aiIntegrationsFixtures";

// The section reaches the backend only through `invoke`; mock the bridge so
// jsdom never needs the Tauri runtime — and so no test can spawn a real CLI or
// make a real request.
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const commands = commandsFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The section as a whole
// ---------------------------------------------------------------------------

describe("the AI integrations section", () => {
  it("renders exactly two levels, each with its own tabs (AII-FR-01, AII-FR-02, AII-FR-03)", async () => {
    // AII-FR-01 / FR-03.
    backend();
    render(<BothLevels />);

    await waitFor(() =>
      expect(screen.getByTestId("ai-api-level")).toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(screen.getByTestId("agentic-level")).toBeInTheDocument(),
    );

    expect(commands()).toContain("list_ai_api_integrations");
    expect(commands()).toContain("list_agentic_integrations");

    await waitFor(() =>
      expect(apiLevel().getAllByRole("tab").map((t) => t.textContent)).toEqual([
        "OpenRouter",
        "Anthropic",
        "OpenAI",
        "Custom",
      ]),
    );
    // The vendor strip alone: the Claude Code tab holds a strip of its own
    // (AII-FR-IUUM), which is not a tab of this level.
    const vendorStrip = agenticLevel().getByRole("tablist", { name: "Agentic AI" });
    expect(within(vendorStrip).getAllByRole("tab").map((t) => t.textContent)).toEqual([
      "Claude Code",
      "Codex",
      "OpenCode",
      "Claude Agent API",
      "Custom agent API",
    ]);
  });

  it("renders each level as a self-contained section that names nothing itself (AII-FR-01, AII-FR-02, AII-FR-03)", async () => {
    // AII-FR-02: each level is a whole Global settings section, and the section
    // heading in the nav frame is what names it (GLS-FR-16). A heading of its
    // own would name the same thing twice, and — worse — would still read as a
    // sub-heading if the two were ever stacked again.
    backend();
    render(<BothLevels />);

    const api = await screen.findByTestId("ai-api-level");
    const agentic = await screen.findByTestId("agentic-level");
    for (const level of [api, agentic]) {
      expect(level.tagName).toBe("SECTION");
      expect(within(level).queryByRole("heading")).toBeNull();
    }
    // Containment is deliberately *not* asserted: `BothLevels` renders the two
    // as fragment siblings and neither component renders the other, so one
    // could never contain the other and the check would pass no matter what.
    // That the sections are separately mounted is asserted where it can
    // actually fail — `GlobalSettings.test.tsx`, GLS-FR-03, GLS-FR-16.
  });

  it("shares no field between the two levels (AII-FR-02)", async () => {
    // AII-FR-02's "share no control" clause, mounted together so a leak has
    // somewhere to leak to. The navigation half of AII-FR-01, AII-FR-02, AII-FR-10 — that leaving a
    // section unmounts its level and discards its candidate — needs the real
    // section navigation and lives in `GlobalSettings.test.tsx`.
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();

    const apiUrl = await apiLevel().findByLabelText("Base URL");
    await user.clear(apiUrl);
    await user.type(apiUrl, "https://typed-in-api-level.example/v1");

    // The Agentic level's own API tab must not have picked that up.
    await user.click(
      agenticLevel().getByRole("tab", { name: /Claude Agent API/ }),
    );
    expect(await agenticLevel().findByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );

    // And the API level's candidate is where it was left.
    expect(apiLevel().getByLabelText("Base URL")).toHaveValue(
      "https://typed-in-api-level.example/v1",
    );
  });

  it("keeps each level's active marker to itself (AII-FR-04)", async () => {
    // AII-FR-04: activating in one level leaves the other's choice untouched.
    let apiList = [
      apiVerified("anthropic", "https://api.anthropic.com/v1"),
      apiIntegration("openai"),
      apiIntegration("openrouter"),
      apiIntegration("custom"),
    ];
    const agenticList = [
      cliVerified("claude_code", "/bin/claude", { active: true }),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ];
    backend(
      { api: apiList, agentic: agenticList },
      {
        list_ai_api_integrations: () => apiList,
        list_agentic_integrations: () => agenticList,
        detect_agentic_cli_binary: () => ({ path: null }),
        set_active_ai_api_integration: (args) => {
          apiList = apiList.map((i) => ({
            ...i,
            active: i.provider === args?.provider,
          }));
          return apiList;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await apiLevel().findByRole("tab", { name: /Anthropic/ });
    await user.click(
      await apiLevel().findByRole("button", { name: /use this integration/i }),
    );

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-active-marker")).toBeInTheDocument(),
    );
    // The agentic level's marker is exactly where it was.
    expect(
      agenticLevel().getByTestId("agentic-active-marker"),
    ).toBeInTheDocument();
    expect(commands()).not.toContain("set_active_agentic_integration");
  });

  it("holds no dirty state, whatever is done in either level (AII-FR-28)", async () => {
    // AII-FR-28: every action applies at once. The proof is that each action
    // reaches the backend on its own, with no save command anywhere.
    let apiList = [
      apiVerified("anthropic", "https://api.anthropic.com/v1"),
      ...API_ALL.slice(1).map((p) => apiIntegration(p)),
    ];
    // The scenario says "in either level", so both are acted in. With only the
    // API level touched, the agentic level contributed nothing to the asserted
    // set but its mount-time reads — and its own immediate-apply went unproven.
    let agenticList = [
      cliVerified("claude_code", "/bin/claude"),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ];
    backend(
      { api: apiList, agentic: agenticList },
      {
        list_ai_api_integrations: () => apiList,
        list_agentic_integrations: () => agenticList,
        set_ai_api_model: (args) => {
          apiList = apiList.map((i) =>
            i.provider === "anthropic"
              ? { ...i, selectedModel: (args?.modelId as string) ?? null }
              : i,
          );
          return apiList[0];
        },
        set_agentic_integration_model: (args) => {
          agenticList = agenticList.map((v) =>
            v.vendor === "claude_code"
              ? { ...v, selectedModel: (args?.modelId as string) ?? null }
              : v,
          );
          return agenticList[0];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await pick(user, apiLevel(), "Model", "Model A");
    await waitFor(() => expect(commands()).toContain("set_ai_api_model"));

    await pick(user, agenticLevel(), "Model", "Opus");
    await waitFor(() =>
      expect(commands()).toContain("set_agentic_integration_model"),
    );

    // The whole command set, not a name-spelling check: the claim is that each
    // selection reached the backend on its own, with nothing else in between —
    // no save of any kind, in either level.
    expect(new Set(commands())).toEqual(
      new Set([
        "list_ai_api_integrations",
        "list_agentic_integrations",
        "set_ai_api_model",
        "set_agentic_integration_model",
      ]),
    );
  });

  it("renders both levels' empty states with nothing configured at all (AII-FR-29, AII-FR-50)", async () => {
    // AII-FR-29: a first-class empty state naming what is needed, not an error,
    // and no activation control enabled anywhere.
    backend();
    render(<BothLevels />);

    expect(await apiLevel().findByTestId("ai-api-empty")).toHaveTextContent(
      /OpenRouter is not set up yet/i,
    );
    expect(await agenticLevel().findByTestId("agentic-empty")).toHaveTextContent(
      /Claude Code is not set up yet/i,
    );
    for (const level of [apiLevel(), agenticLevel()]) {
      expect(
        level.getByRole("button", { name: /use this integration/i }),
      ).toBeDisabled();
    }
    // Queried by role rather than by the error glyph, which is presentation the
    // component is free to change.
    expect(screen.queryByTestId("ai-api-action-error")).not.toBeInTheDocument();
    expect(screen.queryByTestId("agentic-action-error")).not.toBeInTheDocument();
    expect(document.querySelectorAll(".picker-error")).toHaveLength(0);
  });

  it("renders every control from stored state with no verification call (AII-FR-01)", async () => {
    // AII-FR-01 / the non-functional requirements: nothing contacts a provider
    // or runs a binary until the author activates Verify.
    backend({
      api: [
        apiVerified("anthropic", "https://api.anthropic.com/v1"),
        ...API_ALL.slice(1).map((p) => apiIntegration(p)),
      ],
      agentic: [
        cliVerified("claude_code", "/bin/claude"),
        ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    // Walk every tab in both levels, which is what would reach for a backend if
    // anything did.
    for (const name of [/OpenAI/, /OpenRouter/, /Custom/]) {
      await user.click(await apiLevel().findByRole("tab", { name }));
    }
    for (const name of [/Codex/, /OpenCode/, /Claude Agent API/, /Custom agent API/]) {
      await user.click(await agenticLevel().findByRole("tab", { name }));
    }

    // The complete set, not a couple of negatives: anything that reached a
    // provider or ran a binary would show up here as a new name.
    expect(new Set(commands())).toEqual(
      new Set([
        "list_ai_api_integrations",
        "list_agentic_integrations",
        "detect_agentic_cli_binary",
      ]),
    );
  });
});

// ---------------------------------------------------------------------------
// The status lines (pure)
// ---------------------------------------------------------------------------

describe("the status lines", () => {
  const draft = (over: Partial<AgenticDraft> = {}): AgenticDraft => ({
    ...emptyDraft(),
    ...over,
  });

  it("puts an in-flight verification above everything", () => {
    const i = apiVerified("openai", "https://api.openai.com/v1");
    expect(apiStatus(i, i.baseUrl!, "", "verifying", "an error").text).toBe(
      "Verifying…",
    );
    const a = cliVerified("claude_code", "/bin/claude");
    expect(
      agenticStatus(
        a,
        draft({ path: "/bin/claude" }),
        "verifying",
        "an error",
        false,
      ).text,
    ).toBe("Verifying…");
  });

  it("puts a fresh error above the stored state", () => {
    // The author just acted; the outcome of that action is what they need.
    const i = apiVerified("openai", "https://api.openai.com/v1");
    expect(
      apiStatus(i, i.baseUrl!, "", "idle", "the endpoint refused that key"),
    ).toEqual({ text: "the endpoint refused that key", tone: "warn" });
  });

  it("treats a typed key as an edit even when the URL is untouched", () => {
    // Otherwise the tab would go on claiming the previous key is in force while
    // showing an uncommitted one.
    const i = apiVerified("openai", "https://api.openai.com/v1");
    expect(apiStatus(i, i.baseUrl!, "sk-new", "idle", "").text).toMatch(
      /Not verified/,
    );
    expect(apiStatus(i, i.baseUrl!, "", "idle", "").tone).toBe("ok");
  });

  it("ranks an edited field above a stored verification", () => {
    const a = cliVerified("claude_code", "/bin/claude");
    expect(
      agenticStatus(a, draft({ path: "/bin/other" }), "idle", "", false).text,
    ).toMatch(/Not verified/);
  });

  it("names a detected candidate as detected", () => {
    const a = agenticIntegration("claude_code");
    expect(
      agenticStatus(
        a,
        draft({ path: "/found/claude" }),
        "idle",
        "",
        false,
        "/found/claude",
      ).text,
    ).toMatch(/^Detected/);
  });

  it("reports each kind's degradation only once its field matches the stored one", () => {
    const missing = cliVerified("claude_code", "/bin/claude", {
      state: "missing",
    });
    expect(
      agenticStatus(missing, draft({ path: "/bin/claude" }), "idle", "", false)
        .tone,
    ).toBe("warn");
    const keyGone = apiVerified("openai", "https://api.openai.com/v1", {
      state: "key_unavailable",
    });
    expect(apiStatus(keyGone, keyGone.baseUrl!, "", "idle", "").tone).toBe(
      "warn",
    );
  });

  it("reports a CLI whose stored OAuth token can no longer be read (AII-FR-27)", () => {
    // AIC-FR-15 gives Claude Code both degraded states. The status line has to
    // tell them apart, because a missing binary and an unreadable token call
    // for entirely different corrections.
    const tokenGone = cliVerified("claude_code", "/bin/claude", {
      state: "key_unavailable",
      keyState: "unavailable",
    });
    const status = agenticStatus(
      tokenGone,
      draft({ path: "/bin/claude" }),
      "idle",
      "",
      false,
    );
    expect(status.tone).toBe("warn");
    expect(status.text).toMatch(/OAuth token can no longer be read/);
  });

  it("returns a Claude Code tab to unverified while a new token is typed (AII-FR-22)", () => {
    // A token is an edit like a path is: the tab stops claiming to be verified
    // until Verify commits what is now in front of the author.
    const verified = cliVerified("claude_code", "/bin/claude");
    expect(
      agenticStatus(verified, draft({ path: "/bin/claude" }), "idle", "", false)
        .tone,
    ).toBe("ok");
    expect(
      agenticStatus(
        verified,
        draft({ path: "/bin/claude", oauthToken: SAMPLE_TOKEN }),
        "idle",
        "",
        false,
      ).text,
    ).toMatch(/Not verified/);
  });
});

// ---------------------------------------------------------------------------
// Error messages
// ---------------------------------------------------------------------------

describe("aiErrorMessage", () => {
  it("keeps the failures the author must act on differently apart", () => {
    // AII-FR-09 / FR-20: a wrong URL, a refused key, and an unreachable host are
    // three different things to do next.
    const texts = [
      AI_ERRORS.notFound,
      AI_ERRORS.notExecutable,
      AI_ERRORS.notTheExpectedCli,
      AI_ERRORS.baseUrlInvalid,
      AI_ERRORS.keyMissing,
      AI_ERRORS.unreachable,
      AI_ERRORS.rejected,
      AI_ERRORS.notAnAiEndpoint,
      AI_ERRORS.notAnAgentEndpoint,
      AI_ERRORS.timedOut,
      AI_ERRORS.keychainUnavailable,
      AI_ERRORS.tokenMissing,
      AI_ERRORS.tokenMalformed,
    ].map(aiErrorMessage);
    expect(new Set(texts).size).toBe(texts.length);
    for (const t of texts) expect(t).not.toMatch(/^(undefined|operation failed)$/);
  });

  it("falls back to the raw string rather than swallowing it", () => {
    expect(aiErrorMessage("something odd")).toBe("something odd");
    expect(aiErrorMessage(new Error("boom"))).toBe("boom");
    expect(aiErrorMessage(undefined)).toBe("operation failed");
  });
});
