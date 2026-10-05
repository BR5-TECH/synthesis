import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Settings } from "./Settings";
import type {
  AgenticIntegration,
  AgenticVendorId,
  AiApiIntegration,
  AiApiProviderId,
  ProjectAgenticIntegration,
  ProjectAiApiIntegration,
} from "../types";

// SET-FR-13 / SET-FR-14: the Project section names which agentic integration and
// which AI API integration this project uses, and how each resolved. The
// controls' content is owned by `AII-ai-integrations.md` (AII-FR-31..35); this
// file asserts the three claims `SET-project-settings.md` makes about them —
// that they are in the Project section, that each applies at once rather than
// through a section save, and that they are independent of one another.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function agenticIntegration(
  vendor: AgenticVendorId,
  displayName: string,
  state: AgenticIntegration["state"],
): AgenticIntegration {
  return {
    vendor,
    kind: "cli",
    displayName,
    binaryPath: state === "unconfigured" ? null : `/bin/${vendor}`,
    pathOrigin: state === "unconfigured" ? "unset" : "detected",
    baseUrl: null,
    keyState: "unset",
    maskedHint: null,
    keyRequired: false,
    state,
    version: state === "unconfigured" ? null : "1.0.0",
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    reasoningEfforts: [],
    selectedEffort: null,
    modelOverrides: {},
    effortOverrides: {},
    active: false,
  };
}

function apiIntegration(
  provider: AiApiProviderId,
  displayName: string,
  state: AiApiIntegration["state"],
): AiApiIntegration {
  return {
    provider,
    displayName,
    baseUrl: state === "unconfigured" ? null : `https://${provider}.example/v1`,
    keyState: state === "verified" ? "set" : "unset",
    maskedHint: state === "verified" ? "3f9a" : null,
    keyRequired: true,
    state,
    verifiedAt: null,
    models: [],
    modelsOrigin: "catalog",
    selectedModel: null,
    selectedReasoning: null,
    turnTimeoutMs: null,
    active: false,
  };
}

function backend(opts: {
  agenticResolved: ProjectAgenticIntegration;
  agenticList: AgenticIntegration[];
  apiResolved: ProjectAiApiIntegration;
  apiList: AiApiIntegration[];
  onSetAgentic?: (vendor: string | null) => ProjectAgenticIntegration;
  onSetApi?: (provider: string | null) => ProjectAiApiIntegration;
}) {
  let agentic = opts.agenticResolved;
  let api = opts.apiResolved;
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case "get_project_agentic_integration":
          return agentic;
        case "list_agentic_integrations":
          return opts.agenticList;
        case "set_project_agentic_integration":
          agentic = opts.onSetAgentic
            ? opts.onSetAgentic((args?.vendor as string) ?? null)
            : agentic;
          return agentic;
        case "get_project_ai_api_integration":
          return api;
        case "list_ai_api_integrations":
          return opts.apiList;
        case "set_project_ai_api_integration":
          api = opts.onSetApi
            ? opts.onSetApi((args?.provider as string) ?? null)
            : api;
          return api;
        // The Project section also reads the GitHub binding on mount.
        case "get_project_github_token_binding":
          return { tokenId: null, resolution: "none_stored" };
        case "list_github_tokens":
          return [];
        default:
          return undefined;
      }
    },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

/**
 * Make sure the Project section is the one presented.
 *
 * SET-FR-03: it is the window's first section and the one it opens on, so this
 * is ordinarily already true — it is asked for by its nav row rather than by
 * its text because the section's own heading reads "Project" too.
 */
async function openProjectSection() {
  await userEvent.click(screen.getByRole("tab", { name: "Project" }));
}

const noop = () => {};

const invoked = (cmd: string) =>
  invokeMock.mock.calls.some((c) => c[0] === cmd);

describe("the Project section's AI integration controls (SET-FR-13 / SET-FR-14)", () => {
  it("SET-FR-13 names the inherited agentic choice, then overrides it without a dirty state", async () => {
    backend({
      agenticResolved: {
        vendor: "claude_code",
        resolution: "inherited",
        overrideVendor: null,
      },
      agenticList: [
        agenticIntegration("claude_code", "Claude Code", "verified"),
        agenticIntegration("codex", "Codex", "verified"),
      ],
      apiResolved: {
        provider: null,
        resolution: "none_configured",
        overrideProvider: null,
      },
      apiList: [],
      onSetAgentic: (vendor) =>
        vendor
          ? {
              vendor: vendor as AgenticVendorId,
              resolution: "overridden",
              overrideVendor: vendor as AgenticVendorId,
            }
          : {
              vendor: "claude_code",
              resolution: "inherited",
              overrideVendor: null,
            },
    });
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={noop} />);
    await openProjectSection();

    // Read on mount, and named as inherited from the global choice.
    await waitFor(() =>
      expect(invoked("get_project_agentic_integration")).toBe(true),
    );
    const control = screen.getByTestId("settings-agentic-integration");
    expect(control).toHaveTextContent(/Claude Code/);
    expect(control).toHaveTextContent(/inherited/i);

    await userEvent.click(
      within(control).getByRole("button", { name: /change/i }),
    );
    await userEvent.selectOptions(
      within(control).getByLabelText("Agentic integration"),
      "codex",
    );

    await waitFor(() =>
      expect(
        screen.getByTestId("settings-agentic-integration"),
      ).toHaveTextContent(/overrides the global choice/i),
    );
    expect(
      invokeMock.mock.calls.find(
        (c) => c[0] === "set_project_agentic_integration",
      )?.[1],
    ).toEqual({ vendor: "codex" });
    // Applied at once: no section save stands between the choice and the write.
    expect(invoked("save_project_config")).toBe(false);
  });

  it("SET-FR-13, SET-FR-14 names the Global settings section when neither level has anything", async () => {
    backend({
      agenticResolved: {
        vendor: null,
        resolution: "none_configured",
        overrideVendor: null,
      },
      agenticList: [
        agenticIntegration("claude_code", "Claude Code", "unconfigured"),
        agenticIntegration("codex", "Codex", "unconfigured"),
      ],
      apiResolved: {
        provider: null,
        resolution: "none_configured",
        overrideProvider: null,
      },
      apiList: [apiIntegration("anthropic", "Anthropic", "unconfigured")],
    });
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={noop} />);
    await openProjectSection();

    // Each control names *its own* level's section (GLS-FR-16), not a generic
    // "AI integrations" that no longer exists in the nav — a pointer the author
    // cannot follow is worse than none.
    expect(
      await screen.findByTestId("settings-agentic-integration-none"),
    ).toHaveTextContent(/Global settings . Agentic AI/i);
    expect(
      await screen.findByTestId("settings-ai-api-integration-none"),
    ).toHaveTextContent(/Global settings . AI API/i);

    // Nothing to choose between, so neither control offers an action.
    expect(
      screen.queryByRole("button", { name: /change/i }),
    ).not.toBeInTheDocument();
  });

  it("SET-FR-14 renders the two controls independently and overrides one alone", async () => {
    // SET-FR-14: the levels are chosen independently, so overriding the API one
    // must leave the agentic control's resolution untouched.
    backend({
      agenticResolved: {
        vendor: "claude_code",
        resolution: "inherited",
        overrideVendor: null,
      },
      agenticList: [agenticIntegration("claude_code", "Claude Code", "verified")],
      apiResolved: {
        provider: "openrouter",
        resolution: "inherited",
        overrideProvider: null,
      },
      apiList: [
        apiIntegration("openrouter", "OpenRouter", "verified"),
        apiIntegration("anthropic", "Anthropic", "verified"),
      ],
      onSetApi: (provider) => ({
        provider: provider as AiApiProviderId,
        resolution: "overridden",
        overrideProvider: provider as AiApiProviderId,
      }),
    });
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={noop} />);
    await openProjectSection();

    await waitFor(() =>
      expect(invoked("get_project_ai_api_integration")).toBe(true),
    );
    expect(invoked("get_project_agentic_integration")).toBe(true);

    const apiControl = screen.getByTestId("settings-ai-api-integration");
    await userEvent.click(
      within(apiControl).getByRole("button", { name: /change/i }),
    );
    await userEvent.selectOptions(
      within(apiControl).getByLabelText("AI API integration"),
      "anthropic",
    );

    await waitFor(() =>
      expect(
        screen.getByTestId("settings-ai-api-integration-resolution"),
      ).toHaveTextContent(/Anthropic · overrides the global choice/),
    );
    // The agentic control is exactly where it was, and was never written to.
    expect(
      screen.getByTestId("settings-agentic-integration-resolution"),
    ).toHaveTextContent(/Claude Code · inherited/);
    expect(invoked("set_project_agentic_integration")).toBe(false);
    expect(invoked("save_project_config")).toBe(false);
  });

  it("renders both controls without running a binary or reaching an endpoint (SET non-functional)", async () => {
    backend({
      agenticResolved: {
        vendor: "codex",
        resolution: "overridden",
        overrideVendor: "codex",
      },
      agenticList: [agenticIntegration("codex", "Codex", "verified")],
      apiResolved: {
        provider: "anthropic",
        resolution: "overridden",
        overrideProvider: "anthropic",
      },
      apiList: [apiIntegration("anthropic", "Anthropic", "verified")],
    });
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={noop} />);
    await openProjectSection();

    await screen.findByTestId("settings-agentic-integration-resolution");
    await screen.findByTestId("settings-ai-api-integration-resolution");
    for (const forbidden of [
      "verify_agentic_integration",
      "detect_agentic_cli_binary",
      "verify_ai_api_integration",
    ]) {
      expect(invoked(forbidden)).toBe(false);
    }
  });

  it("renders neither an endpoint, a key hint, nor a path (SET non-functional)", async () => {
    // The controls describe what the project resolves to and nothing more.
    backend({
      agenticResolved: {
        vendor: "codex",
        resolution: "overridden",
        overrideVendor: "codex",
      },
      agenticList: [agenticIntegration("codex", "Codex", "verified")],
      apiResolved: {
        provider: "anthropic",
        resolution: "inherited",
        overrideProvider: null,
      },
      apiList: [apiIntegration("anthropic", "Anthropic", "verified")],
    });
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={noop} />);
    await openProjectSection();
    await screen.findByTestId("settings-ai-api-integration-resolution");

    const html = document.body.innerHTML;
    expect(html).not.toContain("/bin/codex");
    expect(html).not.toContain("anthropic.example/v1");
    expect(html).not.toContain("3f9a");
  });
});
