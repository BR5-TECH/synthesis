import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { EmbeddingsProjectStatus, EmbeddingsStatus, ProjectEmbeddingsMode } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Handler[]>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: Handler) => {
    listeners.set(name, [...(listeners.get(name) ?? []), handler]);
    return Promise.resolve(() => {
      listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== handler));
    });
  },
  emit: () => Promise.resolve(),
}));

function pushStatus(next: EmbeddingsStatus) {
  act(() => {
    for (const handler of listeners.get("embeddings-status-changed") ?? []) handler({ payload: next });
  });
}

import { ProjectIndexingSettings } from "./ProjectIndexingSettings";

let mode: ProjectEmbeddingsMode;
let status: EmbeddingsStatus;
let refuseMode: string | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function project(over: Partial<EmbeddingsProjectStatus> = {}): EmbeddingsProjectStatus {
  return {
    mode: "use_global",
    working: true,
    state: "ready",
    git: "repository",
    branch: "feature",
    detached: false,
    rootBranch: "main",
    rootTip: "0123456789abcdef",
    specChunks: 42,
    documentChunks: 7,
    ...over,
  };
}

beforeEach(() => {
  resetLogBufferForTest();
  listeners.clear();
  invokeMock.mockReset();
  mode = "use_global";
  refuseMode = null;
  status = { enabled: true, model: "BGESmallENV15", modelState: "ready", project: project() };
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_project_embeddings_mode") return mode;
    if (cmd === "save_project_embeddings_mode") {
      if (refuseMode) throw new Error(refuseMode);
      mode = (args as { mode: ProjectEmbeddingsMode }).mode;
      return mode;
    }
    if (cmd === "get_embeddings_status") return status;
    if (cmd === "start_reindex") return status;
    if (cmd === "open_settings_window") return undefined;
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

describe("Project Indexing section", () => {
  it("SET-FR-QCUJ: three choices, Use Global for a project that stored nothing", async () => {
    render(<ProjectIndexingSettings />);
    const useGlobal = await screen.findByRole("radio", { name: "Use Global" });
    await waitFor(() => expect(useGlobal).toBeChecked());
    expect(screen.getByRole("radio", { name: "On" })).not.toBeChecked();
    expect(screen.getByRole("radio", { name: "Off" })).not.toBeChecked();
    expect(screen.getByText(/never committed/)).toBeInTheDocument();
  });

  it("SET-FR-DHIZ: a choice persists at once; a refused write restores the previous one", async () => {
    const user = userEvent.setup();
    render(<ProjectIndexingSettings />);
    await waitFor(() => expect(screen.getByRole("radio", { name: "Use Global" })).toBeChecked());
    await user.click(screen.getByRole("radio", { name: "Off" }));
    await waitFor(() => expect(calls("save_project_embeddings_mode")).toHaveLength(1));
    expect(calls("save_project_embeddings_mode")[0][1]).toEqual({ mode: "off" });

    refuseMode = "store unavailable";
    await user.click(screen.getByRole("radio", { name: "On" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("store unavailable");
    expect(screen.getByRole("radio", { name: "Off" })).toBeChecked();
  });

  it("SET-FR-MKDJ, SET-FR-QCUJ: with Global off the section shows, names the reason, and routes to Global Indexing", async () => {
    const user = userEvent.setup();
    status = { enabled: false, model: "BGESmallENV15", modelState: "not_downloaded", project: project({ working: false, state: "off" }) };
    mode = "on";
    render(<ProjectIndexingSettings />);
    const line = await screen.findByTestId("project-indexing-working");
    await waitFor(() => expect(line).toHaveTextContent(/off in Global settings/));
    expect(line).toHaveTextContent(/On does nothing/);
    await user.click(within(line).getByRole("button", { name: "Open Global Indexing" }));
    await waitFor(() => expect(calls("open_settings_window")).toHaveLength(1));
    expect(calls("open_settings_window")[0][1]).toEqual({ section: "global/indexing" });
  });

  it("SET-FR-UNRY: the status names the state, the root branch and tip, the branch, and the counts", async () => {
    render(<ProjectIndexingSettings />);
    const panel = await screen.findByTestId("project-indexing-status");
    expect(panel).toHaveTextContent("Ready");
    expect(panel).toHaveTextContent("Default branch main at 01234567; working on branch feature.");
    expect(panel).toHaveTextContent("42 specification chunks and 7 document chunks");
  });

  it("SET-FR-HRRN: Reindex is disabled while embeddings do not work for the project", async () => {
    status = { ...status, project: project({ working: false, state: "off", mode: "off" }) };
    mode = "off";
    render(<ProjectIndexingSettings />);
    await screen.findByTestId("project-indexing-status");
    expect(screen.getByRole("button", { name: "Reindex…" })).toBeDisabled();
    expect(screen.getByTestId("project-indexing-working")).toHaveTextContent("off for this project");
  });

  it("SET-FR-WZPS: the dialog offers Current branch (selected) and Whole trace; confirming starts that scope", async () => {
    const user = userEvent.setup();
    render(<ProjectIndexingSettings />);
    await user.click(await screen.findByRole("button", { name: "Reindex…" }));
    const dialog = await screen.findByTestId("reindex-dialog");
    const current = within(dialog).getByRole("radio", { name: /Current branch/ });
    expect(current).toBeChecked();
    expect(dialog).toHaveTextContent(/The default branch stays/);
    await user.click(within(dialog).getByRole("radio", { name: /Whole trace/ }));
    await user.click(within(dialog).getByRole("button", { name: "Reindex" }));
    await waitFor(() => expect(calls("start_reindex")).toHaveLength(1));
    expect(calls("start_reindex")[0][1]).toEqual({ scope: "whole_trace" });
    await waitFor(() => expect(screen.queryByTestId("reindex-dialog")).toBeNull());
  });

  it("SET-FR-VGQB: the section holds no model picker and starts no download", async () => {
    render(<ProjectIndexingSettings />);
    await screen.findByTestId("project-indexing-status");
    expect(screen.queryByLabelText("Model")).toBeNull();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(calls("save_embeddings_settings")).toHaveLength(0);
    expect(calls("retry_embeddings_model_download")).toHaveLength(0);
  });

  it("SET-FR-WZPS: a refused Reindex keeps the dialog open and says why", async () => {
    const user = userEvent.setup();
    const wired = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "start_reindex") throw new Error("embeddings_not_working");
      return wired(cmd, args);
    });
    render(<ProjectIndexingSettings />);
    await user.click(await screen.findByRole("button", { name: "Reindex…" }));
    const dialog = await screen.findByTestId("reindex-dialog");
    await user.click(within(dialog).getByRole("button", { name: "Reindex" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("do not work for this project");
    expect(screen.getByTestId("reindex-dialog")).toBeInTheDocument();
  });

  it("SET-FR-UNRY: the job progress and the last error render", async () => {
    status = {
      ...status,
      project: project({ state: "preparing", job: { kind: "reindex", completed: 2, total: 8 }, lastError: "embed failed" }),
    };
    render(<ProjectIndexingSettings />);
    expect(await screen.findByTestId("embeddings-job")).toHaveTextContent("Reindexing embeddings — 2 of 8 chunks");
    expect(screen.getByTestId("project-indexing-status")).toHaveTextContent("embed failed");
    expect(screen.getByTestId("project-indexing-status")).toHaveTextContent("Preparing");
  });

  it("SET-FR-WZPS: Cancel and Escape close the dialog and start nothing", async () => {
    const user = userEvent.setup();
    render(<ProjectIndexingSettings />);
    await user.click(await screen.findByRole("button", { name: "Reindex…" }));
    await user.click(within(await screen.findByTestId("reindex-dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("reindex-dialog")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Reindex…" }));
    await screen.findByTestId("reindex-dialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("reindex-dialog")).toBeNull();
    expect(calls("start_reindex")).toHaveLength(0);
  });

  it("SET-FR-WZPS: the dialog keeps Tab inside it and returns focus to Reindex when it closes", async () => {
    const user = userEvent.setup();
    render(<ProjectIndexingSettings />);
    const opener = await screen.findByRole("button", { name: "Reindex…" });
    await user.click(opener);
    const dialog = await screen.findByTestId("reindex-dialog");
    expect(within(dialog).getByRole("radio", { name: /Current branch/ })).toHaveFocus();
    for (let i = 0; i < 6; i++) {
      await user.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("reindex-dialog")).toBeNull());
    expect(opener).toHaveFocus();
  });

  it("SET-FR-HRRN: on a detached HEAD Current branch says it embeds the documents only; outside Git it embeds the whole project", async () => {
    const user = userEvent.setup();
    status = { ...status, project: project({ detached: true, branch: undefined }) };
    render(<ProjectIndexingSettings />);
    await user.click(await screen.findByRole("button", { name: "Reindex…" }));
    expect(await screen.findByTestId("reindex-dialog")).toHaveTextContent(/documents again only/);
    cleanup();

    status = { ...status, project: project({ git: "none", branch: undefined }) };
    render(<ProjectIndexingSettings />);
    await user.click(await screen.findByRole("button", { name: "Reindex…" }));
    expect(await screen.findByTestId("reindex-dialog")).toHaveTextContent(/whole project and the documents again/);
  });

  it("SET-FR-VGQB: the model download shows its progress here too, with no Retry", async () => {
    status = {
      enabled: true,
      model: "BGESmallENV15",
      modelState: "downloading",
      download: { completedBytes: 5 * 1024 * 1024, totalBytes: 10 * 1024 * 1024 },
      project: project({ state: "preparing" }),
    };
    render(<ProjectIndexingSettings />);
    expect(await screen.findByTestId("embeddings-download")).toHaveTextContent("5.0 MB of 10.0 MB");
    pushStatus({ ...status, modelState: "ready", download: undefined });
    await waitFor(() => expect(screen.queryByTestId("embeddings-download")).toBeNull());
    pushStatus({ ...status, modelState: "failed", modelError: "network unreachable" });
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    expect(calls("retry_embeddings_model_download")).toHaveLength(0);
  });

  it("SET-FR-WZPS: the dialog closes when the project status goes, and does not open again", async () => {
    const user = userEvent.setup();
    render(<ProjectIndexingSettings />);
    await user.click(await screen.findByRole("button", { name: "Reindex…" }));
    expect(await screen.findByTestId("reindex-dialog")).toBeInTheDocument();
    pushStatus({ enabled: true, model: "BGESmallENV15", modelState: "ready" });
    await waitFor(() => expect(screen.queryByTestId("reindex-dialog")).toBeNull());
    expect(screen.getByTestId("project-indexing-working")).toHaveTextContent(/not known yet/);
    pushStatus({ enabled: true, model: "BGESmallENV15", modelState: "ready", project: project() });
    await screen.findByTestId("project-indexing-status");
    expect(screen.queryByTestId("reindex-dialog")).toBeNull();
    expect(calls("start_reindex")).toHaveLength(0);
  });
});
