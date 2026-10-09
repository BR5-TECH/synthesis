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

import { inTabOrder } from "./AiIntegrations";
import { AI_ERRORS } from "../types";
import {
  API_ALL,
  API_NAMES,
  BothLevels,
  agenticLevel,
  apiIntegration,
  apiLevel,
  apiVerified,
  argsOfFor,
  backendFor,
  commandsFor,
  openApiTab,
  openRouterWith,
  optionsOf,
  pick,
  withLadder,
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
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The AI API level
// ---------------------------------------------------------------------------

describe("the AI API level", () => {
  it("AII-FR-QSOR: renders the same rows in the same order in every tab (AII-FR-05)", async () => {
    // AII-FR-05, asserted as real document order — presence alone would survive
    // a reshuffle.
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await apiLevel().findByRole("tab", { name: /Anthropic/ });

    for (const provider of API_ALL) {
      await user.click(
        apiLevel().getByRole("tab", { name: new RegExp(API_NAMES[provider]) }),
      );
      const panel = await screen.findByTestId(`ai-api-panel-${provider}`);
      const rows = [
        within(panel).getByLabelText(provider === "custom" ? "Gateway URL" : "Base URL"),
        within(panel).getByLabelText(provider === "custom" ? "Secret" : "API key"),
        within(panel).getByTestId("ai-api-status"),
        within(panel).getByRole("combobox", { name: "Model" }),
        within(panel).getByLabelText("Turn timeout"),
        within(panel).getByRole("button", { name: /use this integration/i }),
      ];
      expect(
        within(panel).getByRole("button", { name: "Verify" }),
      ).toBeInTheDocument();
      for (let i = 0; i < rows.length - 1; i++) {
        expect(
          rows[i].compareDocumentPosition(rows[i + 1]) &
            Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      }
    }
  });

  it("AII-FR-05, AII-FR-QSOR: the turn timeout follows the reasoning selector and precedes the activation control", async () => {
    backend(openRouterWith([withLadder("a/model", ["high", "low"])], "a/model"));
    render(<BothLevels />);
    const panel = await screen.findByTestId("ai-api-panel-openrouter");
    const rows = [
      await within(panel).findByRole("combobox", { name: "Reasoning" }),
      within(panel).getByLabelText("Turn timeout"),
      within(panel).getByRole("button", { name: /use this integration/i }),
    ];
    for (let i = 0; i < rows.length - 1; i++) {
      expect(
        rows[i].compareDocumentPosition(rows[i + 1]) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
  });

  it("AII-FR-06: prefills a named provider's URL and leaves the Custom gateway URL empty", async () => {
    // AII-FR-06: Custom is the tab through which the company LLM Gateway is
    // configured, so it ships no default at all.
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(await apiLevel().findByLabelText("Base URL")).toHaveValue(
      "https://openrouter.ai/api/v1",
    );
    await openApiTab(user, apiLevel(), /Anthropic/);
    expect(apiLevel().getByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );

    await user.click(apiLevel().getByRole("tab", { name: /Custom/ }));
    expect(await apiLevel().findByLabelText("Gateway URL")).toHaveValue("");
  });

  it("discards an abandoned candidate when an API tab is left (AII-FR-10)", async () => {
    // AII-FR-10: a candidate that never verified is not a fact about anything,
    // so re-entering the tab must show the last configuration that verified —
    // not a URL the application would never actually call. The agentic level's
    // twin is AII-FR-18, AII-FR-21, AII-FR-22.
    backend({
      api: [
        apiVerified("anthropic", "https://api.anthropic.com/v1"),
        ...API_ALL.slice(1).map((p) => apiIntegration(p)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    const url = await apiLevel().findByLabelText("Base URL");
    await user.clear(url);
    await user.type(url, "https://abandoned.example/v1");
    await user.type(apiLevel().getByLabelText("API key"), "sk-never-committed");

    await user.click(apiLevel().getByRole("tab", { name: /OpenAI/ }));
    await user.click(apiLevel().getByRole("tab", { name: /Anthropic/ }));

    expect(await apiLevel().findByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );
    expect(apiLevel().getByLabelText("API key")).toHaveValue("");
    expect(commands()).not.toContain("verify_ai_api_integration");
    // AII-FR-30: the abandoned key is gone from the page entirely.
    expect(document.body.innerHTML).not.toContain("sk-never-committed");
  });

  it("AII-FR-08, AAP-FR-04: verifies a gateway with a secret and states that every key is required", async () => {
    backend(
      {},
      {
        verify_ai_api_integration: (args) => {
          expect(args?.apiKey).toBe("gw-secret");
          return apiVerified("custom", args?.baseUrl as string);
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await apiLevel().findByRole("tab", { name: /Custom/ }),
    );
    expect(
      await apiLevel().findByTestId("ai-api-key-requirement"),
    ).toHaveTextContent(/required — sent as a bearer token/i);

    await user.type(
      apiLevel().getByLabelText("Gateway URL"),
      "https://llm-gateway.example.com",
    );
    await user.type(apiLevel().getByLabelText("Secret"), "gw-secret");
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /verified/i,
      ),
    );
    expect(
      apiLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeEnabled();

    // A named provider says a key is required as well.
    await user.click(apiLevel().getByRole("tab", { name: /Anthropic/ }));
    expect(apiLevel().getByTestId("ai-api-key-requirement")).toHaveTextContent(
      /requires an API key/i,
    );
  });

  it("never populates the key field and shows only the masked hint (AII-FR-07, AII-FR-30)", async () => {
    // AII-FR-07 / FR-30: the strongest claim in this surface. The key the author
    // entered must appear nowhere in the rendered page.
    const secret = "sk-proj-supersecret3f9a";
    backend(
      {},
      {
        verify_ai_api_integration: () =>
          apiVerified("anthropic", "https://api.anthropic.com/v1"),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    const key = await apiLevel().findByLabelText("API key");
    expect(key).toHaveAttribute("type", "password");
    await user.type(key, secret);
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /verified/i,
      ),
    );
    // Emptied on commit, and represented only by the hint from here.
    expect(apiLevel().getByLabelText("API key")).toHaveValue("");
    expect(apiLevel().getByLabelText("API key")).toHaveAttribute(
      "placeholder",
      expect.stringContaining("3f9a"),
    );
    expect(document.body.innerHTML).not.toContain(secret);
    expect(document.body.innerHTML).not.toContain("supersecret");
  });

  it("commits the configuration on a successful verification (AII-FR-09, AII-FR-10)", async () => {
    // AII-FR-09 / FR-10.
    backend(
      {},
      {
        verify_ai_api_integration: (args) =>
          apiVerified("anthropic", args?.baseUrl as string),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    const url = await apiLevel().findByLabelText("Base URL");
    await user.clear(url);
    await user.type(url, "https://gateway.internal/v1");
    await user.type(apiLevel().getByLabelText("API key"), "sk-1234");
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /verified/i,
      ),
    );
    expect(argsOf("verify_ai_api_integration")).toEqual({
      provider: "anthropic",
      baseUrl: "https://gateway.internal/v1",
      apiKey: "sk-1234",
    });
  });

  it("distinguishes a refused key from an unreachable host (AII-FR-09, AII-FR-10)", async () => {
    // AII-FR-09: each failure names itself on the status line, beneath the
    // fields — never as a modal or a transient notification.
    let failure: string = AI_ERRORS.rejected;
    backend(
      {
        api: [
          apiVerified("anthropic", "https://api.anthropic.com/v1"),
          ...API_ALL.slice(1).map((p) => apiIntegration(p)),
        ],
      },
      {
        verify_ai_api_integration: () => {
          throw failure;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await user.type(await apiLevel().findByLabelText("API key"), "sk-bad");
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /refused that key/i,
      ),
    );
    // The previously stored configuration is unchanged.
    expect(apiLevel().getByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    failure = AI_ERRORS.unreachable;
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /could not be reached/i,
      ),
    );
  });

  it("AII-FR-09: a refused certificate names the host and the cause in the status line", async () => {
    let failure = "tls_untrusted:unknown_issuer:gateway.corp.example";
    backend(
      {
        api: [
          apiVerified("anthropic", "https://api.anthropic.com/v1"),
          ...API_ALL.slice(1).map((p) => apiIntegration(p)),
        ],
      },
      {
        verify_ai_api_integration: () => {
          throw failure;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await user.type(await apiLevel().findByLabelText("API key"), "sk-secret");
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /certificate of gateway\.corp\.example is not trusted.*issuer.*unknown/i,
      ),
    );
    expect(apiLevel().getByTestId("ai-api-status")).not.toHaveTextContent(
      /could not be reached/i,
    );
    expect(apiLevel().getByTestId("ai-api-status")).not.toHaveTextContent(
      "sk-secret",
    );

    failure = "tls_untrusted:expired:gateway.corp.example";
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /gateway\.corp\.example.*expired/i,
      ),
    );

    failure = "tls_untrusted:hostname_mismatch:gateway.corp.example";
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /gateway\.corp\.example.*host name/i,
      ),
    );
  });

  it("returns to an unverified presentation when a field is edited (AII-FR-11)", async () => {
    // AII-FR-11: and no operation is invoked by the edit itself.
    backend({
      api: [
        apiVerified("anthropic", "https://api.anthropic.com/v1"),
        ...API_ALL.slice(1).map((p) => apiIntegration(p)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(await apiLevel().findByTestId("ai-api-status")).toHaveTextContent(
      /verified/i,
    );

    // The agentic level's mount is a *chain*: `detect_agentic_cli_binary` is
    // only issued once `list_agentic_integrations` has resolved (AII-FR-17), so
    // the API level's status settling does not mean the mount is done. Baselined
    // before that third call lands, the count below grows by one on the next
    // flush and the edit gets the blame for a command it never caused.
    await waitFor(() =>
      expect(commands()).toContain("detect_agentic_cli_binary"),
    );

    const before = commands().length;
    await user.type(apiLevel().getByLabelText("Base URL"), "/extra");
    expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
      /Not verified/i,
    );
    expect(commands().length).toBe(before);

    // Restoring it returns the tab to verified without a round trip.
    const url = apiLevel().getByLabelText("Base URL");
    await user.clear(url);
    await user.type(url, "https://api.anthropic.com/v1");
    expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
      /verified/i,
    );
  });

  it("leads the model selector with the provider default and names the source (AII-FR-12)", async () => {
    // AII-FR-12.
    let list = [
      apiVerified("anthropic", "https://api.anthropic.com/v1", {
        modelsOrigin: "probed",
      }),
      apiVerified("openai", "https://api.openai.com/v1", {
        modelsOrigin: "catalog",
      }),
      apiIntegration("openrouter"),
      apiIntegration("custom"),
    ];
    backend(
      { api: list },
      {
        list_ai_api_integrations: () => list,
        set_ai_api_model: (args) => {
          list = list.map((i) =>
            i.provider === "anthropic"
              ? { ...i, selectedModel: (args?.modelId as string) ?? null }
              : i,
          );
          return list[0];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await apiLevel().findByRole("combobox", { name: "Model" });
    expect(await optionsOf(user, apiLevel(), "Model")).toEqual([
      "Provider default",
      "Model A",
      "Model B",
    ]);
    expect(apiLevel().getByTestId("ai-api-models-origin")).toHaveTextContent(
      "from the endpoint",
    );

    await user.click(
      within(apiLevel().getByRole("listbox", { name: "Model" })).getByRole(
        "option",
        { name: "Model B" },
      ),
    );
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "anthropic",
        modelId: "model-b",
      }),
    );

    // A catalog-sourced list says so instead.
    await user.click(apiLevel().getByRole("tab", { name: /OpenAI/ }));
    expect(
      await apiLevel().findByTestId("ai-api-models-origin"),
    ).toHaveTextContent(/bundled list/i);
  });

  it("selects the provider's own default when the default entry is chosen (AII-FR-12)", async () => {
    backend(
      {
        api: [
          apiVerified("anthropic", "https://api.anthropic.com/v1", {
            selectedModel: "model-a",
          }),
          ...API_ALL.slice(1).map((p) => apiIntegration(p)),
        ],
      },
      {
        set_ai_api_model: (args) => {
          expect(args?.modelId).toBeNull();
          return apiVerified("anthropic", "https://api.anthropic.com/v1");
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);
    await apiLevel().findByRole("combobox", { name: "Model" });
    await pick(user, apiLevel(), "Model", "Provider default");
    await waitFor(() => expect(commands()).toContain("set_ai_api_model"));
  });

  it("enables activation only for a verified provider and marks exactly one (AII-FR-13)", async () => {
    // AII-FR-13.
    let list = [
      apiVerified("anthropic", "https://api.anthropic.com/v1"),
      apiIntegration("openai"),
      apiIntegration("openrouter"),
      apiIntegration("custom"),
    ];
    backend(
      { api: list },
      {
        list_ai_api_integrations: () => list,
        set_active_ai_api_integration: (args) => {
          list = list.map((i) => ({
            ...i,
            active: i.provider === args?.provider,
          }));
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await apiLevel().findByRole("tab", { name: /Anthropic/ });
    expect(
      apiLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeEnabled();

    await user.click(apiLevel().getByRole("tab", { name: /OpenAI/ }));
    expect(
      await apiLevel().findByRole("button", { name: /use this integration/i }),
    ).toBeDisabled();

    await user.click(apiLevel().getByRole("tab", { name: /Anthropic/ }));
    await user.click(
      await apiLevel().findByRole("button", { name: /use this integration/i }),
    );
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-active-marker")).toBeInTheDocument(),
    );
    // The marker replaces the activation control in the active tab.
    expect(
      apiLevel().queryByRole("button", { name: /use this integration/i }),
    ).not.toBeInTheDocument();
  });

  it("marks a sole verified provider active without any activation (AII-FR-13)", async () => {
    // AII-FR-13: the marker follows the record's flag, not the author's click.
    backend({
      api: [
        apiVerified("anthropic", "https://api.anthropic.com/v1", {
          active: true,
        }),
        ...API_ALL.slice(1).map((p) => apiIntegration(p)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);
    expect(
      await apiLevel().findByTestId("ai-api-active-marker"),
    ).toBeInTheDocument();
    expect(commands()).not.toContain("set_active_ai_api_integration");
  });

  it("marks neither provider once a second verifies and none was activated (AII-FR-13)", async () => {
    // The other half of the set-of-one rule, and the half a regression would
    // invert: with two verified and no explicit choice, no record carries the
    // flag, so both tabs must offer an enabled activation control.
    backend({
      api: [
        apiVerified("anthropic", "https://api.anthropic.com/v1"),
        apiVerified("openai", "https://api.openai.com/v1"),
        apiIntegration("openrouter"),
        apiIntegration("custom"),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await apiLevel().findByRole("tab", { name: /Anthropic/ });
    expect(
      apiLevel().queryByTestId("ai-api-active-marker"),
    ).not.toBeInTheDocument();
    expect(
      apiLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeEnabled();

    await user.click(apiLevel().getByRole("tab", { name: /OpenAI/ }));
    expect(
      apiLevel().queryByTestId("ai-api-active-marker"),
    ).not.toBeInTheDocument();
    expect(
      await apiLevel().findByRole("button", { name: /use this integration/i }),
    ).toBeEnabled();
  });

  it("returns a tab to its empty state when cleared (AII-FR-14, AII-FR-29)", async () => {
    // AII-FR-14.
    let list = [
      apiVerified("anthropic", "https://api.anthropic.com/v1", { active: true }),
      ...API_ALL.slice(1).map((p) => apiIntegration(p)),
    ];
    backend(
      { api: list },
      {
        list_ai_api_integrations: () => list,
        clear_ai_api_integration: () => {
          list = API_ALL.map((p) => apiIntegration(p));
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await user.click(await apiLevel().findByRole("button", { name: /clear/i }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-empty")).toBeInTheDocument(),
    );
    expect(
      apiLevel().queryByTestId("ai-api-active-marker"),
    ).not.toBeInTheDocument();
    // Back to the provider's default URL, with no hint left.
    expect(apiLevel().getByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );
    expect(apiLevel().getByLabelText("API key")).toHaveAttribute(
      "placeholder",
      "required",
    );
  });

  it("reports an unreadable key while keeping the configuration (AII-FR-15)", async () => {
    // AII-FR-15.
    backend({
      api: [
        apiVerified("anthropic", "https://api.anthropic.com/v1", {
          state: "key_unavailable",
          keyState: "unavailable",
          selectedModel: "model-a",
        }),
        ...API_ALL.slice(1).map((p) => apiIntegration(p)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    expect(await apiLevel().findByTestId("ai-api-status")).toHaveTextContent(
      /can no longer be read/i,
    );
    expect(apiLevel().getByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );
    expect(apiLevel().getByRole("combobox", { name: "Model" })).toHaveTextContent(
      "Model A",
    );
    expect(
      apiLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeDisabled();
  });

  it("renders a rejected selection beside the action, not on the status line", async () => {
    backend(
      {
        api: [
          apiVerified("anthropic", "https://api.anthropic.com/v1"),
          ...API_ALL.slice(1).map((p) => apiIntegration(p)),
        ],
      },
      {
        set_ai_api_model: () => {
          throw AI_ERRORS.unknownModel;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    await apiLevel().findByRole("combobox", { name: "Model" });
    await pick(user, apiLevel(), "Model", "Model A");
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-action-error")).toHaveTextContent(
        /not one this backend offers/i,
      ),
    );
    // The status line still reports the endpoint, which is not in question.
    expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
      /verified/i,
    );
  });

  it("reports a failure to load rather than rendering half a level", async () => {
    backend(
      {},
      {
        list_ai_api_integrations: () => {
          throw "boom";
        },
      },
    );
    render(<BothLevels />);
    expect(await apiLevel().findByText(/✗ boom/)).toBeInTheDocument();
    // The other level is unaffected.
    expect(await agenticLevel().findAllByRole("tab")).toHaveLength(5);
  });
});

describe("the API level's tab order", () => {
  it("is the level's own, whatever order the backend returned (AII-FR-01, AII-FR-02, AII-FR-03)", async () => {
    // AII-FR-03: OpenRouter first, and the ordering is a fact about this
    // surface rather than a coupling to the backend's iteration order.
    const shuffled = [
      apiIntegration("custom"),
      apiIntegration("openai"),
      apiIntegration("openrouter"),
      apiIntegration("anthropic"),
    ];
    expect(inTabOrder(shuffled).map((i) => i.provider)).toEqual([
      "openrouter",
      "anthropic",
      "openai",
      "custom",
    ]);

    backend({ api: shuffled });
    render(<BothLevels />);
    await waitFor(() =>
      expect(apiLevel().getAllByRole("tab").map((t) => t.textContent)).toEqual([
        "OpenRouter",
        "Anthropic",
        "OpenAI",
        "Custom",
      ]),
    );
    // And the first tab is the one selected on open.
    expect(
      apiLevel().getByRole("tab", { name: /OpenRouter/ }),
    ).toHaveAttribute("aria-selected", "true");
  });
});

describe("verifying a provider whose key is already stored (AII-FR-07)", () => {
  it("verifies with an empty key field and says so (AII-FR-07, AII-FR-08)", async () => {
    // The defect this guards: the key field is never populated from the record,
    // so an empty field is the *normal* state of a configured provider. If that
    // could not verify, a stored key would make re-verification impossible
    // without the author holding their key a second time.
    backend(
      {
        api: [
          apiVerified("openrouter", "https://openrouter.ai/api/v1"),
          ...API_ALL.filter((p) => p !== "openrouter").map((p) =>
            apiIntegration(p),
          ),
        ],
      },
      {
        verify_ai_api_integration: (args) => {
          expect(args?.apiKey, "an untouched field sends no key").toBeNull();
          return apiVerified("openrouter", "https://openrouter.ai/api/v1");
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(
      await apiLevel().findByTestId("ai-api-key-requirement"),
    ).toHaveTextContent(/leave empty to verify again with the stored key/i);

    await user.click(apiLevel().getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        /verified/i,
      ),
    );
    // AII-FR-07: the hint is still the only representation of the key.
    expect(apiLevel().getByLabelText("API key")).toHaveAttribute(
      "placeholder",
      expect.stringContaining("3f9a"),
    );
  });

  it("AII-FR-07, AII-FR-08: still says a key is required when none is stored", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Anthropic/);

    expect(
      apiLevel().getByTestId("ai-api-key-requirement"),
    ).toHaveTextContent(/requires an api key/i);

    // ...and Custom says its secret is required too, and never optional.
    await openApiTab(user, apiLevel(), /Custom/);
    const note = apiLevel().getByTestId("ai-api-key-requirement");
    expect(note).toHaveTextContent(/required/i);
    expect(note).not.toHaveTextContent(/optional/i);
  });
});

// ---------------------------------------------------------------------------
// AII-FR-54, AII-FR-12, AII-FR-45: what the AI API level says its selections are for
// ---------------------------------------------------------------------------

describe("the AI API level's explanation (AII-FR-54, AII-FR-12, AII-FR-45)", () => {
  /**
   * The level's rendered text, one entry per text node.
   *
   * Deliberately **not** `level.textContent` split on sentence punctuation:
   * React renders sibling elements with no whitespace between them, so such a
   * split never breaks at an element boundary and the explanation's last
   * sentence swallows every field, status line, and label after it. A stray
   * line added anywhere below would then be concatenated into an entry that
   * already satisfies the assertion, and the check would guard nothing.
   */
  const textBlocks = (level: HTMLElement) => {
    const walker = document.createTreeWalker(level, NodeFilter.SHOW_TEXT);
    const out: string[] = [];
    let node: Node | null;
    while ((node = walker.nextNode())) {
      const text = (node.textContent ?? "").trim();
      if (text) out.push(text);
    }
    return out;
  };

  it("says who uses the model and the reasoning it holds, above the tab strip (AII-FR-54)", async () => {
    backend();
    render(<BothLevels />);

    const level = await screen.findByTestId("ai-api-level");
    const explanation = within(level).getByTestId("ai-api-explanation");

    // What it configures, and — the part an author cannot work out from the
    // fields — who its model and reasoning selections serve.
    expect(explanation).toHaveTextContent(/endpoints Synthesis calls itself/i);
    expect(explanation).toHaveTextContent(
      /model and reasoning here are used only by the graduation loop/i,
    );
    // And where an agent's own model is chosen instead.
    expect(explanation).toHaveTextContent(
      /agent uses its own model and reasoning/i,
    );
    expect(explanation).toHaveTextContent(/Agents section/i);

    // Above the tab strip and before any field: an explanation an author meets
    // after choosing a model has already failed to do its job.
    const tablist = within(level).getByRole("tablist", { name: "AI API" });
    expect(
      explanation.compareDocumentPosition(tablist) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("AII-FR-QTZF: never says a selection made here changes what an agent runs on (AII-FR-54)", async () => {
    backend(
      openRouterWith([withLadder("a/model", ["high", "low"])], "a/model"),
    );
    render(<BothLevels />);
    const level = await screen.findByTestId("ai-api-level");
    await within(level).findByRole("combobox", { name: "Model" });

    // Exactly three lines of the level mention an agent at all: the one saying
    // the agent's model is its own, the one saying agents follow the provider
    // active for the project (AII-FR-54), and the turn timeout's explanation, which
    // bounds how long an agent's turn runs and not what it runs on
    // (AII-FR-QTZF). Asserted as an equality over the
    // whole level rather than as a predicate over the explanation, because the
    // requirement is about what the level does *not* say: a stray hint beside
    // the model selector would mislead exactly as badly, and only an equality
    // turns red when one appears.
    expect(textBlocks(level).filter((t) => /\bagent/i.test(t))).toEqual([
      "The model and reasoning here are used only by the graduation loop. An " +
        "agent uses its own model and reasoning, set in the Agents section.",
      "Agents are served by the provider that is active for the project, so " +
        "activating a provider here changes the endpoint every agent uses.",
      "Bounds each agent conversation turn on this provider. Empty uses the " +
        "project's execution timeout, or 5 minutes.",
    ]);
  });

  it("leaves the model and reasoning selectors behaving exactly as they otherwise would (AII-FR-12, AII-FR-45)", async () => {
    backend(
      openRouterWith(
        [
          withLadder("a/model", ["high", "low"]),
          withLadder("b/model", ["high", "low"]),
        ],
        "a/model",
      ),
      {
        set_ai_api_model: (args) => ({
          ...apiVerified("openrouter", "https://openrouter.ai/api/v1", {
            models: [
              withLadder("a/model", ["high", "low"]),
              withLadder("b/model", ["high", "low"]),
            ],
            selectedModel: (args?.modelId as string) ?? null,
          }),
        }),
        set_ai_api_reasoning: (args) => ({
          ...apiVerified("openrouter", "https://openrouter.ai/api/v1", {
            models: [
              withLadder("a/model", ["high", "low"]),
              withLadder("b/model", ["high", "low"]),
            ],
            selectedModel: "b/model",
          }),
          selectedReasoning: args?.choice,
        }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const level = await screen.findByTestId("ai-api-level");
    const before = within(level).getByTestId("ai-api-explanation").textContent;

    await within(level).findByRole("combobox", { name: "Model" });
    await pick(user, apiLevel(), "Model", "b/model");
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "openrouter",
        modelId: "b/model",
      }),
    );

    await pick(user, apiLevel(), "Reasoning", "low");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "openrouter",
        choice: { kind: "effort", effort: "low" },
      }),
    );

    // The explanation is a statement about who reads the selection, not a
    // status line that follows it.
    expect(
      within(await screen.findByTestId("ai-api-level")).getByTestId(
        "ai-api-explanation",
      ).textContent,
    ).toBe(before);
  });
});
