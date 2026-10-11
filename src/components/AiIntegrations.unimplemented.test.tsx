import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { flushLogs, resetLogBufferForTest } from "../logging";
import type { LogInput } from "../types";
import { ProjectAiIntegrations } from "./AiIntegrations";
import {
  AGENTIC_ALL,
  BothLevels,
  agenticIntegration,
  agenticLevel,
  apiAgentVerified,
  argsOfFor,
  backendFor,
  cliVerified,
  commandsFor,
} from "../test/aiIntegrationsFixtures";

// AII-FR-OTQY: only Claude Code is implemented. Every other vendor keeps its
// tab, but the tab is read-only and the section invokes nothing for it. These
// tests use the real gate and never open it.
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const commands = commandsFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

/** Every control a panel holds that the author could operate. */
function operableControls(panel: HTMLElement): HTMLElement[] {
  return Array.from(
    panel.querySelectorAll<HTMLElement>("input, select, textarea, button"),
  );
}

/** The log records the section emitted, after the pending batch is written. */
function loggedRecords(): LogInput[] {
  flushLogs();
  return invokeMock.mock.calls
    .filter((c) => c[0] === "append_log_records")
    .flatMap((c) => (c[1] as { records: LogInput[] }).records);
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
});

afterEach(cleanup);

describe("an agentic vendor that is not implemented", () => {
  it("AII-FR-OTQY, AII-FR-16, AII-FR-17, AII-FR-20: a CLI tab is selectable, states it, and is read-only", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();

    const codexTab = await agenticLevel().findByRole("tab", { name: /Codex/ });
    await user.click(codexTab);

    // The tab is selected, and its label is marked as not implemented.
    expect(codexTab).toHaveAttribute("aria-selected", "true");
    expect(codexTab).toHaveAttribute("data-unimplemented", "true");
    expect(
      agenticLevel().getByRole("tab", { name: /Claude Code/ }),
    ).toHaveAttribute("data-unimplemented", "false");

    // The statement comes first in the panel.
    const panel = await agenticLevel().findByTestId("agentic-panel-codex");
    const notice = within(panel).getByTestId("to-be-implemented");
    expect(notice).toHaveAttribute("role", "note");
    expect(notice).toHaveTextContent("To Be Implemented...");
    expect(panel.firstElementChild).toBe(notice);

    // Every field, selector, and action is disabled, Verify included.
    const controls = operableControls(panel);
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) expect(control).toBeDisabled();
    const verify = within(panel).getByRole("button", { name: "Verify" });
    fireEvent.click(verify);

    // The empty state invites the author to configure it, so it is not shown.
    expect(within(panel).queryByTestId("agentic-empty")).toBeNull();

    // AII-FR-17 / AII-FR-20: no detection and no verification for this tab.
    // Claude Code opens first with no stored path, so it may have detected.
    const detected = invokeMock.mock.calls
      .filter((c) => c[0] === "detect_agentic_cli_binary")
      .map((c) => (c[1] as { vendor: string }).vendor);
    expect(detected).not.toContain("codex");
    expect(commands()).not.toContain("verify_agentic_integration");
  });

  it("AII-FR-OTQY, AII-FR-16: an API tab is read-only and still shows that it is active", async () => {
    backend({
      agentic: [
        cliVerified("claude_code", "/bin/claude"),
        agenticIntegration("codex"),
        agenticIntegration("opencode"),
        apiAgentVerified("claude_agent_api", "https://api.anthropic.com/v1", {
          active: true,
        }),
        agenticIntegration("custom_agent_api"),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    const apiTab = await agenticLevel().findByRole("tab", {
      name: /Claude Agent API/,
    });
    // The active badge stays on the tab.
    expect(within(apiTab).getByText("active")).toBeInTheDocument();
    await user.click(apiTab);

    const panel = await agenticLevel().findByTestId(
      "agentic-panel-claude_agent_api",
    );
    expect(within(panel).getByTestId("to-be-implemented")).toBeInTheDocument();
    expect(within(panel).getByTestId("agentic-active-marker")).toBeInTheDocument();
    expect(within(panel).getByLabelText("Base URL")).toBeDisabled();
    expect(within(panel).getByLabelText("API key")).toBeDisabled();
    expect(within(panel).getByRole("button", { name: "Clear" })).toBeDisabled();
    for (const control of operableControls(panel)) expect(control).toBeDisabled();

    // Even an event that reaches the handlers invokes nothing: no Clear, no
    // model, no effort, and no file picker.
    fireEvent.click(within(panel).getByRole("button", { name: "Clear" }));
    fireEvent.change(within(panel).getByLabelText("Reasoning effort"), {
      target: { value: "high" },
    });
    fireEvent.click(within(panel).getByTestId("agentic-model"));
    const option = await screen.findByRole("option", { name: "Opus" });
    fireEvent.click(option);
    for (const cmd of [
      "clear_agentic_integration",
      "set_agentic_integration_model",
      "set_agentic_integration_effort",
      "set_active_agentic_integration",
    ]) {
      expect(commands()).not.toContain(cmd);
    }
  });

  it("AII-FR-03, AII-FR-17, AII-FR-OTQY: the gate applies per tab, and Claude Code stays editable", async () => {
    backend({
      agentic: AGENTIC_ALL.map((v) => agenticIntegration(v)),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    // Claude Code opens first, with no stored path, so it detects.
    await waitFor(() =>
      expect(argsOf("detect_agentic_cli_binary")).toEqual({
        vendor: "claude_code",
      }),
    );

    for (const [name, vendor] of [
      [/Codex/, "codex"],
      [/OpenCode/, "opencode"],
      [/Claude Agent API/, "claude_agent_api"],
      [/Custom agent API/, "custom_agent_api"],
    ] as const) {
      await user.click(await agenticLevel().findByRole("tab", { name }));
      const panel = await agenticLevel().findByTestId(`agentic-panel-${vendor}`);
      expect(within(panel).getByTestId("to-be-implemented")).toBeInTheDocument();
    }

    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    const panel = await agenticLevel().findByTestId("agentic-panel-claude_code");
    expect(within(panel).queryByTestId("to-be-implemented")).toBeNull();
    expect(within(panel).getByLabelText("Binary")).toBeEnabled();

    // Every detection was for Claude Code.
    const detections = invokeMock.mock.calls
      .filter((c) => c[0] === "detect_agentic_cli_binary")
      .map((c) => (c[1] as { vendor: string }).vendor);
    expect(detections.length).toBeGreaterThan(0);
    expect(new Set(detections)).toEqual(new Set(["claude_code"]));
  });

  it("AII-FR-OTQY: opening a read-only tab logs one debug record with the vendor alone", async () => {
    backend({
      agentic: [
        cliVerified("claude_code", "/bin/claude"),
        ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(await agenticLevel().findByRole("tab", { name: /Codex/ }));
    await agenticLevel().findByTestId("to-be-implemented");
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    await agenticLevel().findByTestId("agentic-panel-claude_code");

    const records = loggedRecords().filter(
      (r) => r.message === "agentic tab not implemented",
    );
    expect(records).toHaveLength(1);
    expect(records[0].level).toBe("DEBUG");
    expect(records[0].fields).toEqual({ vendor: "codex" });
  });
});

describe("the project agentic override", () => {
  it("AII-FR-32, AII-FR-OTQY: offers only implemented integrations, and still names a recorded override to another", async () => {
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/bin/claude", { active: true }),
          cliVerified("codex", "/bin/codex"),
          agenticIntegration("opencode"),
        ],
        project: {
          vendor: "codex",
          resolution: "overridden",
          overrideVendor: "codex",
        },
      },
      {
        set_project_agentic_integration: () => ({
          vendor: "claude_code",
          resolution: "inherited",
          overrideVendor: null,
        }),
      },
    );
    render(<ProjectAiIntegrations />);
    const user = userEvent.setup();

    const control = within(
      await screen.findByTestId("settings-agentic-integration"),
    );
    // The recorded override still renders as the project's choice.
    await waitFor(() =>
      expect(
        control.getByTestId("settings-agentic-integration-resolution"),
      ).toHaveTextContent(/Codex · overrides the global choice/),
    );

    await user.click(control.getByRole("button", { name: /change/i }));
    const select = control.getByLabelText("Agentic integration");
    // Codex is verified, but it is not implemented, so it is not offered. It
    // stays the shown value as a disabled entry, so that choosing the inherit
    // entry is a change.
    const options = within(select).getAllByRole("option") as HTMLOptionElement[];
    expect(options.map((o) => o.textContent)).toEqual([
      "Inherit the global choice",
      "Codex",
      "Claude Code",
    ]);
    expect(options.filter((o) => !o.disabled).map((o) => o.textContent)).toEqual([
      "Inherit the global choice",
      "Claude Code",
    ]);
    expect(select).toHaveValue("codex");

    // The author can still leave the override.
    await user.selectOptions(select, "");
    await waitFor(() =>
      expect(argsOf("set_project_agentic_integration")).toEqual({ vendor: null }),
    );
  });

  it("AII-FR-32, AII-FR-OTQY: with nothing to offer, a recorded override still has the action that clears it", async () => {
    backend(
      {
        agentic: [
          agenticIntegration("claude_code"),
          cliVerified("codex", "/bin/codex", { active: true }),
        ],
        project: {
          vendor: "codex",
          resolution: "overridden",
          overrideVendor: "codex",
        },
      },
      {
        set_project_agentic_integration: () => ({
          vendor: "codex",
          resolution: "inherited",
          overrideVendor: null,
        }),
      },
    );
    render(<ProjectAiIntegrations />);
    const user = userEvent.setup();

    const control = within(
      await screen.findByTestId("settings-agentic-integration"),
    );
    await user.click(await control.findByRole("button", { name: /change/i }));
    const select = control.getByLabelText("Agentic integration");
    const options = within(select).getAllByRole("option") as HTMLOptionElement[];
    expect(options.filter((o) => !o.disabled).map((o) => o.textContent)).toEqual([
      "Inherit the global choice",
    ]);

    await user.selectOptions(select, "");
    await waitFor(() =>
      expect(argsOf("set_project_agentic_integration")).toEqual({ vendor: null }),
    );
  });
});
