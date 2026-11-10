import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { OpenSpecCliState, OpenSpecStatus } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { OPENSPEC_INSTALL_COMMAND, OpenSpecSection, cliStatement } from "./OpenSpecSection";

let status: OpenSpecStatus;
let refuseSave: string | null;
let refuseRead: string | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const AVAILABLE: OpenSpecCliState = { state: "available", path: "/usr/local/bin/openspec", version: "1.17.0" };

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  refuseSave = null;
  refuseRead = null;
  status = { setting: "auto", usesOpenspec: true, active: true, cli: AVAILABLE };
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_openspec_status") {
      if (refuseRead) throw new Error(refuseRead);
      return status;
    }
    if (cmd === "save_project_config") {
      if (refuseSave) throw new Error(refuseSave);
      const mode = (args as { config: { openspecMode: "auto" | "off" } }).config.openspecMode;
      status = { ...status, setting: mode, active: mode === "auto" && status.usesOpenspec };
      return undefined;
    }
    if (cmd === "load_project_config") return { lineEndings: "crlf", draftTemplate: null, graduationConcurrencyLimit: 1 };
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

describe("Project OpenSpec section", () => {
  it("SET-FR-FZKL: reads the status on mount and states detection, mode, and the CLI", async () => {
    render(<OpenSpecSection />);
    expect(await screen.findByText("This project uses OpenSpec.")).toBeInTheDocument();
    expect(screen.getByText("OpenSpec mode is active.")).toBeInTheDocument();
    expect(screen.getByTestId("openspec-cli")).toHaveTextContent(
      "OpenSpec CLI 1.17.0 is installed at /usr/local/bin/openspec.",
    );
    expect(calls("get_openspec_status")).toHaveLength(1);
    expect(screen.queryByTestId("openspec-install")).not.toBeInTheDocument();
  });

  it("SET-FR-FZKL: states a project that does not use OpenSpec and an inactive mode", async () => {
    status = { setting: "auto", usesOpenspec: false, active: false, cli: AVAILABLE };
    render(<OpenSpecSection />);
    expect(await screen.findByText(/This project does not use OpenSpec/)).toBeInTheDocument();
    expect(screen.getByText("OpenSpec mode is not active.")).toBeInTheDocument();
  });

  it("SET-FR-FZKL: states each CLI state in words", () => {
    expect(cliStatement({ state: "not_found" })).toBe("The OpenSpec CLI is not installed on this machine.");
    expect(cliStatement({ state: "unsupported", path: "/x/openspec", version: "0.9.1" })).toBe(
      "OpenSpec CLI 0.9.1 at /x/openspec is a version that is not supported.",
    );
    expect(cliStatement({ state: "failed", path: "/x/openspec", reason: "timed_out" })).toContain(
      "did not answer in time",
    );
    expect(cliStatement({ state: "failed", path: "/x/openspec", reason: "not_executable" })).toContain(
      "could not be started",
    );
    expect(cliStatement({ state: "failed", path: "/x/openspec", reason: "no_version" })).toContain(
      "did not report a version",
    );
  });

  it("SET-FR-QVUP: the checkbox is selected while the setting is auto", async () => {
    render(<OpenSpecSection />);
    const box = await screen.findByRole("checkbox", { name: "Use OpenSpec mode" });
    await waitFor(() => expect(box).toBeChecked());

    cleanup();
    status = { ...status, setting: "off", active: false };
    render(<OpenSpecSection />);
    const off = await screen.findByRole("checkbox", { name: "Use OpenSpec mode" });
    await waitFor(() => expect(off).not.toBeChecked());
  });

  it("SET-FR-QVUP, PSS-FR-HVUZ: a change persists at once with openspecMode and the stored line endings, then reads the status again", async () => {
    const user = userEvent.setup();
    render(<OpenSpecSection />);
    const box = await screen.findByRole("checkbox", { name: "Use OpenSpec mode" });
    await waitFor(() => expect(box).toBeChecked());

    await user.click(box);
    await waitFor(() => expect(box).not.toBeChecked());
    expect(calls("save_project_config")).toEqual([["save_project_config", { config: { lineEndings: "crlf", openspecMode: "off" } }]]);
    expect(calls("get_openspec_status")).toHaveLength(2);
    expect(await screen.findByText("OpenSpec mode is not active.")).toBeInTheDocument();

    await user.click(box);
    await waitFor(() => expect(box).toBeChecked());
    const saves = calls("save_project_config");
    expect(saves[saves.length - 1]).toEqual(["save_project_config", { config: { lineEndings: "crlf", openspecMode: "auto" } }]);
  });

  it("SET-FR-QVUP: the checkbox keeps the focus across a write, so the keyboard toggles it again", async () => {
    const user = userEvent.setup();
    render(<OpenSpecSection />);
    const box = await screen.findByRole("checkbox", { name: "Use OpenSpec mode" });
    await waitFor(() => expect(box).toBeChecked());
    box.focus();
    await user.keyboard(" ");
    await waitFor(() => expect(box).not.toBeChecked());
    expect(box).toHaveFocus();
    expect(box).not.toBeDisabled();
    await user.keyboard(" ");
    await waitFor(() => expect(box).toBeChecked());
    expect(calls("save_project_config")).toHaveLength(2);
  });

  it("SET-FR-QVUP: a second change while a write is in flight is ignored", async () => {
    const user = userEvent.setup();
    let release: () => void = () => {};
    render(<OpenSpecSection />);
    const box = await screen.findByRole("checkbox", { name: "Use OpenSpec mode" });
    await waitFor(() => expect(box).toBeChecked());
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: unknown) =>
      cmd === "save_project_config"
        ? new Promise<void>((resolve) => {
            release = () => resolve(base(cmd, args));
          })
        : base(cmd, args),
    );
    await user.click(box);
    await waitFor(() => expect(box).toHaveAttribute("aria-busy", "true"));
    await user.click(box);
    expect(calls("save_project_config")).toHaveLength(1);
    release();
    await waitFor(() => expect(box).not.toHaveAttribute("aria-busy"));
  });

  it("SET-FR-QVUP: a failed write shows its error inline and keeps the earlier state", async () => {
    const user = userEvent.setup();
    refuseSave = "openspec_mode_invalid";
    render(<OpenSpecSection />);
    const box = await screen.findByRole("checkbox", { name: "Use OpenSpec mode" });
    await waitFor(() => expect(box).toBeChecked());

    await user.click(box);
    expect(await screen.findByRole("alert")).toHaveTextContent("openspec_mode_invalid");
    expect(box).toBeChecked();
    expect(screen.getByText("OpenSpec mode is active.")).toBeInTheDocument();
    expect(calls("get_openspec_status")).toHaveLength(1);
  });

  it("SET-FR-ABXE: a CLI that is not found or not supported shows what the mode needs and the install command", async () => {
    for (const cli of [{ state: "not_found" }, { state: "unsupported", path: "/x/openspec", version: "0.9.1" }] as OpenSpecCliState[]) {
      cleanup();
      status = { setting: "auto", usesOpenspec: true, active: true, cli };
      render(<OpenSpecSection />);
      const install = await screen.findByTestId("openspec-install");
      expect(install).toHaveTextContent("OpenSpec mode needs OpenSpec CLI 1.0 or later.");
      expect(install).toHaveTextContent(OPENSPEC_INSTALL_COMMAND);
      expect(OPENSPEC_INSTALL_COMMAND).toBe("npm install -g @fission-ai/openspec");
    }
  });

  it("SET-FR-ABXE: a failed CLI shows no install command", async () => {
    status = { setting: "auto", usesOpenspec: true, active: true, cli: { state: "failed", path: "/x", reason: "no_version" } };
    render(<OpenSpecSection />);
    await screen.findByTestId("openspec-cli");
    expect(screen.queryByTestId("openspec-install")).not.toBeInTheDocument();
  });

  it("SET-FR-FZKL: a status that cannot be read shows the error and keeps the checkbox disabled", async () => {
    refuseRead = "no project open";
    render(<OpenSpecSection />);
    expect(await screen.findByRole("alert")).toHaveTextContent("no project open");
    expect(screen.getByRole("checkbox", { name: "Use OpenSpec mode" })).toBeDisabled();
  });
});
