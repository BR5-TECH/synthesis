/**
 * The OpenSpec failure notice
 * (`../../specifications/ui/OFN-openspec-failure-notices.md`).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const logWarn = vi.fn();
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: (...args: unknown[]) => logWarn(...args),
  logError: vi.fn(),
}));

import { COPIED_STATEMENT_MS, OpenSpecFailureNotice } from "./OpenSpecFailureNotice";
import { configureOpenSpecChangeRoutes } from "../state/openspec/changeRoutes";
import { configureRunRoute } from "../state/graduation/runRoute";
import type { OpenSpecFailure } from "../types";

/** The commands this file does not expect, each answered with a rejection. */
let unknown: string[];
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const openChange = vi.fn();
const openRun = vi.fn();

const f = (over: Partial<OpenSpecFailure> & { code: string }): OpenSpecFailure => ({
  diagnostics: [],
  ...over,
});

beforeEach(() => {
  unknown = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records" || cmd === "open_settings_window") return undefined;
    if (cmd === "get_openspec_status") {
      return { setting: "auto", usesOpenspec: true, active: true, cli: { state: "not_found" } };
    }
    unknown.push(cmd);
    throw new Error(`unexpected invoke ${cmd}`);
  });
  logWarn.mockReset();
  openChange.mockReset();
  openRun.mockReset();
  configureOpenSpecChangeRoutes({ open: openChange });
  configureRunRoute(openRun);
});
afterEach(() => {
  cleanup();
  configureOpenSpecChangeRoutes(null);
  configureRunRoute(null);
  vi.useRealTimers();
  Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
  expect(unknown).toEqual([]);
});

describe("the sentence and its announcement", () => {
  it("OFN-FR-RNWG, OFN-FR-ONNF: the sentence of the classifying code stands in one live region, with the frame", () => {
    render(
      <OpenSpecFailureNotice
        failure={f({ code: "apply_progress_unavailable", cause: "cli_timed_out" })}
        surface="test"
        frame="No run started."
      />,
    );
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0].textContent).toBe(
      "The OpenSpec CLI did not finish in time, also after one more try. No run started.",
    );
  });

  it("OFN-FR-PVYE: a code that the table does not name renders nothing", () => {
    const { container } = render(
      <OpenSpecFailureNotice failure={f({ code: "change_not_found" })} surface="test" />,
    );
    expect(container).toBeEmptyDOMElement();
    expect(logWarn).not.toHaveBeenCalled();
  });

  it("OFN-FR-LKAT: a notice that appears emits one WARN record, and a render of the same notice emits none", () => {
    const failure = f({ code: "cli_rejected", change: "add-logout", diagnostics: [{ message: "private" }] });
    const view = render(<OpenSpecFailureNotice failure={failure} surface="test" />);
    view.rerender(<OpenSpecFailureNotice failure={failure} surface="test" frame="x" />);
    expect(logWarn).toHaveBeenCalledTimes(1);
    expect(logWarn.mock.calls[0][2]).toEqual({ surface: "test", code: "cli_rejected", cause: "", class: "content" });
    view.rerender(<OpenSpecFailureNotice failure={{ ...failure }} surface="test" />);
    expect(logWarn).toHaveBeenCalledTimes(2);
  });
});

describe("the actions of each class", () => {
  it("OFN-FR-EPIG: an environment notice opens the OpenSpec section of Project settings", async () => {
    render(<OpenSpecFailureNotice failure={f({ code: "cli_unavailable", detail: "not_found" })} surface="test" />);
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open OpenSpec settings" }));
    await waitFor(() =>
      expect(calls("open_settings_window")[0][1]).toEqual({ section: "project/openspec" }),
    );
  });

  it("OFN-FR-EPIG: an image notice opens the Docker section in its place", async () => {
    render(
      <OpenSpecFailureNotice failure={f({ code: "openspec_image_cli_unavailable", detail: "failed" })} surface="test" />,
    );
    expect(screen.queryByRole("button", { name: "Open OpenSpec settings" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open Docker settings" }));
    await waitFor(() =>
      expect(calls("open_settings_window")[0][1]).toEqual({ section: "project/docker" }),
    );
  });

  it("OFN-FR-CBLG: Retry re-checks the CLI, then runs the failed operation again, and is disabled while the two calls are in flight", async () => {
    const order: string[] = [];
    let releaseStatus: () => void = () => {};
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "append_log_records") return Promise.resolve();
      if (cmd === "get_openspec_status") {
        order.push(`status ${JSON.stringify(args)}`);
        return new Promise((resolve) => {
          releaseStatus = () => resolve({ active: true });
        });
      }
      unknown.push(cmd);
      return Promise.reject(new Error(cmd));
    });
    let releaseRetry: () => void = () => {};
    const onRetry = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          order.push("operation");
          releaseRetry = resolve;
        }),
    );
    render(<OpenSpecFailureNotice failure={f({ code: "cli_failed" })} surface="test" onRetry={onRetry} />);
    const retry = screen.getByRole("button", { name: "Retry" });
    // By keyboard: the button keeps the focus while it is disabled.
    retry.focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(retry).toHaveAttribute("aria-disabled", "true"));
    expect(document.activeElement).toBe(retry);
    expect(order).toEqual(['status {"recheck":true}']);
    expect(onRetry).not.toHaveBeenCalled();
    // A second press while disabled does nothing.
    await userEvent.click(retry);
    expect(order).toEqual(['status {"recheck":true}']);
    await act(async () => releaseStatus());
    await waitFor(() => expect(onRetry).toHaveBeenCalledTimes(1));
    expect(order).toEqual(['status {"recheck":true}', "operation"]);
    expect(retry).toHaveAttribute("aria-disabled", "true");
    await act(async () => releaseRetry());
    await waitFor(() => expect(retry).not.toHaveAttribute("aria-disabled"));
    expect(document.activeElement).toBe(retry);
  });

  it("OFN-FR-CBLG: a re-check that fails still runs the operation again", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return undefined;
      if (cmd === "get_openspec_status") throw new Error("no_open_project");
      unknown.push(cmd);
      throw new Error(cmd);
    });
    const onRetry = vi.fn(async () => {});
    render(<OpenSpecFailureNotice failure={f({ code: "cli_timed_out" })} surface="test" onRetry={onRetry} />);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(onRetry).toHaveBeenCalledTimes(1));
  });

  it("OFN-FR-DPIJ: a surface with its own retry gives none, so the notice offers no Retry", () => {
    render(<OpenSpecFailureNotice failure={f({ code: "cli_failed" })} surface="test" onRetry={null} />);
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });

  it("OFN-FR-RLSO: a content notice opens the change in the location of the failure and selects its thread", async () => {
    render(
      <OpenSpecFailureNotice
        failure={f({ code: "archive_failed", cause: "archive_validation_failed", change: "add-logout", threadId: "t-3" })}
        surface="test"
        location={{ kind: "stream", streamId: "s-1" }}
        streamName="auth-work"
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Open change" }));
    expect(openChange).toHaveBeenCalledWith({ kind: "stream", streamId: "s-1" }, "add-logout", {
      thread: "t-3",
      streamName: "auth-work",
    });
  });

  it("OFN-FR-RLSO: a notice on the tab of that change, with no location, offers no Open change", () => {
    render(<OpenSpecFailureNotice failure={f({ code: "cli_rejected", change: "c" })} surface="test" />);
    expect(screen.queryByRole("button", { name: "Open change" })).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("OFN-FR-RCWY: a repair run is offered as Open run, which opens that run", async () => {
    render(
      <OpenSpecFailureNotice
        failure={f({ code: "archive_failed", cause: "archive_spec_update_failed", change: "c", repairRunId: "g-12" })}
        surface="test"
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("A rework run started to repair the change.");
    await userEvent.click(screen.getByRole("button", { name: "Open run" }));
    expect(openRun).toHaveBeenCalledWith("g-12");
  });

  it("SMP-FR-KVKD: a notice with no actions shows its sentence and Details alone", () => {
    render(
      <OpenSpecFailureNotice
        failure={f({ code: "cli_unavailable", detail: "failed", diagnostics: [{ message: "m" }] })}
        surface="test"
        actions={false}
      />,
    );
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual(["Details ▾"]);
  });
});

describe("Details", () => {
  const diagnostics = [
    { severity: "error", code: "archive_spec_update_failed", message: 'MODIFIED "X" not found', fix: "Use ADDED" },
    { severity: "warning", path: "specs/auth/spec.md", message: "<b>bold</b> **strong** [link](https://x.test)" },
    { message: "a\u0007bell" },
  ];

  it("OFN-FR-IHSQ, OFN-FR-ONNF: Details is a closed disclosure that carries its state, and lists each diagnostic on its own line", async () => {
    render(<OpenSpecFailureNotice failure={f({ code: "cli_rejected", diagnostics })} surface="test" />);
    const toggle = screen.getByRole("button", { name: /Details/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    // The list is not mounted while closed, so the button names no element.
    expect(toggle).not.toHaveAttribute("aria-controls");
    expect(screen.queryByTestId("openspec-failure-details")).toBeNull();
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAttribute("aria-controls", screen.getByTestId("openspec-failure-details").id);
    const items = screen.getAllByTestId("openspec-failure-diagnostic");
    expect(items.map((item) => [...item.querySelectorAll(".ofn__line")].map((l) => l.textContent))).toEqual([
      ['error archive_spec_update_failed: MODIFIED "X" not found', "Fix: Use ADDED"],
      ["warning specs/auth/spec.md: <b>bold</b> **strong** [link](https://x.test)"],
      ["a bell"],
    ]);
    // Each line is a block, and in text form the fix stands on its own line.
    expect([...items[0].querySelectorAll(".ofn__line")].map((l) => l.tagName)).toEqual(["DIV", "DIV"]);
    expect(items[0].textContent).toBe('error archive_spec_update_failed: MODIFIED "X" not found\nFix: Use ADDED');
    expect(items[1].textContent).toBe("warning specs/auth/spec.md: <b>bold</b> **strong** [link](https://x.test)");
    // Opening Details adds nothing to the live region, also inside a host
    // region such as the comment log of a discussion.
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByTestId("openspec-failure-details")).toHaveAttribute("aria-live", "off");
    await userEvent.click(toggle);
    expect(screen.queryByTestId("openspec-failure-details")).toBeNull();
    expect(toggle).not.toHaveAttribute("aria-controls");
  });

  it("OFN-FR-UNEF: Details renders no Markdown, no HTML, and no link", async () => {
    render(<OpenSpecFailureNotice failure={f({ code: "cli_rejected", diagnostics })} surface="test" />);
    await userEvent.click(screen.getByRole("button", { name: /Details/ }));
    const details = screen.getByTestId("openspec-failure-details");
    expect(details.querySelector("b, strong, a, em")).toBeNull();
    expect(within(details).queryByRole("link")).toBeNull();
    expect(details.textContent).toContain("<b>bold</b> **strong**");
  });

  it("OFN-FR-IHSQ: a failure with no diagnostic offers no Details", () => {
    render(<OpenSpecFailureNotice failure={f({ code: "cli_timed_out" })} surface="test" onRetry={vi.fn()} />);
    expect(screen.queryByRole("button", { name: /Details/ })).toBeNull();
  });

  it("OFN-FR-DZSQ: Copy writes every diagnostic as plain text and states for 2 seconds that the text is copied", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    vi.useFakeTimers();
    render(<OpenSpecFailureNotice failure={f({ code: "cli_rejected", diagnostics })} surface="test" />);
    fireEvent.click(screen.getByRole("button", { name: /Details/ }));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    });
    expect(writeText).toHaveBeenCalledWith(
      'error archive_spec_update_failed: MODIFIED "X" not found\nFix: Use ADDED\n' +
        "warning specs/auth/spec.md: <b>bold</b> **strong** [link](https://x.test)\na bell",
    );
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Copied.");
    act(() => vi.advanceTimersByTime(COPIED_STATEMENT_MS - 1));
    expect(status).toHaveTextContent("Copied.");
    act(() => vi.advanceTimersByTime(1));
    expect(status).toHaveTextContent("");
  });

  it("OFN-FR-DZSQ: with no clipboard, Copy states nothing and logs a WARN record with no diagnostic", async () => {
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    render(<OpenSpecFailureNotice failure={f({ code: "cli_rejected", diagnostics })} surface="test" />);
    fireEvent.click(screen.getByRole("button", { name: /Details/ }));
    logWarn.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "the openspec diagnostics could not be copied", {
        surface: "test",
        reason: "unavailable",
      }),
    );
    expect(screen.getByRole("status")).toHaveTextContent("");
    expect(JSON.stringify(logWarn.mock.calls)).not.toContain("MODIFIED");
  });

  it("OFN-FR-DZSQ: a rejected write states nothing and logs a WARN record with the reason name alone", async () => {
    const writeText = vi.fn(() => Promise.reject(Object.assign(new Error("private text"), { name: "NotAllowedError" })));
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<OpenSpecFailureNotice failure={f({ code: "cli_rejected", diagnostics })} surface="test" />);
    fireEvent.click(screen.getByRole("button", { name: /Details/ }));
    logWarn.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    await waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "the openspec diagnostics could not be copied", {
        surface: "test",
        reason: "NotAllowedError",
      }),
    );
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status")).toHaveTextContent("");
    expect(JSON.stringify(logWarn.mock.calls)).not.toContain("private text");
  });

  it("OFN-FR-ONNF: every action is reachable with the keyboard alone", async () => {
    const user = userEvent.setup();
    render(
      <OpenSpecFailureNotice
        failure={f({ code: "archive_failed", cause: "archive_error", change: "c", repairRunId: "g-1", diagnostics })}
        surface="test"
        location={{ kind: "stream", streamId: "s-1" }}
      />,
    );
    await user.tab();
    expect(screen.getByRole("button", { name: "Open change" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Open run" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: /Details/ })).toHaveFocus();
    await user.keyboard("{Enter}");
    await user.tab();
    expect(screen.getByRole("button", { name: "Copy" })).toHaveFocus();
  });
});
