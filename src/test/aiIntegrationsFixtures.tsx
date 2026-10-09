/**
 * The shapes, harness and helpers the AI integrations tests build from
 * (`../../specifications/core/AII-ai-integrations.md`).
 *
 * Shared rather than copied, on the model of `streamFixtures.ts`: the section's
 * tests live in more than one file, and a fixture that drifted between them
 * would let one of them pass against a record the backend never sends.
 *
 * `vi.mock` is file-scoped, so each test file keeps its own `invokeMock` and
 * its own mock of the Tauri bridge. `backendFor`, `commandsFor` and `argsOfFor`
 * take that mock and give back the helpers the tests call.
 */

import { screen, within } from "@testing-library/react";
import type userEvent from "@testing-library/user-event";
import type { Mock } from "vitest";

import { AgenticAiIntegrations, AiApiIntegrations } from "../components/AiIntegrations";
import type {
  AgenticIntegration,
  AgenticVendorId,
  AiApiIntegration,
  AiApiProviderId,
  ModelOption,
  ProjectAgenticIntegration,
  ProjectAiApiIntegration,
} from "../types";

/**
 * Drive a filterable model/reasoning selector (AII-FR-36): open the panel, then
 * click the option. `selectOptions` cannot reach it — it is a combobox the
 * section renders itself, not a native `<select>`.
 */
export async function pick(
  user: ReturnType<typeof userEvent.setup>,
  scope: ReturnType<typeof within>,
  label: string,
  option: string | RegExp,
) {
  await user.click(scope.getByRole("combobox", { name: label }));
  // Scoped to this selector's own listbox: the level renders more than one
  // control carrying options, and an unscoped query would reach into the
  // neighbouring one.
  const list = within(scope.getByRole("listbox", { name: label }));
  await user.click(list.getByRole("option", { name: option }));
}

/**
 * Open a named tab of the AI API level. OpenRouter is the level's first tab
 * (AII-FR-03), so a test driving any other provider says which one it means.
 */
export async function openApiTab(
  user: ReturnType<typeof userEvent.setup>,
  level: ReturnType<typeof within>,
  name: RegExp,
) {
  await user.click(await level.findByRole("tab", { name }));
}

/** The options a filterable selector is offering right now, in order. */
export async function optionsOf(
  user: ReturnType<typeof userEvent.setup>,
  scope: ReturnType<typeof within>,
  label: string,
): Promise<string[]> {
  await user.click(scope.getByRole("combobox", { name: label }));
  return within(scope.getByRole("listbox", { name: label }))
    .getAllByRole("option")
    .map((o) => o.textContent ?? "");
}

// --- Fixtures --------------------------------------------------------------

export const API_NAMES: Record<AiApiProviderId, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  openrouter: "OpenRouter",
  custom: "Custom",
};
export const API_ALL: AiApiProviderId[] = ["anthropic", "openai", "openrouter", "custom"];

export function apiIntegration(
  provider: AiApiProviderId,
  over: Partial<AiApiIntegration> = {},
): AiApiIntegration {
  return {
    provider,
    displayName: API_NAMES[provider],
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: true,
    state: "unconfigured",
    verifiedAt: null,
    models: [
      { id: "model-a", label: "Model A" },
      { id: "model-b", label: "Model B" },
    ],
    modelsOrigin: "catalog",
    selectedModel: null,
    selectedReasoning: null,
    turnTimeoutMs: null,
    active: false,
    ...over,
  };
}

export function apiVerified(
  provider: AiApiProviderId,
  baseUrl: string,
  over: Partial<AiApiIntegration> = {},
): AiApiIntegration {
  return apiIntegration(provider, {
    baseUrl,
    keyState: "set",
    maskedHint: "3f9a",
    state: "verified",
    verifiedAt: "2026-07-30T14:02:00Z",
    modelsOrigin: "probed",
    ...over,
  });
}

export const AGENTIC_NAMES: Record<AgenticVendorId, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  claude_agent_api: "Claude Agent API",
  custom_agent_api: "Custom agent API",
};
export const AGENTIC_ALL: AgenticVendorId[] = [
  "claude_code",
  "codex",
  "opencode",
  "claude_agent_api",
  "custom_agent_api",
];
export const CLI_VENDORS: AgenticVendorId[] = ["claude_code", "codex", "opencode"];

/** A well-formed OAuth token (AII-FR-50). It is a fake value, not a real credential. */
export const SAMPLE_TOKEN =
  "sk-ant-oat01-FAKE-TEST-TOKEN-NOT-A-REAL-CREDENTIAL-000000000000000000000000000000000000000-ygAA";

export function agenticIntegration(
  vendor: AgenticVendorId,
  over: Partial<AgenticIntegration> = {},
): AgenticIntegration {
  const kind: AgenticIntegration["kind"] = CLI_VENDORS.includes(vendor)
    ? "cli"
    : "api";
  return {
    vendor,
    kind,
    displayName: AGENTIC_NAMES[vendor],
    binaryPath: null,
    pathOrigin: "unset",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    // AIC-FR-25: the credential fields follow the credential, not the kind.
    // Claude Code holds an OAuth token; Codex and OpenCode hold nothing.
    keyRequired: vendor === "claude_agent_api" || vendor === "claude_code",
    state: "unconfigured",
    version: null,
    verifiedAt: null,
    models: [
      { id: "opus", label: "Opus" },
      { id: "sonnet", label: "Sonnet" },
    ],
    modelsOrigin: "catalog",
    selectedModel: null,
    modelOverrides: {},
    reasoningEfforts: [
      { id: "low", label: "Low" },
      { id: "high", label: "High" },
    ],
    selectedEffort: null,
    effortOverrides: {},
    active: false,
    ...over,
  };
}

/**
 * A verified CLI record as the backend would return it — which for Claude Code
 * means a stored OAuth token described by its masked hint, because that vendor
 * cannot reach `verified` without one (AIC-FR-26).
 */
export function cliVerified(
  vendor: AgenticVendorId,
  path: string,
  over: Partial<AgenticIntegration> = {},
): AgenticIntegration {
  const holdsToken = vendor === "claude_code";
  return agenticIntegration(vendor, {
    binaryPath: path,
    pathOrigin: "detected",
    state: "verified",
    version: "2.1.4",
    verifiedAt: "2026-07-01T09:00:00Z",
    ...(holdsToken ? { keyState: "set" as const, maskedHint: "ygAA" } : {}),
    ...over,
  });
}

export function apiAgentVerified(
  vendor: AgenticVendorId,
  baseUrl: string,
  over: Partial<AgenticIntegration> = {},
): AgenticIntegration {
  return agenticIntegration(vendor, {
    baseUrl,
    keyState: "set",
    maskedHint: "a71c",
    state: "verified",
    verifiedAt: "2026-07-30T14:02:00Z",
    ...over,
  });
}

/**
 * Every command either level may call, wired to two lists plus per-command
 * overrides. A command with no override and no default throws, so a test that
 * silently reaches for something unexpected fails rather than passing on
 * `undefined`.
 */
export function backendFor(invokeMock: Mock) {
  return function backend(
    opts: {
      api?: AiApiIntegration[];
      agentic?: AgenticIntegration[];
      project?: ProjectAgenticIntegration;
      projectApi?: ProjectAiApiIntegration;
    } = {},
    overrides: Record<string, (args?: Record<string, unknown>) => unknown> = {},
  ) {
    const api = opts.api ?? API_ALL.map((p) => apiIntegration(p));
    const agentic = opts.agentic ?? AGENTIC_ALL.map((v) => agenticIntegration(v));
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (overrides[cmd]) return overrides[cmd](args);
        switch (cmd) {
          // The log flush the section's own emitters start. Answered, not
          // rejected, so no test depends on a swallowed rejection.
          case "append_log_records":
            return undefined;
          case "list_ai_api_integrations":
            return api;
          case "list_agentic_integrations":
            return agentic;
          case "detect_agentic_cli_binary":
            return { path: null };
          case "get_project_agentic_integration":
            return (
              opts.project ?? {
                vendor: null,
                resolution: "none_configured",
                overrideVendor: null,
              }
            );
          case "get_project_ai_api_integration":
            return (
              opts.projectApi ?? {
                provider: null,
                resolution: "none_configured",
                overrideProvider: null,
              }
            );
          default:
            throw new Error(`unexpected command ${cmd}`);
        }
      },
    );
  };
}

/** The commands invoked so far, in order. */
export const commandsFor = (invokeMock: Mock) => () =>
  invokeMock.mock.calls.map((c) => c[0] as string);

/** The arguments of the last call to `cmd`. */
export const argsOfFor = (invokeMock: Mock) => (cmd: string) => {
  const calls = invokeMock.mock.calls.filter((c) => c[0] === cmd);
  return calls[calls.length - 1]?.[1] as Record<string, unknown> | undefined;
};

export const apiLevel = () => within(screen.getByTestId("ai-api-level"));
export const agenticLevel = () => within(screen.getByTestId("agentic-level"));

/**
 * Both levels mounted at once.
 *
 * In Global settings each level is its own navigable section and only the
 * selected one is mounted (GLS-FR-16, asserted in `GlobalSettings.test.tsx`).
 * Mounting them together here is deliberately the *stricter* arrangement: it is
 * what lets a test prove that a URL typed into one level reaches nothing in the
 * other, and that activating in one leaves the other's choice alone (AII-FR-02,
 * AII-FR-04) — claims that would pass vacuously if only one were ever on screen.
 */
export function BothLevels() {
  return (
    <>
      <AiApiIntegrations />
      <AgenticAiIntegrations />
    </>
  );
}

/** A provider carrying enough models that scrolling is not an option. */
export function manyModels(count: number): ModelOption[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `vendor/model-${i}`,
    label: `Vendor Model ${i}`,
  }));
}

/** A model declaring its own ladder. */
export function withLadder(
  id: string,
  efforts: string[],
  over: Partial<NonNullable<ModelOption["reasoning"]>> = {},
): ModelOption {
  return {
    id,
    label: id,
    reasoning: {
      mandatory: false,
      defaultEnabled: true,
      supportedEfforts: efforts,
      defaultEffort: efforts[0],
      ...over,
    },
  };
}

/** OpenRouter, verified, with the given catalogue and selection. */
export function openRouterWith(models: ModelOption[], selectedModel: string | null) {
  return {
    api: [
      apiVerified("openrouter", "https://openrouter.ai/api/v1", {
        models,
        selectedModel,
      }),
      ...API_ALL.filter((p) => p !== "openrouter").map((p) => apiIntegration(p)),
    ],
  };
}

/** Custom, verified, with the given gateway models and selection (AAP-FR-RTMZ). */
export function customWith(models: ModelOption[], selectedModel: string | null) {
  return {
    api: [
      apiVerified("custom", "https://llm-gateway.example.com", {
        models,
        selectedModel,
      }),
      ...API_ALL.filter((p) => p !== "custom").map((p) => apiIntegration(p)),
    ],
  };
}
