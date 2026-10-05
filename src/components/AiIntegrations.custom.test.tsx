// The Custom tab of the AI API level - `specifications/ui/AII-ai-integrations.md`
// AII-FR-06, AII-FR-08, AII-FR-12, AII-FR-14, AII-FR-41 and AII-FR-54, against the
// backend contract of `AAP-ai-api-integrations.md` AAP-FR-04, AAP-FR-KRVT,
// AAP-FR-MDLQ and AAP-FR-RTMZ.
//
// Custom is the tab through which the company LLM Gateway is configured: a
// gateway URL without /v1, a secret that is always required, and a model list
// discovered from the gateway with the route each model supports.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAgentRegistry, subscribe } from "../state/agentRegistry";
import { ProjectAiIntegrations } from "./AiIntegrations";
import { AI_ERRORS } from "../types";
import type { ModelOption } from "../types";
import {
  API_ALL,
  BothLevels,
  apiIntegration,
  apiLevel,
  apiVerified,
  argsOfFor,
  backendFor,
  commandsFor,
  customWith,
  openApiTab,
  optionsOf,
  pick,
} from "../test/aiIntegrationsFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const commands = commandsFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

let registryBumps = 0;
let unsubscribe: (() => void) | null = null;

beforeEach(() => {
  invokeMock.mockReset();
  resetAgentRegistry();
  registryBumps = 0;
  unsubscribe = subscribe(() => {
    registryBumps += 1;
  });
});

afterEach(() => {
  unsubscribe?.();
  cleanup();
});

const GATEWAY = "https://llm-gateway.example.com";

const GATEWAY_MODELS: ModelOption[] = [
  { id: "gw-both", label: "Gateway Both", mode: null },
  { id: "gw-absent", label: "Gateway Absent" },
  { id: "gw-chat", label: "Gateway Chat", mode: "chat" },
  { id: "gw-resp", label: "Gateway Resp", mode: "responses" },
];

describe("the Custom tab's fields (AII-FR-06, AII-FR-08, AAP-FR-04)", () => {
  it("AII-FR-06: labels the URL Gateway URL, leaves it empty, and says host or base URL without /v1", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    expect(await apiLevel().findByLabelText("Gateway URL")).toHaveValue("");
    expect(apiLevel().getByTestId("ai-api-url-hint")).toHaveTextContent(
      "Host or base URL, without /v1.",
    );
    expect(apiLevel().queryByLabelText("Base URL")).not.toBeInTheDocument();
  });

  it("AII-FR-08, AAP-FR-04: labels the key Secret, says it is required and sent as a bearer token, and never says optional", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    const secret = await apiLevel().findByLabelText("Secret");
    expect(secret).toHaveAttribute("placeholder", "required");
    const note = apiLevel().getByTestId("ai-api-key-requirement");
    expect(note).toHaveTextContent(/required/i);
    expect(note).toHaveTextContent(/bearer token/i);
    expect(note).not.toHaveTextContent(/optional/i);
    expect(apiLevel().getByTestId("ai-api-panel-custom")).not.toHaveTextContent(
      /optional/i,
    );
  });

  it("AII-FR-08: a stored secret still says it is required, and what the empty field does", async () => {
    backend({
      api: customWith(GATEWAY_MODELS, null).api,
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    const note = await apiLevel().findByTestId("ai-api-key-requirement");
    expect(note).toHaveTextContent(/required/i);
    expect(note).toHaveTextContent(/leave empty to verify again with the stored key/i);
  });

  it("AII-FR-08, AAP-FR-KRVT: verifying with an empty field and no stored key shows key_missing inline and stores nothing", async () => {
    backend(
      {},
      {
        verify_ai_api_integration: () => {
          throw AI_ERRORS.keyMissing;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    await user.type(await apiLevel().findByLabelText("Gateway URL"), GATEWAY);
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(
        "This provider needs an API key.",
      ),
    );
    // The backend refuses; the UI does not treat Custom as a key-optional tab.
    expect(argsOf("verify_ai_api_integration")?.apiKey).toBeNull();
    expect(
      apiLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeDisabled();
    expect(registryBumps, "a refused verification changed nothing").toBe(0);
  });
});

describe("verifying the gateway (AII-FR-09, AII-FR-10, AAP-FR-MDLQ)", () => {
  it("AII-FR-08, AII-FR-12, AAP-FR-MDLQ: a verification sends the secret, shows verified, and tells the Agents sections", async () => {
    backend(
      {},
      {
        verify_ai_api_integration: (args) => {
          expect(args?.provider).toBe("custom");
          expect(args?.baseUrl).toBe(GATEWAY);
          expect(args?.apiKey).toBe("gw-secret");
          return apiVerified("custom", GATEWAY, { models: GATEWAY_MODELS });
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    await user.type(await apiLevel().findByLabelText("Gateway URL"), GATEWAY);
    await user.type(apiLevel().getByLabelText("Secret"), "gw-secret");
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(/verified/i),
    );
    expect(registryBumps).toBeGreaterThan(0);
    // The secret is never rendered back (AII-FR-30).
    expect(document.body.innerHTML).not.toContain("gw-secret");
    expect(apiLevel().getByLabelText("Secret")).toHaveValue("");
    expect(
      apiLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeEnabled();
  });

  it.each([
    [AI_ERRORS.notAnAiEndpoint, "That URL answered, but not as a conversational API."],
    [AI_ERRORS.rejected, "The endpoint refused that key."],
    [AI_ERRORS.unreachable, "That endpoint could not be reached. Check the URL and your network."],
  ])("AII-FR-09, AAP-FR-MDLQ: shows the typed failure %s inline and keeps the tab unverified", async (code, text) => {
    backend(
      {},
      {
        verify_ai_api_integration: () => {
          throw code;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    await user.type(await apiLevel().findByLabelText("Gateway URL"), GATEWAY);
    await user.type(apiLevel().getByLabelText("Secret"), "gw-secret");
    await user.click(apiLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(text),
    );
    expect(apiLevel().getByTestId("ai-api-status")).not.toHaveTextContent(/verified/i);
    expect(apiLevel().getByTestId("ai-api-status")).not.toHaveTextContent("gw-secret");
    expect(registryBumps).toBe(0);
  });
});

describe("route-aware model discovery (AII-FR-12, AAP-FR-RTMZ)", () => {
  it("AII-FR-12, AAP-FR-RTMZ: states the list came from the Custom gateway and names each model's route", async () => {
    backend(customWith(GATEWAY_MODELS, null));
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    expect(await apiLevel().findByTestId("ai-api-models-origin")).toHaveTextContent(
      "from the Custom gateway",
    );
    expect(await optionsOf(user, apiLevel(), "Model")).toEqual([
      "Provider default",
      "Gateway Both · chat + responses",
      "Gateway Absent · chat + responses",
      "Gateway Chat · chat",
      "Gateway Resp · responses",
    ]);
  });

  it("AII-FR-12: the status line names the selected model's route", async () => {
    backend(customWith(GATEWAY_MODELS, "gw-chat"));
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    expect(await apiLevel().findByTestId("ai-api-models-origin")).toHaveTextContent(
      "from the Custom gateway · chat",
    );
  });

  it("AII-FR-12: other providers name no route and do not claim a gateway", async () => {
    backend({
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          models: GATEWAY_MODELS,
        }),
        ...API_ALL.filter((p) => p !== "openrouter").map((p) => apiIntegration(p)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(await apiLevel().findByTestId("ai-api-models-origin")).not.toHaveTextContent(
      /Custom gateway/,
    );
    expect((await optionsOf(user, apiLevel(), "Model")).join()).not.toMatch(
      /chat|responses/,
    );
  });

  it("AII-FR-12, AII-FR-36: filters by label or identifier and chooses through set ai api model", async () => {
    backend(customWith(GATEWAY_MODELS, null), {
      set_ai_api_model: (args) => {
        const base = customWith(GATEWAY_MODELS, args?.modelId as string).api[0];
        return base;
      },
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    await user.click(await apiLevel().findByRole("combobox", { name: "Model" }));
    await user.type(apiLevel().getByLabelText("Filter model"), "GW-RESP");
    expect(
      within(apiLevel().getByRole("listbox", { name: "Model" }))
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Gateway Resp · responses"]);
    await user.keyboard("{Enter}");

    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "custom",
        modelId: "gw-resp",
      }),
    );
    expect(await apiLevel().findByTestId("ai-api-models-origin")).toHaveTextContent(
      "from the Custom gateway · responses",
    );
  });

  it("AII-FR-12: picking by clicking an option with a route suffix selects its identifier", async () => {
    backend(customWith(GATEWAY_MODELS, null), {
      set_ai_api_model: (args) =>
        customWith(GATEWAY_MODELS, args?.modelId as string).api[0],
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);
    await apiLevel().findByRole("combobox", { name: "Model" });

    await pick(user, apiLevel(), "Model", /Gateway Chat/);
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "custom",
        modelId: "gw-chat",
      }),
    );
  });
});

describe("no reasoning for Custom (AII-FR-41)", () => {
  it("AII-FR-41: renders no reasoning selector for any Custom model", async () => {
    backend(customWith(GATEWAY_MODELS, "gw-both"));
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);
    await apiLevel().findByRole("combobox", { name: "Model" });

    expect(apiLevel().queryByRole("combobox", { name: "Reasoning" })).not.toBeInTheDocument();
    expect(apiLevel().queryByTestId("ai-api-reasoning")).not.toBeInTheDocument();
    expect(apiLevel().queryByTestId("ai-api-reasoning-stale")).not.toBeInTheDocument();
  });

  it("AII-FR-41: renders none even if a Custom record were to carry reasoning", async () => {
    // Only the OpenRouter tab renders the selector, whatever a record holds.
    const withReasoning: ModelOption[] = [
      {
        id: "gw-r",
        label: "Gateway R",
        reasoning: { mandatory: false, supportedEfforts: ["high", "low"] },
      },
    ];
    backend(customWith(withReasoning, "gw-r"));
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);
    await apiLevel().findByRole("combobox", { name: "Model" });

    expect(apiLevel().queryByRole("combobox", { name: "Reasoning" })).not.toBeInTheDocument();
  });
});

describe("clearing, activating and notifying the Agents sections (AII-FR-14, AII-FR-54)", () => {
  it("AII-FR-14: Clear returns Custom to the unconfigured presentation and tells the Agents sections", async () => {
    let list = customWith(GATEWAY_MODELS, "gw-chat").api;
    backend(
      { api: list },
      {
        clear_ai_api_integration: () => {
          list = API_ALL.map((p) => apiIntegration(p));
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    await user.click(await apiLevel().findByRole("button", { name: /clear/i }));
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-empty")).toBeInTheDocument(),
    );
    expect(apiLevel().getByLabelText("Gateway URL")).toHaveValue("");
    expect(registryBumps).toBeGreaterThan(0);
  });

  it("AII-FR-54: activating a provider tells the Agents sections", async () => {
    let list = customWith(GATEWAY_MODELS, null).api;
    backend(
      { api: list },
      {
        set_active_ai_api_integration: (args) => {
          list = list.map((i) => ({ ...i, active: i.provider === args?.provider }));
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);

    await user.click(await apiLevel().findByRole("button", { name: /use this integration/i }));
    await waitFor(() => expect(commands()).toContain("set_active_ai_api_integration"));
    await waitFor(() => expect(registryBumps).toBeGreaterThan(0));
  });

  it("AII-FR-54: choosing a model tells the Agents sections", async () => {
    backend(customWith(GATEWAY_MODELS, null), {
      set_ai_api_model: (args) =>
        customWith(GATEWAY_MODELS, args?.modelId as string).api[0],
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);
    await apiLevel().findByRole("combobox", { name: "Model" });

    await pick(user, apiLevel(), "Model", /Gateway Resp/);
    await waitFor(() => expect(registryBumps).toBeGreaterThan(0));
  });

  it("AII-FR-54: a refused model choice tells nobody", async () => {
    backend(customWith(GATEWAY_MODELS, null), {
      set_ai_api_model: () => {
        throw AI_ERRORS.unknownModel;
      },
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await openApiTab(user, apiLevel(), /Custom/);
    await apiLevel().findByRole("combobox", { name: "Model" });

    await pick(user, apiLevel(), "Model", /Gateway Resp/);
    await screen.findByTestId("ai-api-action-error");
    expect(registryBumps).toBe(0);
  });
});

describe("the project override (AII-FR-54, AAP-FR-APRV)", () => {
  it("AII-FR-54: choosing a project override tells the Agents sections, because it changes the provider that serves agents", async () => {
    backend(
      {
        api: [
          apiVerified("anthropic", "https://api.anthropic.com/v1", { active: true }),
          apiVerified("custom", GATEWAY, { models: GATEWAY_MODELS }),
        ],
        projectApi: { provider: "anthropic", resolution: "inherited", overrideProvider: null },
      },
      {
        set_project_ai_api_integration: (args) => ({
          provider: args?.provider,
          resolution: "overridden",
          overrideProvider: args?.provider,
        }),
      },
    );
    render(<ProjectAiIntegrations />);
    const user = userEvent.setup();

    const control = within(await screen.findByTestId("settings-ai-api-integration"));
    await user.click(control.getByRole("button", { name: /change/i }));
    await user.selectOptions(control.getByLabelText("AI API integration"), "custom");
    await waitFor(() =>
      expect(
        control.getByTestId("settings-ai-api-integration-resolution"),
      ).toHaveTextContent(/overrides the global choice/),
    );
    expect(registryBumps).toBeGreaterThan(0);
  });
});
