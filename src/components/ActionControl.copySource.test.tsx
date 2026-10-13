/**
 * **Copy as Source** in the action control (`ACT-action-control.md`
 * ACT-FR-03, ACT-FR-04, ACT-FR-05, ACT-FR-07, ACT-FR-HPIX).
 *
 * The control is mounted alone here, because what these tests check is the
 * control's own states: the **Copied** interval, the collapse, and the
 * single-flight. What the copy writes is the New Artifact tab's, and
 * `NewArtifactWorkspace.copySource.test.tsx` covers it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) =>
    cmd === "append_log_records"
      ? Promise.resolve()
      : Promise.reject(new Error(`unexpected command ${cmd}`)),
}));

import { ActionControl, COPIED_LABEL_MS } from "./ActionControl";
import type { HostAction, HostCopyAction } from "./ActionControl";
import type { UseDiscussionsResult } from "../hooks/useDiscussions";
import { resetLogBufferForTest } from "../logging";
import { resetDiscussionFocus } from "../state/discussionFocus";

/** No discussion, no agent: Discuss is not what these tests are about. */
function discussions(): UseDiscussionsResult {
  return {
    discussions: [],
    agents: [],
    pendingTurns: [],
    turnFailures: {},
    failedTurns: {},
    imageNotices: {},
    retryingTurnIds: new Set(),
    retryTurn: async () => {},
    cancelTurn: async () => {},
    identity: null,
    identityError: null,
    retryIdentity: async () => {},
    unresolved: 0,
    errors: {},
    open: async () => {
      throw new Error("not used");
    },
    reply: async () => {},
    setLock: async () => {},
    setResolved: async () => {},
  };
}

function host(label: string): HostAction {
  return { label, onActivate: vi.fn() };
}

function renderControl(
  copySource: HostCopyAction,
  extra: { missing?: boolean; readOnly?: boolean } = {},
) {
  return render(
    <div>
      <p data-testid="outside">The page</p>
      <ActionControl
        discussions={discussions()}
        controlLabel="Draft actions"
        itemNoun="draft"
        discussionsInRail
        graduate={host("Graduate")}
        copySource={copySource}
        publish={host("Publish to GitHub")}
        archive={host("Archive")}
        {...extra}
      />
    </div>,
  );
}

function expand() {
  fireEvent.click(screen.getByRole("button", { name: "Draft actions" }));
  return screen.getByRole("menu");
}

/** Lets the copy's promise settle and React render what it set. */
async function settle() {
  await act(async () => {});
}

beforeEach(() => {
  resetLogBufferForTest();
  resetDiscussionFocus();
  vi.useFakeTimers();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Copy as Source in the action control", () => {
  it("ACT-FR-04, ACT-FR-05: the entries read Discuss, Graduate, Copy as Source, Publish to GitHub, Archive", () => {
    renderControl({ label: "Copy as Source", onActivate: async () => true });
    expand();
    expect(
      screen.getAllByRole("menuitem").map((e) => e.textContent?.trim()),
    ).toEqual([
      "Discuss",
      "Graduate",
      "Copy as Source",
      "Publish to GitHub",
      "Archive",
    ]);
    // ACT-FR-05: a named entry with an icon, like the others.
    const entry = screen.getByRole("menuitem", { name: "Copy as Source" });
    expect(entry.querySelector("svg")).not.toBeNull();
  });

  it("ACT-FR-HPIX, ACT-FR-03: a copy that succeeds reads Copied, is announced, and then collapses the control", async () => {
    const onActivate = vi.fn(async () => true);
    renderControl({ label: "Copy as Source", onActivate });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await settle();
    expect(onActivate).toHaveBeenCalledTimes(1);
    // The control did not collapse the moment the entry was activated.
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Copied" })).toBeInTheDocument();
    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Copied");

    act(() => vi.advanceTimersByTime(COPIED_LABEL_MS - 1));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("");

    // Opened again, the entry reads its own label, not Copied.
    expand();
    expect(
      screen.getByRole("menuitem", { name: "Copy as Source" }),
    ).toBeInTheDocument();
  });

  it("ACT-FR-HPIX: Escape collapses the control at once while it reads Copied", async () => {
    renderControl({ label: "Copy as Source", onActivate: async () => true });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await settle();
    expect(screen.getByRole("menuitem", { name: "Copied" })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    // The interval that was running does nothing more.
    act(() => vi.advanceTimersByTime(COPIED_LABEL_MS));
    expect(screen.queryByRole("menu")).toBeNull();
    // Opened again, the entry reads its own label.
    expand();
    expect(
      screen.getByRole("menuitem", { name: "Copy as Source" }),
    ).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(COPIED_LABEL_MS));
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("ACT-FR-HPIX: a second activation of the control collapses it at once while it reads Copied", async () => {
    renderControl({ label: "Copy as Source", onActivate: async () => true });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await settle();
    expect(screen.getByRole("menuitem", { name: "Copied" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Close draft actions" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("ACT-FR-HPIX: a copy that ends after the control collapsed does not open it again", async () => {
    let finish: (copied: boolean) => void = () => {};
    renderControl({
      label: "Copy as Source",
      onActivate: () => new Promise<boolean>((resolve) => (finish = resolve)),
    });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    await act(async () => finish(true));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("");
    expand();
    expect(
      screen.getByRole("menuitem", { name: "Copy as Source" }),
    ).toBeInTheDocument();
  });

  it("ACT-FR-HPIX: a pointer-down outside collapses the control at once while it reads Copied", async () => {
    renderControl({ label: "Copy as Source", onActivate: async () => true });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await settle();
    expect(screen.getByRole("menuitem", { name: "Copied" })).toBeInTheDocument();
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByRole("menu")).toBeNull();
    act(() => vi.advanceTimersByTime(COPIED_LABEL_MS));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("ACT-FR-HPIX: a copy that fails collapses the control at once and never reads Copied", async () => {
    renderControl({ label: "Copy as Source", onActivate: async () => false });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await settle();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("ACT-FR-HPIX: a copy whose host rejects is a failure too", async () => {
    renderControl({
      label: "Copy as Source",
      onActivate: () => Promise.reject(new Error("refused")),
    });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await settle();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("NAW-FR-QPTN: further activations while the write runs do nothing", async () => {
    let finish: (copied: boolean) => void = () => {};
    const onActivate = vi.fn(
      () => new Promise<boolean>((resolve) => (finish = resolve)),
    );
    renderControl({ label: "Copy as Source", onActivate });
    expand();
    const entry = screen.getByRole("menuitem", { name: "Copy as Source" });
    fireEvent.click(entry);
    fireEvent.click(entry);
    fireEvent.click(entry);
    expect(onActivate).toHaveBeenCalledTimes(1);
    expect(entry).toHaveAttribute("aria-busy", "true");
    await act(async () => finish(true));
    expect(screen.getByRole("menuitem", { name: "Copied" })).toBeInTheDocument();
  });

  it("ACT-FR-HPIX: focus goes back to the control when the Copied interval ends", async () => {
    renderControl({ label: "Copy as Source", onActivate: async () => true });
    expand();
    const entry = screen.getByRole("menuitem", { name: "Copy as Source" });
    entry.focus();
    fireEvent.click(entry);
    await settle();
    act(() => vi.advanceTimersByTime(COPIED_LABEL_MS));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "Draft actions" })).toHaveFocus();
  });

  it("ACT-FR-HPIX: a copy that ends after the column was closed and opened again changes nothing in it", async () => {
    let finish: (copied: boolean) => void = () => {};
    renderControl({
      label: "Copy as Source",
      onActivate: () => new Promise<boolean>((resolve) => (finish = resolve)),
    });
    expand();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expand();
    await act(async () => finish(false));
    // A late failure does not collapse the column now open.
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "Copy as Source" }),
    ).toBeInTheDocument();
  });

  it("ACT-FR-07: an item that no longer resolves disables Copy as Source with every other entry", () => {
    renderControl(
      { label: "Copy as Source", onActivate: async () => true },
      { missing: true },
    );
    expand();
    for (const item of screen.getAllByRole("menuitem")) {
      expect(item).toBeDisabled();
    }
  });

  it("ACT-FR-04: a read-only tab offers no Copy as Source", () => {
    renderControl(
      { label: "Copy as Source", onActivate: async () => true },
      { readOnly: true },
    );
    expand();
    expect(
      screen.getAllByRole("menuitem").map((e) => e.textContent?.trim()),
    ).toEqual(["Discuss"]);
    expect(screen.queryByRole("status")).toBeNull();
  });
});
