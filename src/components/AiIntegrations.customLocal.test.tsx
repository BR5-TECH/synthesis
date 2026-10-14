// The local-model rows of the Custom tab - `specifications/ui/AII-ai-integrations.md`
// AII-FR-05, AII-FR-ZIEG, AII-FR-XGIM, AII-FR-UQLF, AII-FR-TWIR and AII-FR-FBVE,
// against `AAP-ai-api-integrations.md` AAP-FR-AYKU, AAP-FR-ITBL and AAP-FR-JQIJ.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAgentRegistry } from "../state/agentRegistry";
import { resetLogBufferForTest } from "../logging";
import { AI_ERRORS, type ModelOption } from "../types";
import { detectedValuesText, tokenLimitToCommit } from "./aiApiCustom";
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
  pick,
} from "../test/aiIntegrationsFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const commands = commandsFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

const OUTPUT = "set_ai_api_max_output_tokens";
const CONTEXT = "set_ai_api_max_context_size";
const GATEWAY = "http://localhost:11434";

const LOCAL_MODELS: ModelOption[] = [
  {
    id: "qwen3:32b",
    label: "qwen3:32b",
    detectedContextSize: 40960,
    detectedCapabilities: ["completion", "tools", "thinking"],
  },
  { id: "plain", label: "plain" },
];

/** The record the backend returns after a token value is stored. */
function storedCustom(over: Record<string, unknown>) {
  return apiVerified("custom", GATEWAY, { models: LOCAL_MODELS, ...over });
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  resetAgentRegistry();
});

afterEach(cleanup);

async function openCustom() {
  const user = userEvent.setup();
  await openApiTab(user, apiLevel(), /Custom/);
  await apiLevel().findByRole("combobox", { name: "Model" });
  return user;
}

describe("tokenLimitToCommit", () => {
  it.each([
    ["", null],
    ["  ", null],
    ["1", 1],
    [" 32768 ", 32768],
    ["4294967295", 4294967295],
    ["4294967296", undefined],
    ["0", undefined],
    ["-5", undefined],
    ["1.5", undefined],
    ["1e3", undefined],
    ["abc", undefined],
  ])("AII-FR-TWIR, AII-FR-UQLF: %j commits %j", (text, expected) => {
    expect(tokenLimitToCommit(text)).toBe(expected);
  });
});

describe("detected values (AII-FR-ZIEG)", () => {
  it("AII-FR-ZIEG: detectedValuesText names the context size and the capabilities, and nothing when both are absent", () => {
    expect(detectedValuesText(LOCAL_MODELS[0])).toBe(
      "detected · context 40960 tokens · completion, tools, thinking",
    );
    expect(detectedValuesText({ detectedContextSize: 8192, detectedCapabilities: null })).toBe(
      "detected · context 8192 tokens",
    );
    expect(detectedValuesText({ detectedContextSize: null, detectedCapabilities: [] })).toBeNull();
    expect(detectedValuesText(null)).toBeNull();
  });

  it("AII-FR-ZIEG, AAP-FR-JQIJ: the Custom tab shows the selected model's detected values", async () => {
    backend(customWith(LOCAL_MODELS, "qwen3:32b"));
    render(<BothLevels />);
    await openCustom();

    expect(apiLevel().getByTestId("ai-api-detected-values")).toHaveTextContent(
      "detected · context 40960 tokens · completion, tools, thinking",
    );
  });

  it("AII-FR-ZIEG: no line for a model without detected values, or with no model selected", async () => {
    backend(customWith(LOCAL_MODELS, "plain"));
    const { unmount } = render(<BothLevels />);
    await openCustom();
    expect(apiLevel().queryByTestId("ai-api-detected-values")).not.toBeInTheDocument();
    unmount();

    backend(customWith(LOCAL_MODELS, null));
    render(<BothLevels />);
    await openCustom();
    expect(apiLevel().queryByTestId("ai-api-detected-values")).not.toBeInTheDocument();
  });
});

describe("the Custom reasoning effort (AII-FR-XGIM, AAP-FR-AYKU)", () => {
  it("AII-FR-XGIM: choosing a level invokes set ai api reasoning at once, and the default entry sends none", async () => {
    let record = storedCustom({ selectedModel: null });
    backend(
      { api: [record, ...API_ALL.filter((p) => p !== "custom").map((p) => apiIntegration(p))] },
      {
        set_ai_api_reasoning: (args) => {
          record = storedCustom({ selectedReasoning: args!.choice ?? null });
          return record;
        },
      },
    );
    render(<BothLevels />);
    const user = await openCustom();

    expect(apiLevel().getByTestId("ai-api-reasoning-note")).toHaveTextContent(
      /default for every Custom model.*agent's own reasoning wins/i,
    );
    await pick(user, apiLevel(), "Reasoning effort", "none");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "custom",
        choice: { kind: "effort", effort: "none" },
      }),
    );
    await pick(user, apiLevel(), "Reasoning effort", "Model default");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({ provider: "custom", choice: null }),
    );
  });
});

describe("the Custom token fields (AII-FR-UQLF, AII-FR-TWIR, AII-FR-FBVE)", () => {
  it("AII-FR-05, AII-FR-UQLF: the Custom tab renders both fields between the reasoning effort and the turn timeout", async () => {
    backend(customWith(LOCAL_MODELS, null));
    render(<BothLevels />);
    await openCustom();

    const panel = apiLevel().getByTestId("ai-api-panel-custom");
    const order = Array.from(
      panel.querySelectorAll<HTMLElement>(
        "[data-testid='ai-api-reasoning'], [data-testid='ai-api-max-output-tokens'], [data-testid='ai-api-max-context-size'], [data-testid='ai-api-turn-timeout']",
      ),
    ).map((el) => el.dataset.testid);
    expect(order).toEqual([
      "ai-api-reasoning",
      "ai-api-max-output-tokens",
      "ai-api-max-context-size",
      "ai-api-turn-timeout",
    ]);
  });

  it("AII-FR-UQLF: no other tab renders the token fields", async () => {
    backend({ api: API_ALL.map((p) => apiVerified(p, "https://example.com")) });
    render(<BothLevels />);
    await apiLevel().findByTestId("ai-api-turn-timeout");

    expect(apiLevel().queryByTestId("ai-api-max-output-tokens")).not.toBeInTheDocument();
    expect(apiLevel().queryByTestId("ai-api-max-context-size")).not.toBeInTheDocument();
  });

  it("AII-FR-FBVE: the fields show their placeholder and their explanation lines", async () => {
    backend(customWith(LOCAL_MODELS, null));
    render(<BothLevels />);
    await openCustom();

    expect(apiLevel().getByTestId("ai-api-max-output-tokens")).toHaveAttribute(
      "placeholder",
      "128000",
    );
    expect(apiLevel().getByTestId("ai-api-max-output-tokens-note")).toHaveTextContent(
      /empty uses 128000/i,
    );
    expect(apiLevel().getByTestId("ai-api-max-context-size-note")).toHaveTextContent(
      /when the model reports no context size.*never sent to the host/i,
    );
  });

  it("AII-FR-UQLF, AAP-FR-ITBL: a valid entry is committed on blur, and an empty field commits a clear", async () => {
    let stored: Record<string, unknown> = {};
    backend(
      { api: customWith(LOCAL_MODELS, null).api },
      {
        [OUTPUT]: (args) => {
          stored = { ...stored, maxOutputTokens: args!.tokens };
          return storedCustom(stored);
        },
        [CONTEXT]: (args) => {
          stored = { ...stored, maxContextSize: args!.tokens };
          return storedCustom(stored);
        },
      },
    );
    render(<BothLevels />);
    const user = await openCustom();

    const output = apiLevel().getByTestId("ai-api-max-output-tokens") as HTMLInputElement;
    await user.type(output, "8192");
    expect(commands()).not.toContain(OUTPUT);
    await user.tab();
    await waitFor(() => expect(argsOf(OUTPUT)).toEqual({ provider: "custom", tokens: 8192 }));
    await waitFor(() => expect(output.value).toBe("8192"));

    const context = apiLevel().getByTestId("ai-api-max-context-size") as HTMLInputElement;
    await user.type(context, "32768{Enter}");
    await waitFor(() => expect(argsOf(CONTEXT)).toEqual({ provider: "custom", tokens: 32768 }));

    await user.clear(output);
    await user.tab();
    await waitFor(() => expect(argsOf(OUTPUT)).toEqual({ provider: "custom", tokens: null }));
  });

  it("AII-FR-TWIR: an invalid entry shows an inline message, quotes nothing, commits nothing, and blur restores the stored value", async () => {
    backend({ api: customWith(LOCAL_MODELS, null).api });
    render(<BothLevels />);
    const user = await openCustom();

    const output = apiLevel().getByTestId("ai-api-max-output-tokens") as HTMLInputElement;
    await user.type(output, "12abc");
    const message = apiLevel().getByTestId("ai-api-max-output-tokens-invalid");
    expect(message).toHaveTextContent(/whole number from 1 to 4294967295/i);
    expect(message).not.toHaveTextContent("12abc");
    expect(output).toHaveAttribute("aria-invalid", "true");

    await user.tab();
    expect(output.value).toBe("");
    expect(commands()).not.toContain(OUTPUT);
    expect(apiLevel().queryByTestId("ai-api-max-output-tokens-invalid")).not.toBeInTheDocument();
  });

  it("AII-FR-TWIR, AII-FR-28: a backend refusal keeps the stored value and renders inline", async () => {
    backend(
      { api: customWith(LOCAL_MODELS, null).api },
      { [CONTEXT]: () => Promise.reject(AI_ERRORS.tokenLimitInvalid) },
    );
    render(<BothLevels />);
    const user = await openCustom();

    const context = apiLevel().getByTestId("ai-api-max-context-size") as HTMLInputElement;
    await user.type(context, "100");
    await user.tab();

    expect(await apiLevel().findByTestId("ai-api-max-context-size-error")).toHaveTextContent(
      /whole number of tokens/i,
    );
    expect(context.value).toBe("");
  });
});
