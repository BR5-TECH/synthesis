/**
 * The shell's half of the Create a PR window
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-IWDK,
 * CPR-FR-XMRL, CPR-FR-VZUZ, CPR-FR-ITWJ, CPR-FR-RDJP,
 * `SNV-shell-navigation.md` SNV-FR-56).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import { usePullRequestWindow } from "./usePullRequestWindow";
import type { PullRequestSource } from "../components/CreatePullRequest/types";
import { resetLogBufferForTest } from "../logging";
import { clearToasts, visibleToasts } from "../state/toasts";

const invokeMock = vi.fn();
const openUrlMock = vi.fn(async (_url: string) => {});
const logWarnMock = vi.fn();
vi.mock("../logging", async (original) => ({
  ...(await original<typeof import("../logging")>()),
  logWarn: (...args: unknown[]) => logWarnMock(...args),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrlMock(url),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const SOURCE: PullRequestSource = { head: "feature", base: "main", title: "Feature" };
const INPUT = { title: "T", body: "B", base: "main", draft: false };

beforeEach(() => {
  clearToasts();
  openUrlMock.mockClear();
  logWarnMock.mockClear();
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === "append_log_records" ? undefined : Promise.reject(new Error(`unexpected ${cmd}`)),
  );
});

function setup() {
  const closeOthers = vi.fn();
  let settlePicker: (chosen: boolean) => void = () => {};
  const requestToken = vi.fn(
    () => new Promise<boolean>((resolve) => (settlePicker = resolve)),
  );
  const view = renderHook(
    ({ projectPath }) =>
      usePullRequestWindow({ closeOthers, requestToken, projectPath }),
    { initialProps: { projectPath: "/p" } },
  );
  return { ...view, closeOthers, requestToken, settle: (c: boolean) => settlePicker(c) };
}

describe("opening and closing", () => {
  it("CPR-FR-IWDK, SNV-FR-56: opening closes every other overlay and mounts the window for the source", () => {
    const { result, closeOthers } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    expect(closeOthers).toHaveBeenCalledTimes(1);
    expect(result.current.pullRequestWindow?.source).toEqual(SOURCE);
    expect(result.current.pullRequestWindow?.resume).toBeNull();
  });

  it("SNV-FR-56: another overlay opening takes the window down", () => {
    const { result } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    act(() => result.current.dismissPullRequestWindow());
    expect(result.current.pullRequestWindow).toBeNull();
  });

  it("CPR-FR-SSQI: closing discards the window, so opening again starts from the source", () => {
    const { result } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    const first = result.current.pullRequestWindow!.nonce;
    act(() => result.current.closePullRequestWindow());
    expect(result.current.pullRequestWindow).toBeNull();
    act(() => result.current.openPullRequestWindow(SOURCE));
    expect(result.current.pullRequestWindow!.nonce).not.toBe(first);
  });

  it("CPR-FR-XMRL: while a request runs neither a close nor another overlay takes the window down", () => {
    const { result } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    result.current.pullRequestSubmittingRef.current = true;
    act(() => result.current.closePullRequestWindow());
    act(() => result.current.dismissPullRequestWindow());
    expect(result.current.pullRequestWindow).not.toBeNull();
  });
});

describe("the token picker (CPR-FR-VZUZ)", () => {
  it("gives way to the picker, then returns with the held input and submits once when a token was chosen", async () => {
    const { result, requestToken, settle } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    let pending: Promise<void> = Promise.resolve();
    act(() => {
      pending = result.current.giveWayToPicker(INPUT);
    });
    expect(requestToken).toHaveBeenCalledTimes(1);
    // The picker stands in the window's place.
    expect(result.current.pullRequestWindow).toBeNull();
    await act(async () => {
      settle(true);
      await pending;
    });
    expect(result.current.pullRequestWindow?.resume).toEqual({
      input: INPUT,
      submit: true,
      notice: null,
    });
  });

  it("returns with the held input and an inline notice, and no submission, when the picker was cancelled", async () => {
    const { result, settle } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    let pending: Promise<void> = Promise.resolve();
    act(() => {
      pending = result.current.giveWayToPicker(INPUT);
    });
    await act(async () => {
      settle(false);
      await pending;
    });
    const resume = result.current.pullRequestWindow?.resume;
    expect(resume?.submit).toBe(false);
    expect(resume?.input).toEqual(INPUT);
    expect(resume?.notice).toMatch(/No GitHub token was selected/);
  });

  it("SNV-FR-56: another overlay taking the picker's place clears the record, so the window does not come back", async () => {
    const { result, settle } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    let pending: Promise<void> = Promise.resolve();
    act(() => {
      pending = result.current.giveWayToPicker(INPUT);
    });
    // Some other opener closes the picker and the record with it.
    act(() => result.current.dismissPullRequestWindow());
    await act(async () => {
      settle(false);
      await pending;
    });
    expect(result.current.pullRequestWindow).toBeNull();
  });

  it("the picker's own opening does not clear the record, and a later dismissal does", async () => {
    const closeOthers = vi.fn();
    let settlePicker: (chosen: boolean) => void = () => {};
    const view = renderHook(() => {
      const hook = usePullRequestWindow({
        closeOthers,
        // The shell's picker opener dismisses the window while it opens.
        requestToken: () => {
          hook.dismissPullRequestWindow();
          return new Promise<boolean>((resolve) => (settlePicker = resolve));
        },
        projectPath: "/p",
      });
      return hook;
    });
    act(() => view.result.current.openPullRequestWindow(SOURCE));
    let pending: Promise<void> = Promise.resolve();
    act(() => {
      pending = view.result.current.giveWayToPicker(INPUT);
    });
    await act(async () => {
      settlePicker(true);
      await pending;
    });
    expect(view.result.current.pullRequestWindow?.resume?.submit).toBe(true);

    // After the hook stopped yielding, a dismissal clears the window.
    act(() => view.result.current.dismissPullRequestWindow());
    expect(view.result.current.pullRequestWindow).toBeNull();
  });
});

describe("a picker that cannot open (CPR-FR-VZUZ)", () => {
  it("does not leave every later dismissal ignored", async () => {
    const closeOthers = vi.fn();
    const view = renderHook(() => {
      const hook = usePullRequestWindow({
        closeOthers,
        requestToken: () => {
          throw new Error("the picker did not open");
        },
        projectPath: "/p",
      });
      return hook;
    });
    act(() => view.result.current.openPullRequestWindow(SOURCE));
    await act(async () => {
      await expect(view.result.current.giveWayToPicker(INPUT)).rejects.toThrow(
        "the picker did not open",
      );
    });
    act(() => view.result.current.dismissPullRequestWindow());
    expect(view.result.current.pullRequestWindow).toBeNull();
  });
});

describe("the opener (CPR-FR-IWDK)", () => {
  it("keeps the control that opened the window, so focus can return to it", () => {
    const { result } = setup();
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    act(() => result.current.openPullRequestWindow(SOURCE));
    expect(result.current.pullRequestWindow?.opener).toBe(opener);
    opener.remove();
  });
});

describe("the toast of a created pull request (CPR-FR-ITWJ, CPR-FR-RDJP)", () => {
  const created = () => visibleToasts().find((t) => t.key === "pull-request-created");

  it("CPR-FR-ITWJ, NTF-FR-PRCK: a created pull request closes the window and shows an Info toast with a link to it", () => {
    const { result } = setup();
    act(() => result.current.openPullRequestWindow(SOURCE));
    act(() =>
      result.current.pullRequestCreated({ number: 5, url: "https://github.com/a/b/pull/5" }),
    );
    expect(result.current.pullRequestWindow).toBeNull();
    expect(created()).toMatchObject({
      level: "Info",
      title: "Pull request created",
      body: "Pull request #5 is created.",
      address: null,
    });
    expect(created()?.link?.label).toBe("Open pull request #5");
    created()?.link?.onActivate();
    expect(openUrlMock).toHaveBeenCalledWith("https://github.com/a/b/pull/5");
  });

  it("CPR-FR-RDJP: the toast is no overlay, so a window that opens and closes after it leaves it", () => {
    const { result } = setup();
    act(() =>
      result.current.pullRequestCreated({ number: 5, url: "https://github.com/a/b/pull/5" }),
    );
    act(() => result.current.openPullRequestWindow(SOURCE));
    expect(result.current.pullRequestWindow).not.toBeNull();
    act(() => result.current.dismissPullRequestWindow());
    expect(result.current.pullRequestWindow).toBeNull();
    expect(created()).toBeDefined();
  });

  it("NTF-FR-PRCK: a second pull request replaces the toast of the first", () => {
    const { result } = setup();
    act(() =>
      result.current.pullRequestCreated({ number: 5, url: "https://github.com/a/b/pull/5" }),
    );
    act(() =>
      result.current.pullRequestCreated({ number: 6, url: "https://github.com/a/b/pull/6" }),
    );
    const all = visibleToasts().filter((t) => t.key === "pull-request-created");
    expect(all).toHaveLength(1);
    expect(all[0].body).toBe("Pull request #6 is created.");
  });

  it("NTF-FR-PRCK: a link that does not open is logged and leaves no unhandled error", async () => {
    const { result } = setup();
    act(() =>
      result.current.pullRequestCreated({ number: 5, url: "https://github.com/a/b/pull/5" }),
    );
    openUrlMock.mockRejectedValueOnce(new Error("opener refused"));
    created()?.link?.onActivate();
    await vi.waitFor(() =>
      expect(logWarnMock).toHaveBeenCalledWith(
        ["frontend", "remote"],
        "pull request link did not open",
        { number: 5, error: "Error: opener refused" },
      ),
    );
    expect(created()).toBeDefined();
  });
});
