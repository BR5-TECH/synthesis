/**
 * The pull request notice (CPR-FR-ITWJ, CPR-FR-RDJP).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { PullRequestNotice } from "./PullRequestNotice";
import { resetLogBufferForTest } from "../logging";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const openUrlMock = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));

const NOTICE = { number: 42, url: "https://github.com/acme/platform/pull/42" };

beforeEach(() => {
  resetLogBufferForTest();
  openUrlMock.mockReset();
  openUrlMock.mockResolvedValue(undefined);
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === "append_log_records"
      ? undefined
      : Promise.reject(new Error(`unexpected command ${cmd}`)),
  );
});
afterEach(cleanup);

describe("the pull request notice", () => {
  it("CPR-FR-ITWJ: names the pull request by its number and is announced as a status", () => {
    render(<PullRequestNotice notice={NOTICE} onDismiss={() => {}} />);
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Pull request #42 created");
    expect(screen.getByTestId("pull-request-notice-link")).toHaveAttribute("href", NOTICE.url);
  });

  it("CPR-FR-ITWJ: the link opens the page through the application's opener", async () => {
    render(<PullRequestNotice notice={NOTICE} onDismiss={() => {}} />);
    await userEvent.click(screen.getByTestId("pull-request-notice-link"));
    await waitFor(() => expect(openUrlMock).toHaveBeenCalledWith(NOTICE.url));
  });

  it("CPR-FR-ITWJ: it is dismissible and renders nothing without a notice", async () => {
    const onDismiss = vi.fn();
    const { rerender } = render(<PullRequestNotice notice={NOTICE} onDismiss={onDismiss} />);
    await userEvent.click(screen.getByTestId("pull-request-notice-dismiss"));
    expect(onDismiss).toHaveBeenCalledTimes(1);
    rerender(<PullRequestNotice notice={null} onDismiss={onDismiss} />);
    expect(screen.queryByTestId("pull-request-notice")).toBeNull();
  });

  it("CPR-FR-RDJP: it takes no focus", () => {
    render(<PullRequestNotice notice={NOTICE} onDismiss={() => {}} />);
    expect(screen.getByTestId("pull-request-notice")).not.toHaveFocus();
    expect(document.body).toHaveFocus();
  });
});
