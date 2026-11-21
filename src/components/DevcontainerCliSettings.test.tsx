import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { DevcontainerCli } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { DevcontainerCliSettings, devcontainerCliFailureMessage } from "./DevcontainerCliSettings";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

let stored: DevcontainerCli;
let detected: string | null;
let verifyAnswer: DevcontainerCli | { code: string; message: string };

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  stored = { path: "", state: "unverified", version: null, verifiedAt: null };
  detected = "/opt/homebrew/bin/devcontainer";
  verifyAnswer = { path: "", state: "verified", version: "0.91.0", verifiedAt: "t" };
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "load_devcontainer_cli":
        return stored;
      case "detect_devcontainer_cli":
        return { path: detected };
      case "save_devcontainer_cli":
        stored = { path: String(args?.path), state: "unverified", version: null, verifiedAt: null };
        return stored;
      case "verify_devcontainer_cli":
        if ("code" in verifyAnswer) throw verifyAnswer;
        return verifyAnswer;
      case "append_log_records":
        return undefined;
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
});
afterEach(cleanup);

describe("Global settings → Docker → Dev container CLI (GLS-FR-AGPG, GLS-FR-RQSP)", () => {
  it("GLS-FR-AGPG: an empty path is filled from detection and marked, and the scope is stated", async () => {
    render(<DevcontainerCliSettings />);
    const field = await screen.findByLabelText("Path");
    await waitFor(() => expect(field).toHaveValue("/opt/homebrew/bin/devcontainer"));
    expect(screen.getByText("Found on this machine.")).toBeInTheDocument();
    expect(screen.getByText(/Only a project whose agent environment is a dev container/)).toBeInTheDocument();
    expect(calls("save_devcontainer_cli")).toHaveLength(0);
  });

  it("GLS-FR-RQSP: Verify stores a detected path, then shows the version", async () => {
    render(<DevcontainerCliSettings />);
    const field = await screen.findByLabelText("Path");
    await waitFor(() => expect(field).toHaveValue("/opt/homebrew/bin/devcontainer"));
    const verifyButton = screen.getByRole("button", { name: "Verify the dev container CLI" });
    await waitFor(() => expect(verifyButton).toBeEnabled());
    await userEvent.click(verifyButton);
    await waitFor(() => expect(screen.getByTestId("devcontainer-cli-status")).toHaveTextContent("0.91.0"));
    expect(calls("save_devcontainer_cli")[0][1]).toEqual({ path: "/opt/homebrew/bin/devcontainer" });
  });

  it("GLS-FR-RQSP: a changed path persists on blur and the status is unverified", async () => {
    stored = { path: "/a/devcontainer", state: "verified", version: "0.90.0", verifiedAt: "t" };
    render(<DevcontainerCliSettings />);
    await waitFor(() => expect(screen.getByTestId("devcontainer-cli-status")).toHaveTextContent("0.90.0"));
    const field = screen.getByLabelText("Path");
    await userEvent.clear(field);
    await userEvent.type(field, "/b/devcontainer");
    await userEvent.tab();
    await waitFor(() => expect(calls("save_devcontainer_cli")).toHaveLength(1));
    await waitFor(() =>
      expect(screen.getByTestId("devcontainer-cli-status")).toHaveTextContent("Not verified yet."),
    );
  });

  it("GLS-FR-RQSP: a failure is stated in words", async () => {
    verifyAnswer = { code: "not_found", message: "" };
    detected = null;
    render(<DevcontainerCliSettings />);
    const verifyButton = await screen.findByRole("button", { name: "Verify the dev container CLI" });
    await waitFor(() => expect(verifyButton).toBeEnabled());
    await userEvent.click(verifyButton);
    await waitFor(() =>
      expect(screen.getByTestId("devcontainer-cli-status")).toHaveTextContent("npm install -g @devcontainers/cli"),
    );
    expect(devcontainerCliFailureMessage({ code: "timed_out" })).toMatch(/did not answer in time/);
    expect(devcontainerCliFailureMessage({ code: "not_executable" })).toMatch(/not executable/);
    expect(devcontainerCliFailureMessage({ code: "failed", message: "boom" })).toMatch(/boom/);
  });
});
