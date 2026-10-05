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

import { type AgenticIntegration, type AgenticVendorId, AI_ERRORS } from "../types";
import {
  AGENTIC_ALL,
  AGENTIC_NAMES,
  BothLevels,
  CLI_VENDORS,
  agenticIntegration,
  agenticLevel,
  apiAgentVerified,
  argsOfFor,
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
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The Agentic level
// ---------------------------------------------------------------------------

describe("the Agentic level", () => {
  it("renders each kind's own fields in one strip (AII-FR-16, AII-FR-49)", async () => {
    // AII-FR-16 / AIC-FR-25: a CLI tab has a binary field and no endpoint
    // fields; an API tab has the reverse. Everything downstream is common.
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await agenticLevel().findByRole("tab", { name: /Claude Code/ });

    for (const vendor of AGENTIC_ALL) {
      await user.click(
        agenticLevel().getByRole("tab", {
          name: new RegExp(AGENTIC_NAMES[vendor]),
        }),
      );
      const panel = await screen.findByTestId(`agentic-panel-${vendor}`);

      if (CLI_VENDORS.includes(vendor)) {
        expect(within(panel).getByLabelText("Binary")).toBeInTheDocument();
        expect(
          within(panel).queryByLabelText("Base URL"),
        ).not.toBeInTheDocument();
        expect(
          within(panel).queryByLabelText("API key"),
        ).not.toBeInTheDocument();
      } else {
        expect(within(panel).getByLabelText("Base URL")).toBeInTheDocument();
        expect(within(panel).getByLabelText("API key")).toBeInTheDocument();
        expect(within(panel).queryByLabelText("Binary")).not.toBeInTheDocument();
      }

      // The common rows are in the same order whichever kind it is.
      const rows = [
        within(panel).getByTestId("agentic-status"),
        within(panel).getByRole("combobox", { name: "Model" }),
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

  it("detects a binary for a CLI tab with no stored path (AII-FR-17)", async () => {
    // AII-FR-17.
    backend(
      {},
      { detect_agentic_cli_binary: () => ({ path: "/opt/homebrew/bin/claude" }) },
    );
    render(<BothLevels />);

    await waitFor(() =>
      expect(agenticLevel().getByLabelText("Binary")).toHaveValue(
        "/opt/homebrew/bin/claude",
      ),
    );
    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /Detected — verify this path before use/i,
      ),
    );
  });

  it("says so when detection finds nothing (AII-FR-17)", async () => {
    backend();
    render(<BothLevels />);
    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /No binary found/i,
      ),
    );
    expect(agenticLevel().getByLabelText("Binary")).toHaveValue("");
  });

  it("never asks an API tab to detect a binary (AII-FR-17)", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await user.click(
      await agenticLevel().findByRole("tab", { name: /Claude Agent API/ }),
    );
    await screen.findByTestId("agentic-panel-claude_agent_api");
    const detections = invokeMock.mock.calls.filter(
      (c) => c[0] === "detect_agentic_cli_binary",
    );
    expect(detections.every((c) => c[1]?.vendor !== "claude_agent_api")).toBe(
      true,
    );
  });

  it("discards an abandoned candidate when the tab is left (AII-FR-18, AII-FR-21, AII-FR-22)", async () => {
    // AII-FR-18 / FR-21 / FR-22.
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.type(
      await agenticLevel().findByLabelText("Binary"),
      "/my/own/claude",
    );

    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));

    expect(await agenticLevel().findByLabelText("Binary")).toHaveValue("");
    expect(commands()).not.toContain("verify_agentic_integration");
  });

  it("verifies an API-kind agent with the fields of its own kind (AII-FR-19, AII-FR-20)", async () => {
    // AII-FR-19 / FR-20 / AIC-FR-25: the config carries a base URL and a key,
    // and no path.
    backend(
      {},
      {
        verify_agentic_integration: (args) =>
          apiAgentVerified(
            args?.vendor as AgenticVendorId,
            (args?.config as { baseUrl: string }).baseUrl,
          ),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await agenticLevel().findByRole("tab", { name: /Custom agent API/ }),
    );
    // AII-FR-19: Custom agent API ships no default URL.
    const url = await agenticLevel().findByLabelText("Base URL");
    expect(url).toHaveValue("");

    await user.type(url, "https://self-hosted.internal/v1");
    await user.type(agenticLevel().getByLabelText("API key"), "sk-agent-a71c");
    await user.click(agenticLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /verified/i,
      ),
    );
    expect(argsOf("verify_agentic_integration")).toEqual({
      vendor: "custom_agent_api",
      config: {
        baseUrl: "https://self-hosted.internal/v1",
        apiKey: "sk-agent-a71c",
      },
    });
    // AII-FR-30: the key is gone from the field and from the page.
    expect(agenticLevel().getByLabelText("API key")).toHaveValue("");
    expect(document.body.innerHTML).not.toContain("sk-agent-a71c");
  });

  it("prefills the named API agent's URL (AII-FR-19, AII-FR-20)", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();
    await user.click(
      await agenticLevel().findByRole("tab", { name: /Claude Agent API/ }),
    );
    expect(await agenticLevel().findByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );
  });

  it("commits a CLI path on a successful verification (AII-FR-20, AII-FR-21, AII-FR-49)", async () => {
    // AII-FR-20 / FR-21 / FR-49. Codex rather than Claude Code, because the
    // payload under test is the credential-free one: a path and nothing else,
    // with no token value of any kind riding along.
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
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(await agenticLevel().findByRole("tab", { name: /Codex/ }));
    await user.type(
      await agenticLevel().findByLabelText("Binary"),
      "/usr/bin/codex",
    );
    await user.click(agenticLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /verified · 2\.1\.4/,
      ),
    );
    expect(argsOf("verify_agentic_integration")).toEqual({
      vendor: "codex",
      config: { path: "/usr/bin/codex" },
    });
  });

  it("names a CLI failure on the status line and leaves the stored path (AII-FR-20, AII-FR-21)", async () => {
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/usr/bin/claude"),
          ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
        ],
      },
      {
        verify_agentic_integration: () => {
          throw AI_ERRORS.notTheExpectedCli;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    const field = await agenticLevel().findByLabelText("Binary");
    await user.clear(field);
    await user.type(field, "/bin/ls");
    await user.click(agenticLevel().getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
        /not this vendor's CLI/i,
      ),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // The stored path is exactly what it was: a failed verification commits
    // nothing, so leaving the tab and returning shows it again.
    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    expect(await agenticLevel().findByLabelText("Binary")).toHaveValue(
      "/usr/bin/claude",
    );
  });

  it("clears the displayed version when a verified tab is edited (AII-FR-22)", async () => {
    // AII-FR-22: the version belongs to the configuration that verified. Leaving
    // it on screen beside an edited path would claim something untrue of what is
    // shown — and the edit itself must invoke nothing.
    backend({
      agentic: [
        cliVerified("claude_code", "/usr/bin/claude"),
        ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    const status = await agenticLevel().findByTestId("agentic-status");
    expect(status).toHaveTextContent(/verified · 2\.1\.4/);

    const before = commands().length;
    await user.type(agenticLevel().getByLabelText("Binary"), "-x");
    expect(agenticLevel().getByTestId("agentic-status")).toHaveTextContent(
      /Not verified/i,
    );
    expect(agenticLevel().getByTestId("agentic-status")).not.toHaveTextContent(
      "2.1.4",
    );
    expect(commands().length).toBe(before);
  });

});

describe("the Agentic level", () => {
  it("hands the single active marker across kinds (AII-FR-25)", async () => {
    // AII-FR-25: one active choice spans CLI and API vendors alike.
    let list: AgenticIntegration[] = [
      cliVerified("claude_code", "/usr/bin/claude", { active: true }),
      agenticIntegration("codex"),
      agenticIntegration("opencode"),
      apiAgentVerified("claude_agent_api", "https://api.anthropic.com/v1"),
      agenticIntegration("custom_agent_api"),
    ];
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        detect_agentic_cli_binary: () => ({ path: null }),
        set_active_agentic_integration: (args) => {
          list = list.map((i) => ({ ...i, active: i.vendor === args?.vendor }));
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(
      await agenticLevel().findByTestId("agentic-active-marker"),
    ).toBeInTheDocument();

    await user.click(
      agenticLevel().getByRole("tab", { name: /Claude Agent API/ }),
    );
    await user.click(
      await agenticLevel().findByRole("button", {
        name: /use this integration/i,
      }),
    );
    await waitFor(() =>
      expect(
        agenticLevel().getByTestId("agentic-active-marker"),
      ).toBeInTheDocument(),
    );

    // The CLI tab offers its activation control again.
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    expect(
      await agenticLevel().findByRole("button", {
        name: /use this integration/i,
      }),
    ).toBeEnabled();
  });

  it("does not re-detect a path the author just cleared (AII-FR-26, AII-FR-17, AII-FR-29)", async () => {
    // AII-FR-26: re-detecting would immediately refill the field the author just
    // emptied, undoing the action they took.
    let list = [
      cliVerified("claude_code", "/usr/bin/claude"),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ];
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        detect_agentic_cli_binary: () => ({ path: "/usr/bin/claude" }),
        clear_agentic_integration: () => {
          list = AGENTIC_ALL.map((v) => agenticIntegration(v));
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await agenticLevel().findByRole("button", { name: /clear/i }),
    );
    await waitFor(() =>
      expect(agenticLevel().getByTestId("agentic-empty")).toBeInTheDocument(),
    );
    expect(agenticLevel().getByLabelText("Binary")).toHaveValue("");
    expect(commands()).not.toContain("detect_agentic_cli_binary");
  });

  it("reports each kind's degradation while keeping its configuration (AII-FR-27)", async () => {
    // AII-FR-27.
    backend({
      agentic: [
        cliVerified("claude_code", "/usr/bin/claude", { state: "missing" }),
        agenticIntegration("codex"),
        agenticIntegration("opencode"),
        apiAgentVerified("claude_agent_api", "https://api.anthropic.com/v1", {
          state: "key_unavailable",
          keyState: "unavailable",
        }),
        agenticIntegration("custom_agent_api"),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(await agenticLevel().findByTestId("agentic-status")).toHaveTextContent(
      /no longer at this path/i,
    );
    expect(agenticLevel().getByLabelText("Binary")).toHaveValue(
      "/usr/bin/claude",
    );
    expect(
      agenticLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeDisabled();

    await user.click(
      agenticLevel().getByRole("tab", { name: /Claude Agent API/ }),
    );
    expect(await agenticLevel().findByTestId("agentic-status")).toHaveTextContent(
      /can no longer be read/i,
    );
    expect(agenticLevel().getByLabelText("Base URL")).toHaveValue(
      "https://api.anthropic.com/v1",
    );
    expect(
      agenticLevel().getByRole("button", { name: /use this integration/i }),
    ).toBeDisabled();
  });

  it("fills the binary field from the file picker, and leaves it alone when dismissed (AII-FR-18)", async () => {
    backend(
      {},
      { browse_for_file: () => ({ selected: { path: "/picked/claude" } }) },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await agenticLevel().findByRole("button", { name: /browse/i }),
    );
    await waitFor(() =>
      expect(agenticLevel().getByLabelText("Binary")).toHaveValue(
        "/picked/claude",
      ),
    );

    backend({}, { browse_for_file: () => "cancelled" });
    await user.click(agenticLevel().getByRole("button", { name: /browse/i }));
    expect(agenticLevel().getByLabelText("Binary")).toHaveValue("/picked/claude");
  });
});
