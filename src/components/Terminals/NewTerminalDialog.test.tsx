// The New terminal dialog (`TRM-terminals.md` TRM-FR-JINY … TRM-FR-IVGX). The
// backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { logWarn } from "../../logging";
import { refreshTerminals, terminalsSnapshot } from "../../state/terminals";
import { NewTerminalDialog } from "./NewTerminalDialog";
import { TerminalsPanel } from "./TerminalsPanel";
import { commands, resetTerminalFixtures, serve, session, target } from "./terminalFixtures";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

beforeEach(() => {
  resetTerminalFixtures();
  vi.mocked(logWarn).mockClear();
});
afterEach(cleanup);

const stream = target({
  target: { kind: "work_stream", streamId: "st" },
  name: "auth",
  branch: "synthesis/stream/auth",
  isActiveWorktree: false,
  busyRunId: "run-7",
  kinds: [
    { kind: "microsandbox_shell", runsIn: "microsandbox", available: true },
    { kind: "host_shell", runsIn: "host", available: true },
  ],
});
const other = target({ target: { kind: "worktree", path: "/b" }, name: "beta", isActiveWorktree: false });
const other2 = target({ target: { kind: "worktree", path: "/a" }, name: "alpha", isActiveWorktree: false });

function renderDialog() {
  const props = { onClose: vi.fn(), onCreated: vi.fn() };
  render(<NewTerminalDialog {...props} />);
  return props;
}

const targetNames = () =>
  within(screen.getByRole("group", { name: "Target" }))
    .getAllByRole("radio")
    .map((r) => r.closest("label")!.querySelector(".new-terminal__name")!.textContent);

describe("New terminal dialog", () => {
  it("TRM-FR-JINY, TRM-FR-AASW: it reads the targets and lists the active worktree first, then worktrees, then streams", async () => {
    serve("list_terminal_targets", [stream, other, target(), other2]);
    renderDialog();
    await screen.findByRole("group", { name: "Target" });
    expect(commands("list_terminal_targets")).toHaveLength(1);
    expect(targetNames()).toEqual(["repo", "alpha", "beta", "auth"]);
    expect(within(screen.getByRole("group", { name: "Target" })).getAllByRole("radio")[0]).toBeChecked();
  });

  it("TRM-FR-CZTR, TRM-FR-HJUD: kinds come in fixed order, name where they run, and a greyed one shows its reason", async () => {
    serve("list_terminal_targets", [target()]);
    renderDialog();
    const kinds = await screen.findByRole("group", { name: "Kind" });
    const options = within(kinds).getAllByRole("radio");
    expect(options).toHaveLength(4);
    const microsandbox = options[2].closest("label")!;
    expect(microsandbox).toHaveTextContent("Microsandbox shell");
    expect(microsandbox).toHaveTextContent("No VM runs for this target");
    expect(options[2]).toBeDisabled();
    expect(options[0]).toBeChecked();
    expect(options[3].closest("label")).toHaveTextContent("Codex");
    expect(options[1].closest("label")).toHaveTextContent("docker");
  });

  it("TRM-FR-CZTR, TRM-FR-IVGX: a busy target shows the warning and still allows Create; the first available kind is selected", async () => {
    serve("list_terminal_targets", [target(), stream]);
    renderDialog();
    const targets = await screen.findByRole("group", { name: "Target" });
    await userEvent.click(within(targets).getAllByRole("radio")[1]);
    expect(screen.getByText("A graduation run is working in this target.")).toBeInTheDocument();
    const kinds = within(screen.getByRole("group", { name: "Kind" })).getAllByRole("radio");
    expect(kinds[0].closest("label")).toHaveTextContent("Host shell");
    expect(kinds[0]).toBeChecked();
    expect(screen.getByRole("button", { name: "Create" })).toBeEnabled();
  });

  it("TRM-FR-FYYB: Create calls create_terminal with the target, kind and vendor, and hands the session on", async () => {
    serve("list_terminal_targets", [target()]);
    const created = session({ id: "n1", kind: "agent_cli", vendor: "codex", label: "Codex 1" });
    serve("create_terminal", created);
    const props = renderDialog();
    const kinds = await screen.findByRole("group", { name: "Kind" });
    await userEvent.click(within(kinds).getAllByRole("radio")[3]);
    await userEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(props.onCreated).toHaveBeenCalledWith(created));
    expect(commands("create_terminal")[0].args).toEqual({
      target: { kind: "worktree", path: "/repo" },
      kind: "agent_cli",
      vendor: "codex",
    });
    expect(terminalsSnapshot().sessions.map((s) => s.id)).toEqual(["n1"]);
  });

  it("TRM-FR-FYYB: Enter creates, Escape cancels", async () => {
    serve("list_terminal_targets", [target()]);
    serve("create_terminal", session({ id: "n2" }));
    const props = renderDialog();
    await screen.findByRole("group", { name: "Kind" });
    // The selected target holds the focus once the targets are read.
    const targets = screen.getByRole("group", { name: "Target" });
    await waitFor(() => expect(within(targets).getAllByRole("radio")[0]).toHaveFocus());
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(props.onCreated).toHaveBeenCalled());
    expect(commands("create_terminal")[0].args.vendor).toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(props.onClose).toHaveBeenCalled();
  });

  it("TRM-FR-YVPJ: a failed create keeps the dialog open, shows the error and logs a WARN", async () => {
    serve("list_terminal_targets", [target()]);
    serve("create_terminal", () => {
      throw { code: "terminal_limit_reached" };
    });
    const props = renderDialog();
    await screen.findByRole("group", { name: "Kind" });
    await userEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("16");
    expect(props.onCreated).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "New terminal" })).toBeInTheDocument();
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "terminal create failed", expect.objectContaining({ code: "terminal_limit_reached" }));
  });

  it("TRM-FR-JINY, TRM-FR-FYYB: from the panel, a created session appears as a row and its tab opens", async () => {
    serve("list_terminals", []);
    await act(() => refreshTerminals());
    serve("list_terminal_targets", [target()]);
    serve("create_terminal", session({ id: "n3", label: "Host shell 1" }));
    const onOpenTerminal = vi.fn();
    const onOverlayOpening = vi.fn();
    render(<TerminalsPanel activeWorktreePath="/repo" onOpenTerminal={onOpenTerminal} onOverlayOpening={onOverlayOpening} />);
    await userEvent.click(screen.getAllByRole("button", { name: "New terminal" })[0]);
    expect(onOverlayOpening).toHaveBeenCalled();
    await screen.findByRole("group", { name: "Kind" });
    await userEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(onOpenTerminal).toHaveBeenCalledWith(expect.objectContaining({ id: "n3" })));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("option", { name: /^Host shell 1,/ })).toBeInTheDocument();
  });
});
