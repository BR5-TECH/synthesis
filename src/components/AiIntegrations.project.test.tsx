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

import { ProjectAiIntegrations } from "./AiIntegrations";
import {
  AGENTIC_ALL,
  API_ALL,
  agenticIntegration,
  apiAgentVerified,
  apiIntegration,
  apiVerified,
  backendFor,
  cliVerified,
  commandsFor,
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
// The Project settings controls
// ---------------------------------------------------------------------------

describe("the project AI integration controls", () => {
  it("names both resolved integrations and how each resolved (AII-FR-31)", async () => {
    // AII-FR-31.
    backend({
      agentic: [cliVerified("claude_code", "/bin/claude", { active: true })],
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          active: true,
        }),
      ],
      project: {
        vendor: "claude_code",
        resolution: "inherited",
        overrideVendor: null,
      },
      projectApi: {
        provider: "openrouter",
        resolution: "inherited",
        overrideProvider: null,
      },
    });
    render(<ProjectAiIntegrations />);

    expect(
      await screen.findByTestId("settings-agentic-integration-resolution"),
    ).toHaveTextContent(/Claude Code · inherited from your global choice/);
    expect(
      await screen.findByTestId("settings-ai-api-integration-resolution"),
    ).toHaveTextContent(/OpenRouter · inherited from your global choice/);

    expect(commands()).toContain("get_project_agentic_integration");
    expect(commands()).toContain("get_project_ai_api_integration");
  });

  it("offers only its own level's verified integrations, and overrides one alone (AII-FR-32)", async () => {
    // AII-FR-32: the two controls are independent.
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/bin/claude", { active: true }),
          cliVerified("codex", "/bin/codex"),
          agenticIntegration("opencode"),
        ],
        api: [
          apiVerified("openrouter", "https://openrouter.ai/api/v1", {
            active: true,
          }),
        ],
        project: {
          vendor: "claude_code",
          resolution: "inherited",
          overrideVendor: null,
        },
        projectApi: {
          provider: "openrouter",
          resolution: "inherited",
          overrideProvider: null,
        },
      },
      {
        set_project_agentic_integration: (args) => ({
          vendor: args?.vendor,
          resolution: "overridden",
          overrideVendor: args?.vendor,
        }),
      },
    );
    render(<ProjectAiIntegrations />);
    const user = userEvent.setup();

    const control = within(
      await screen.findByTestId("settings-agentic-integration"),
    );
    await user.click(control.getByRole("button", { name: /change/i }));
    const select = control.getByLabelText("Agentic integration");
    // Only verified agentic integrations, plus the inherit entry. No provider.
    expect(
      within(select)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Inherit the global choice", "Claude Code", "Codex"]);

    await user.selectOptions(select, "codex");
    await waitFor(() =>
      expect(
        control.getByTestId("settings-agentic-integration-resolution"),
      ).toHaveTextContent(/Codex · overrides the global choice/),
    );
    // The API control is untouched.
    expect(
      screen.getByTestId("settings-ai-api-integration-resolution"),
    ).toHaveTextContent(/OpenRouter · inherited/);
    expect(commands()).not.toContain("set_project_ai_api_integration");
  });

  it("clears an override with the inherit entry (AII-FR-32)", async () => {
    // AII-FR-32.
    backend(
      {
        api: [
          apiVerified("anthropic", "https://api.anthropic.com/v1"),
          apiVerified("openai", "https://api.openai.com/v1", { active: true }),
        ],
        projectApi: {
          provider: "anthropic",
          resolution: "overridden",
          overrideProvider: "anthropic",
        },
      },
      {
        set_project_ai_api_integration: (args) => {
          expect(args?.provider).toBeNull();
          return {
            provider: "openai",
            resolution: "inherited",
            overrideProvider: null,
          };
        },
      },
    );
    render(<ProjectAiIntegrations />);
    const user = userEvent.setup();

    const control = within(
      await screen.findByTestId("settings-ai-api-integration"),
    );
    await user.click(control.getByRole("button", { name: /change/i }));
    await user.selectOptions(control.getByLabelText("AI API integration"), "");

    await waitFor(() =>
      expect(
        control.getByTestId("settings-ai-api-integration-resolution"),
      ).toHaveTextContent(/OpenAI · inherited from your global choice/),
    );
  });

  it("names a dead override as dead and says what is in effect (AII-FR-33)", async () => {
    // AII-FR-33: presenting it as the project's choice would be a lie the author
    // acts on.
    backend({
      agentic: [cliVerified("claude_code", "/bin/claude", { active: true })],
      project: {
        vendor: "claude_code",
        resolution: "override_unavailable",
        overrideVendor: "codex",
      },
    });
    render(<ProjectAiIntegrations />);

    const line = await screen.findByTestId(
      "settings-agentic-integration-resolution",
    );
    expect(line).toHaveTextContent(
      /codex was chosen for this project but is no longer available/i,
    );
    expect(line).toHaveTextContent(/Claude Code is in effect instead/);
  });

  it("names where to configure one when a level has nothing (AII-FR-31, AII-FR-34)", async () => {
    // AII-FR-34: and the other control carries on naming its own resolution.
    backend({
      agentic: [cliVerified("claude_code", "/bin/claude", { active: true })],
      api: API_ALL.map((p) => apiIntegration(p)),
      project: {
        vendor: "claude_code",
        resolution: "inherited",
        overrideVendor: null,
      },
      projectApi: {
        provider: null,
        resolution: "none_configured",
        overrideProvider: null,
      },
    });
    render(<ProjectAiIntegrations />);

    expect(
      await screen.findByTestId("settings-ai-api-integration-none"),
    ).toHaveTextContent(/Set one up in Global settings → AI API/i);
    expect(
      screen.getByTestId("settings-agentic-integration-resolution"),
    ).toHaveTextContent(/Claude Code/);
  });

  it("names each level's own section when something is configured but nothing resolves (AII-FR-31)", async () => {
    // The `none_selected` branch — configured but nothing active — carries the
    // same section pointer as `none_configured` and had no test, so a stale
    // "AI integrations" left behind there would ship invisibly. Both branches
    // are asserted because both name a section, and they name different ones.
    backend({
      agentic: AGENTIC_ALL.map((v) => agenticIntegration(v)),
      api: API_ALL.map((p) => apiIntegration(p)),
      project: {
        vendor: null,
        resolution: "none_selected",
        overrideVendor: null,
      },
      projectApi: {
        provider: null,
        resolution: "none_selected",
        overrideProvider: null,
      },
    });
    render(<ProjectAiIntegrations />);

    expect(
      await screen.findByTestId("settings-agentic-integration-resolution"),
    ).toHaveTextContent(/activate one in Global settings → Agentic AI/i);
    expect(
      screen.getByTestId("settings-ai-api-integration-resolution"),
    ).toHaveTextContent(/activate one in Global settings → AI API/i);
  });

  it("renders no endpoint, key, path, model list, or verification state (AII-FR-35)", async () => {
    // AII-FR-35: each control is one line naming the resolution, plus one action.
    backend({
      agentic: [
        apiAgentVerified("claude_agent_api", "https://agent.example/v1", {
          active: true,
        }),
      ],
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          active: true,
        }),
      ],
      project: {
        vendor: "claude_agent_api",
        resolution: "inherited",
        overrideVendor: null,
      },
      projectApi: {
        provider: "openrouter",
        resolution: "inherited",
        overrideProvider: null,
      },
    });
    render(<ProjectAiIntegrations />);
    await screen.findByTestId("settings-agentic-integration-resolution");

    const html = document.body.innerHTML;
    // Both levels probed: an endpoint and a masked hint from each.
    expect(html).not.toContain("agent.example/v1");
    expect(html).not.toContain("a71c");
    expect(html).not.toContain("openrouter.ai/api/v1");
    expect(html).not.toContain("3f9a");
    expect(
      screen.queryByRole("combobox", { name: "Model" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-api-status")).not.toBeInTheDocument();
  });

  it("offers nothing to change until something verifies", async () => {
    backend({
      project: {
        vendor: null,
        resolution: "none_configured",
        overrideVendor: null,
      },
      projectApi: {
        provider: null,
        resolution: "none_configured",
        overrideProvider: null,
      },
    });
    render(<ProjectAiIntegrations />);
    await screen.findByTestId("settings-agentic-integration-none");
    expect(
      screen.queryByRole("button", { name: /change/i }),
    ).not.toBeInTheDocument();
  });
});
