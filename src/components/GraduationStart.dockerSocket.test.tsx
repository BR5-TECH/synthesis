import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { resetLogBufferForTest } from "../logging";
import { graduationErrorMessage } from "../state/graduation";
import { onDockerSocketRefusal } from "../state/dockerSocketRefusal";
import type { WorkStreamSummary } from "../types";
import { GraduationStart } from "./GraduationStart";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const invoked = vi.mocked(invoke);
const SOCKET = "/run/user/1000/docker.sock";

const stream: WorkStreamSummary = {
  stream: {
    id: "editor-work",
    name: "editor-work",
    projectKey: "/dev/acme",
    branch: "synthesis/stream/editor-work",
    baseBranch: "main",
    baseRevision: "a91bc04",
    worktreePath: "/tmp/editor-work",
    createdAt: "2026-09-06T09:00:00Z",
    isMissing: false,
  },
  queuedRunCount: 0,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
};

let refusal: string | null;
let reported: Array<string | null>;
let stopListening: () => void;

beforeEach(() => {
  resetLogBufferForTest();
  reported = [];
  stopListening = onDockerSocketRefusal((path) => reported.push(path));
  refusal = `docker_socket_unavailable: ${SOCKET}`;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (command === "list_work_streams") return [stream];
    if (command === "start_graduation") {
      if (refusal) throw refusal;
      return { id: "g-1" };
    }
    // The commands the dialog sends in the background are answered; any other
    // is refused, as the test rules ask.
    if (command === "get_graduation_capacity") return null;
    if (command === "list_graduation_queue") return { runs: [] };
    if (command === "append_log_records") return undefined;
    throw new Error(`unexpected command ${command}`);
  });
});
afterEach(() => {
  stopListening();
  cleanup();
});

function draw() {
  const onClose = vi.fn();
  const onStarted = vi.fn();
  render(
    <GraduationStart
      draftId="d-1"
      draftName="artifact-window"
      onClose={onClose}
      onStarted={onStarted}
    />,
  );
  return { onClose, onStarted };
}

const graduate = () => screen.getByTestId("graduation-start-confirm");

describe("a start refused for a missing Docker socket (GSD-FR-BXKR)", () => {
  it("GSD-FR-BXKR, GSD-FR-JYRP, GSU-FR-RQDS: the dialog stays open, names the path and the action, and keeps the answers", async () => {
    const { onStarted, onClose } = draw();
    await screen.findByRole("combobox", { name: "Work stream" });
    await userEvent.click(graduate());

    const alert = await screen.findByTestId("graduation-start-error");
    expect(alert).toHaveAttribute("role", "alert");
    expect(alert).toHaveTextContent(SOCKET);
    expect(alert).toHaveTextContent(/Start Docker or restore the socket/);
    expect(alert).toHaveTextContent(/Project settings/);
    expect(alert.textContent).not.toContain("docker_socket_unavailable");
    expect(onStarted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(graduate()).toBeEnabled();
  });

  it("GSD-FR-BXKR, NTF-FR-24: the refusal is reported with the path, so the main window raises the Error toast", async () => {
    draw();
    await screen.findByRole("combobox", { name: "Work stream" });
    await userEvent.click(graduate());

    await screen.findByTestId("graduation-start-error");
    expect(reported).toEqual([SOCKET]);
  });

  it("GSD-FR-BXKR: a refusal of another kind reports nothing", async () => {
    refusal = "docker_backend_unverified";
    draw();
    await screen.findByRole("combobox", { name: "Work stream" });
    await userEvent.click(graduate());
    await screen.findByTestId("graduation-start-error");
    expect(reported).toEqual([]);
  });

  it("GSD-FR-BXKR: after the author restores the socket, the same dialog starts the run", async () => {
    const { onStarted } = draw();
    await screen.findByRole("combobox", { name: "Work stream" });
    await userEvent.click(graduate());
    await screen.findByTestId("graduation-start-error");

    refusal = null;
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
  });

  it("GSD-FR-BXKR: the message is the one graduationErrorMessage gives", () => {
    expect(graduationErrorMessage(`docker_socket_unavailable: ${SOCKET}`)).toContain(SOCKET);
    expect(graduationErrorMessage("docker_socket_unavailable")).toContain(
      "configured path",
    );
  });
});
