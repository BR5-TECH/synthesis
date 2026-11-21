import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { AgentEnvironmentState, DevcontainerCli } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listeners = new Map<string, (payload: unknown) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, (payload) => handler({ payload }));
    return Promise.resolve(() => listeners.delete(name));
  },
  emit: () => Promise.resolve(),
}));

import { ProjectAgentEnvironment } from "./ProjectAgentEnvironment";
import { agentEnvironmentErrorMessage, devcontainerBuildRefusalMessage } from "./ProjectDevcontainer";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const CONFIG = ".devcontainer/devcontainer.json";

function environment(patch: Partial<AgentEnvironmentState> = {}): AgentEnvironmentState {
  return {
    environment: { source: "devcontainer", devcontainerConfig: CONFIG },
    configExists: true,
    agentCli: [
      { vendor: "claude_code", state: "unknown", knownFeature: "ghcr.io/anthropics/devcontainer-features/claude-code" },
      { vendor: "codex", state: "unknown", knownFeature: null },
    ],
    ...patch,
  };
}

let stored: AgentEnvironmentState;
let configs: string[];
let cli: DevcontainerCli;
let isolationMode: "none" | "microsandbox";
let refuseBuild: string | null;
let refuseFeature: unknown;
let inFlight: { operationId: string; lines: string[] } | null;

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  listeners.clear();
  stored = environment();
  configs = [CONFIG];
  cli = { path: "/usr/local/bin/devcontainer", state: "verified", version: "0.91.0", verifiedAt: "t" };
  isolationMode = "none";
  refuseBuild = null;
  refuseFeature = null;
  inFlight = null;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "load_agent_environment":
        return stored;
      case "save_agent_environment": {
        const next = args?.environment as AgentEnvironmentState["environment"];
        if (next.source === "devcontainer" && !next.devcontainerConfig) {
          throw { code: "config_missing" };
        }
        stored = { ...stored, environment: next };
        return stored;
      }
      case "list_devcontainer_configs":
        return configs;
      case "create_devcontainer_config":
        configs = [CONFIG];
        stored = environment();
        return stored;
      case "add_devcontainer_agent_feature":
        if (refuseFeature) throw refuseFeature;
        stored = environment({
          agentCli: [{ ...environment().agentCli[0], state: "found" }, environment().agentCli[1]],
        });
        return stored;
      case "load_devcontainer_cli":
        return cli;
      case "load_isolation_settings":
        return { settings: { mode: isolationMode } };
      case "get_project_agentic_integration":
        return { vendor: "claude_code", resolution: "inherited", override: null };
      case "load_devcontainer_build_in_flight":
        return inFlight;
      case "build_devcontainer":
        if (refuseBuild) throw new Error(refuseBuild);
        return { operationId: "devcontainer-build-1" };
      case "cancel_devcontainer_build":
        return undefined;
      case "open_settings_window":
        return undefined;
      case "load_project_docker_images":
      case "load_docker_backend":
      case "load_project_image_build_in_flight":
      case "load_docker_in_docker_settings":
        throw new Error(`vendor images not under test: ${cmd}`);
      case "append_log_records":
        return undefined;
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
});
afterEach(cleanup);

const emit = (name: string, payload: unknown) => act(() => listeners.get(name)?.(payload));

describe("Project settings → Docker → Agent environment (SET-FR-SVBS)", () => {
  it("SET-FR-SVBS: the selector reads the stored choice, and a change persists at once", async () => {
    stored = environment({ environment: { source: "vendor_images", devcontainerConfig: CONFIG } });
    render(<ProjectAgentEnvironment />);
    const devcontainer = await screen.findByRole("radio", { name: /Dev container/ });
    expect(screen.getByRole("radio", { name: /Vendor images/ })).toBeChecked();
    await userEvent.click(devcontainer);
    await waitFor(() => expect(calls("save_agent_environment")).toHaveLength(1));
    expect(calls("save_agent_environment")[0][1]).toEqual({
      environment: { source: "devcontainer", devcontainerConfig: CONFIG },
    });
    expect(await screen.findByRole("region", { name: "Dev container" })).toBeInTheDocument();
    expect(screen.getByText(/kept on this machine and is not committed/)).toBeInTheDocument();
  });

  it("SET-FR-SVBS, SET-FR-TEZH: Dev container with no config stores nothing and offers Create", async () => {
    stored = environment({ environment: { source: "vendor_images", devcontainerConfig: null } });
    configs = [];
    render(<ProjectAgentEnvironment />);
    await userEvent.click(await screen.findByRole("radio", { name: /Dev container/ }));
    const create = await screen.findByRole("button", { name: "Create devcontainer.json" });
    expect(calls("save_agent_environment")).toHaveLength(0);
    expect(screen.getByText(/has no devcontainer.json/)).toBeInTheDocument();
    await userEvent.click(create);
    await waitFor(() => expect(calls("create_devcontainer_config")).toHaveLength(1));
    expect(await screen.findByRole("combobox", { name: "Configuration" })).toHaveValue(CONFIG);
  });
});

describe("Project settings → Docker → dev container panel (SET-FR-FVIZ)", () => {
  it("SET-FR-FVIZ: the select lists the configs and a selection persists", async () => {
    configs = [CONFIG, ".devcontainer/other/devcontainer.json"];
    render(<ProjectAgentEnvironment />);
    const select = await screen.findByRole("combobox", { name: "Configuration" });
    expect(within(select).getAllByRole("option").map((o) => o.textContent)).toEqual(configs);
    await userEvent.selectOptions(select, ".devcontainer/other/devcontainer.json");
    await waitFor(() => expect(calls("save_agent_environment")).toHaveLength(1));
    expect(calls("save_agent_environment")[0][1]).toEqual({
      environment: { source: "devcontainer", devcontainerConfig: ".devcontainer/other/devcontainer.json" },
    });
  });

  it("SET-FR-PYRJ, SET-FR-CJIA: a row names its state, marks the graduation vendor, and offers the Feature or states the requirement", async () => {
    render(<ProjectAgentEnvironment />);
    const rows = await screen.findByRole("list", { name: "Agent CLI in the dev container" });
    const [claude, codex] = within(rows).getAllByRole("listitem");
    expect(claude).toHaveTextContent("Claude Code");
    expect(claude).toHaveTextContent("Unknown until a build");
    await waitFor(() => expect(claude).toHaveTextContent("Needed for graduation"));
    expect(codex).not.toHaveTextContent("Needed for graduation");
    expect(within(codex).queryByRole("button")).not.toBeInTheDocument();
    expect(codex).toHaveTextContent("must hold codex on the PATH");
    await userEvent.click(within(claude).getByRole("button", { name: "Add the Claude Code feature" }));
    await waitFor(() => expect(calls("add_devcontainer_agent_feature")).toHaveLength(1));
    expect(calls("add_devcontainer_agent_feature")[0][1]).toEqual({ vendor: "claude_code" });
    await waitFor(() => expect(claude).toHaveTextContent("Found"));
    expect(within(claude).queryByRole("button")).not.toBeInTheDocument();
  });

  it("SET-FR-CJIA: a refused Add renders inline in its row with the parser's line", async () => {
    refuseFeature = { code: "config_invalid", message: "unexpected token", line: 4 };
    render(<ProjectAgentEnvironment />);
    await userEvent.click(await screen.findByRole("button", { name: "Add the Claude Code feature" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("(line 4)");
    expect(alert).toHaveTextContent("unexpected token");
  });

  it("SET-FR-MVKM: Build sends the Without cache value, renders the lines, then the result, and offers Cancel while it runs", async () => {
    render(<ProjectAgentEnvironment />);
    await userEvent.click(await screen.findByRole("checkbox", { name: "Without cache" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Build" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() => expect(calls("build_devcontainer")).toHaveLength(1));
    expect(calls("build_devcontainer")[0][1]).toEqual({ noCache: true });
    await waitFor(() => expect(listeners.has("devcontainer-build-progress")).toBe(true));
    emit("devcontainer-build-progress", { operationId: "devcontainer-build-1", message: "Step 1/3" });
    expect(await screen.findByText("Step 1/3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Build" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(calls("cancel_devcontainer_build")[0][1]).toEqual({ operationId: "devcontainer-build-1" });
    emit("devcontainer-build-finished", {
      operationId: "devcontainer-build-1",
      outcome: "failed",
      image: null,
      error: "ERR: boom",
    });
    expect(await screen.findByText("ERR: boom")).toBeInTheDocument();
    expect(screen.getByText("Step 1/3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled();
    await waitFor(() => expect(calls("load_agent_environment").length).toBeGreaterThan(1));
  });

  it("SET-FR-MVKM: a build that runs when the panel opens is restored", async () => {
    inFlight = { operationId: "devcontainer-build-7", lines: ["pulling base"] };
    render(<ProjectAgentEnvironment />);
    expect(await screen.findByText("pulling base")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("SET-FR-VXQZ: an unverified CLI disables Build and links to Global settings", async () => {
    cli = { path: "", state: "unverified", version: null, verifiedAt: null };
    render(<ProjectAgentEnvironment />);
    expect(await screen.findByText("The dev container CLI is not verified")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Build" })).toBeDisabled();
    expect(screen.getByText("Verify the dev container CLI in Global settings first.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Global settings → Docker" }));
    await waitFor(() => expect(calls("open_settings_window")).toHaveLength(1));
    expect(calls("open_settings_window")[0][1]).toEqual({ section: "global/docker" });
  });

  it("SET-FR-MVKM: a refused build renders its reason", async () => {
    refuseBuild = "build_in_flight";
    render(<ProjectAgentEnvironment />);
    const buildButton = await screen.findByRole("button", { name: "Build" });
    await waitFor(() => expect(buildButton).toBeEnabled());
    await userEvent.click(buildButton);
    expect(await screen.findByRole("alert")).toHaveTextContent("already runs");
  });

  it("SET-FR-CLXD: under Microsandbox the panel states that it does not apply; the Docker statement always shows", async () => {
    isolationMode = "microsandbox";
    render(<ProjectAgentEnvironment />);
    expect(await screen.findByText(/Microsandbox does not apply to this project/)).toBeInTheDocument();
    expect(screen.getByText(/docker-outside-of-docker Feature/)).toBeInTheDocument();
  });

  it("SET-FR-CLXD: with isolation None no Microsandbox statement shows, and it shows once the mode is Microsandbox", async () => {
    const { unmount } = render(<ProjectAgentEnvironment />);
    // The resolved vendor is read in the same mount as the isolation mode, so
    // its mark is the sign that the reads have settled.
    await screen.findByText("Needed for graduation");
    expect(screen.queryByText(/Microsandbox does not apply/)).not.toBeInTheDocument();
    unmount();
    isolationMode = "microsandbox";
    render(<ProjectAgentEnvironment />);
    expect(await screen.findByText(/Microsandbox does not apply/)).toBeInTheDocument();
  });

  it("SET-FR-TEZH: after Create the select has the focus", async () => {
    stored = environment({ environment: { source: "devcontainer", devcontainerConfig: null } });
    configs = [];
    render(<ProjectAgentEnvironment />);
    await userEvent.click(await screen.findByRole("button", { name: "Create devcontainer.json" }));
    const select = await screen.findByRole("combobox", { name: "Configuration" });
    await waitFor(() => expect(select).toHaveFocus());
  });
});

describe("refusal words", () => {
  it("SET-FR-SVBS, SET-FR-CJIA: each refusal says what to do", () => {
    expect(agentEnvironmentErrorMessage({ code: "config_path_invalid", reason: "leaves_project" })).toMatch(
      /leaves the project/,
    );
    expect(agentEnvironmentErrorMessage({ code: "config_exists" })).toMatch(/Nothing was overwritten/);
    expect(agentEnvironmentErrorMessage({ code: "no_known_feature" })).toMatch(/no known Feature/);
    expect(devcontainerBuildRefusalMessage(new Error("docker_cli_required"))).toMatch(/No Docker CLI/);
    expect(devcontainerBuildRefusalMessage("devcontainer_cli_missing")).toMatch(/not verified/);
  });
});
