import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { EmbeddingModel, EmbeddingsSettings, EmbeddingsStatus } from "../types";

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

import { IndexingSettings } from "./IndexingSettings";

const MODELS: EmbeddingModel[] = [
  { id: "BGESmallENV15", name: "BGE-small-en-v1.5", dimension: 384, backend: "onnx", recommended: true, default: true },
  { id: "AllMiniLML6V2", name: "all-MiniLM-L6-v2", dimension: 384, backend: "onnx", recommended: true, default: false },
  { id: "EmbeddingGemma300M", name: "EmbeddingGemma 300M", dimension: 768, backend: "onnx", recommended: true, default: false },
  { id: "BGEM3", name: "BGE-M3", dimension: 1024, backend: "onnx", recommended: true, default: false },
  { id: "GTEBaseENV15", name: "gte-base-en-v1.5", dimension: 768, backend: "onnx", recommended: false, default: false },
];

let stored: EmbeddingsSettings;
let status: EmbeddingsStatus;
let refuseSave: string | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function pushStatus(next: EmbeddingsStatus) {
  act(() => {
    for (const handler of listeners.get("embeddings-status-changed") ?? []) handler({ payload: next });
  });
}

beforeEach(() => {
  resetLogBufferForTest();
  listeners.clear();
  invokeMock.mockReset();
  refuseSave = null;
  stored = { enabled: false, model: "BGESmallENV15" };
  status = { enabled: false, model: "BGESmallENV15", modelState: "not_downloaded" };
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_embeddings_settings") return stored;
    if (cmd === "list_embedding_models") return MODELS;
    if (cmd === "get_embeddings_status") return status;
    if (cmd === "save_embeddings_settings") {
      if (refuseSave) throw new Error(refuseSave);
      stored = (args as { settings: EmbeddingsSettings }).settings;
      return stored;
    }
    if (cmd === "retry_embeddings_model_download") return status;
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

const embeddingsSwitch = () => screen.findByRole("switch", { name: "Use embeddings" });

describe("Global Indexing section", () => {
  it("GLS-FR-JDUM, GLS-FR-QRJM: the switch is off on a fresh install and states that BM25 is always on", async () => {
    render(<IndexingSettings />);
    expect(await embeddingsSwitch()).not.toBeChecked();
    expect(screen.getByText(/Search always ranks with BM25/)).toBeInTheDocument();
    expect(calls("save_embeddings_settings")).toHaveLength(0);
  });

  it("GLS-FR-QRJM, GLS-FR-UWFA: turning the switch on saves at once, with no confirmation", async () => {
    const user = userEvent.setup();
    render(<IndexingSettings />);
    await user.click(await embeddingsSwitch());
    await waitFor(() => expect(calls("save_embeddings_settings")).toHaveLength(1));
    expect(calls("save_embeddings_settings")[0][1]).toEqual({
      settings: { enabled: true, model: "BGESmallENV15" },
    });
    expect(screen.queryByTestId("model-switch-confirm")).toBeNull();
  });

  it("GLS-FR-UWFA: a refused save restores the switch and says why", async () => {
    const user = userEvent.setup();
    refuseSave = "disk full";
    render(<IndexingSettings />);
    await user.click(await embeddingsSwitch());
    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
    expect(await embeddingsSwitch()).not.toBeChecked();
  });

  it("GLS-FR-KRZU: the picker lists the recommended four first, marked, with the default marked", async () => {
    render(<IndexingSettings />);
    const select = await screen.findByLabelText("Model");
    const recommended = within(select).getByRole("group", { name: "Recommended" });
    const options = within(recommended).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      "BGE-small-en-v1.5 — 384 dimensions (Recommended, Default)",
      "all-MiniLM-L6-v2 — 384 dimensions (Recommended)",
      "EmbeddingGemma 300M — 768 dimensions (Recommended)",
      "BGE-M3 — 1024 dimensions (Recommended)",
    ]);
    const others = within(select).getByRole("group", { name: "Other models" });
    expect(within(others).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "gte-base-en-v1.5 — 768 dimensions",
    ]);
  });

  it("GLS-FR-ZBQR: while off, a new model saves at once with no confirmation", async () => {
    const user = userEvent.setup();
    render(<IndexingSettings />);
    await user.selectOptions(await screen.findByLabelText("Model"), "BGEM3");
    await waitFor(() => expect(calls("save_embeddings_settings")).toHaveLength(1));
    expect(screen.queryByTestId("model-switch-confirm")).toBeNull();
  });

  it("GLS-FR-ZBQR: while on, a new model asks first; Cancel keeps the old model, confirming saves the new one", async () => {
    const user = userEvent.setup();
    stored = { enabled: true, model: "BGESmallENV15" };
    render(<IndexingSettings />);
    const select = await screen.findByLabelText("Model");
    await user.selectOptions(select, "BGEM3");
    const dialog = await screen.findByTestId("model-switch-confirm");
    expect(dialog).toHaveTextContent(/embeds its specifications and documents again/);
    expect(dialog).toHaveTextContent(/BM25 only/);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(calls("save_embeddings_settings")).toHaveLength(0);
    expect(screen.getByLabelText("Model")).toHaveValue("BGESmallENV15");

    await user.selectOptions(screen.getByLabelText("Model"), "BGEM3");
    await user.click(within(await screen.findByTestId("model-switch-confirm")).getByRole("button", { name: "Switch model" }));
    await waitFor(() => expect(calls("save_embeddings_settings")).toHaveLength(1));
    expect(calls("save_embeddings_settings")[0][1]).toEqual({ settings: { enabled: true, model: "BGEM3" } });
  });

  it("GLS-FR-ZBQR: Escape and a click outside the confirmation keep the old model and save nothing", async () => {
    const user = userEvent.setup();
    stored = { enabled: true, model: "BGESmallENV15" };
    render(<IndexingSettings />);
    await user.selectOptions(await screen.findByLabelText("Model"), "BGEM3");
    await screen.findByTestId("model-switch-confirm");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("model-switch-confirm")).toBeNull());
    expect(screen.getByLabelText("Model")).toHaveValue("BGESmallENV15");
    expect(screen.getByLabelText("Model")).toHaveFocus();

    await user.selectOptions(screen.getByLabelText("Model"), "BGEM3");
    const confirm = await screen.findByTestId("model-switch-confirm");
    await user.click(confirm.closest(".scrim")!);
    expect(screen.queryByTestId("model-switch-confirm")).toBeNull();
    expect(calls("save_embeddings_settings")).toHaveLength(0);
  });

  it("GLS-FR-UWFA: a refused model restores the stored one with the reason; two quick refusals end on the stored value", async () => {
    const user = userEvent.setup();
    refuseSave = "unknown_model";
    render(<IndexingSettings />);
    await user.selectOptions(await screen.findByLabelText("Model"), "BGEM3");
    expect(await screen.findByRole("alert")).toHaveTextContent("not available in this build");
    await waitFor(() => expect(screen.getByLabelText("Model")).toHaveValue("BGESmallENV15"));

    refuseSave = "disk full";
    await user.selectOptions(screen.getByLabelText("Model"), "BGEM3");
    await user.selectOptions(screen.getByLabelText("Model"), "AllMiniLML6V2");
    await waitFor(() => expect(calls("save_embeddings_settings")).toHaveLength(3));
    await waitFor(() => expect(screen.getByLabelText("Model")).toHaveValue("BGESmallENV15"));
  });

  it("GLS-FR-RQFK: with no project open only the model state shows, no job row", async () => {
    status = { enabled: true, model: "BGESmallENV15", modelState: "ready" };
    render(<IndexingSettings />);
    expect(await screen.findByText("The model is ready.")).toBeInTheDocument();
    expect(screen.queryByTestId("embeddings-job")).toBeNull();
  });

  it("GLS-FR-RQFK: the download shows its bytes, then a job shows its count of chunks", async () => {
    render(<IndexingSettings />);
    await embeddingsSwitch();
    pushStatus({
      enabled: true,
      model: "BGESmallENV15",
      modelState: "downloading",
      download: { completedBytes: 5 * 1024 * 1024, totalBytes: 10 * 1024 * 1024 },
    });
    expect(await screen.findByTestId("embeddings-download")).toHaveTextContent("5.0 MB of 10.0 MB");
    pushStatus({
      enabled: true,
      model: "BGESmallENV15",
      modelState: "ready",
      project: {
        mode: "use_global",
        working: true,
        state: "preparing",
        job: { kind: "first_indexing", completed: 3, total: 12 },
        git: "repository",
        detached: false,
        specChunks: 0,
        documentChunks: 0,
      },
    });
    expect(await screen.findByTestId("embeddings-job")).toHaveTextContent("Embedding specifications — 3 of 12 chunks");
    expect(screen.queryByTestId("embeddings-download")).toBeNull();
  });

  it("GLS-FR-HEPP: a failed download shows its reason and a Retry action", async () => {
    const user = userEvent.setup();
    status = { enabled: true, model: "BGESmallENV15", modelState: "failed", modelError: "network unreachable" };
    render(<IndexingSettings />);
    const alert = await screen.findByText(/could not be downloaded: network unreachable/);
    expect(alert).toHaveTextContent(/BM25 only/);
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(calls("retry_embeddings_model_download")).toHaveLength(1));
  });
});
