/**
 * The History of a change on a stream, its versions, and the comparison with
 * the current change (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-RKCC, OCW-FR-DRQD, OCW-FR-ABAR, OCW-FR-QCSL).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

import {
  CHANGE,
  STREAM_ID,
  changeBackend,
  changeProposal,
  changeVersions,
  drawChange,
  newChangeProposal,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

const versionRows = () => screen.getAllByTestId("openspec-version");

/**
 * Select the first occurrence of a text in an element, then release the
 * mouse on that text, as a person does who selects with the mouse.
 */
function selectAndRelease(el: Element, text: string) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const at = node.textContent?.indexOf(text) ?? -1;
    if (at === -1) continue;
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at + text.length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    const target = node.parentElement!;
    act(() => {
      target.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
    return;
  }
  throw new Error(`text not found: ${text}`);
}

/** Open the change and its History, and wait for the versions. */
async function openHistory() {
  const user = userEvent.setup();
  drawChange();
  await screen.findAllByTestId("openspec-artifact");
  await user.click(screen.getByRole("button", { name: "History" }));
  await waitFor(() => expect(versionRows()).toHaveLength(3));
  return user;
}

describe("History (OCW-FR-RKCC)", () => {
  it("OCW-FR-RKCC: a stream location offers History, which lists the versions newest first with summary, author, time, and file count", async () => {
    await openHistory();
    expect(screen.getByRole("button", { name: "History" })).toHaveAttribute("aria-pressed", "true");
    expect(calls("list_change_versions")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
    const rows = versionRows();
    expect(rows[0]).toHaveTextContent("Apply the edits of p0");
    expect(rows[0]).toHaveTextContent("Ann");
    expect(rows[0]).toHaveTextContent("1 file");
    expect(rows[1]).toHaveTextContent("Add the design");
    expect(rows[1]).toHaveTextContent("Bob");
    expect(rows[1]).toHaveTextContent("2 files");
    expect(rows[2]).toHaveTextContent("Propose add-logout-3f2a");
    expect(rows[2].querySelector(".draft-version__at")).toHaveAttribute("title", "2026-01-01T10:00:00Z");
    expect(screen.queryByTestId("openspec-history-truncated")).toBeNull();
  });

  it("OCW-FR-RKCC: a truncated list states that it is truncated", async () => {
    backend.versions = { ...changeVersions(), truncated: true };
    await openHistory();
    expect(screen.getByTestId("openspec-history-truncated")).toHaveTextContent(
      "Only the 3 newest versions are shown.",
    );
  });

  it("OCW-FR-RKCC: a worktree location offers no History and no Information", async () => {
    drawChange({ location: { kind: "active_worktree" }, streamName: undefined });
    await screen.findAllByTestId("openspec-artifact");
    expect(screen.queryByRole("button", { name: "History" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Information" })).toBeNull();
    expect(calls("list_change_versions")).toHaveLength(0);
  });

  it("OCW-FR-RKCC: a list that fails states it with Try again, which reads again", async () => {
    const user = userEvent.setup();
    backend.versions = { reject: "read_failed" };
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    await user.click(screen.getByRole("button", { name: "History" }));
    const alert = await screen.findByText("The History could not be read.");
    backend.versions = changeVersions();
    await user.click(within(alert.closest("[role='alert']") as HTMLElement).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    expect(screen.queryByText("The History could not be read.")).toBeNull();
  });

  it("OCW-FR-RKCC: an empty list says the branch holds no version yet", async () => {
    const user = userEvent.setup();
    backend.versions = { versions: [], truncated: false };
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("The stream branch holds no version of this change yet.")).toBeInTheDocument();
  });
});

describe("reading again (OCW-FR-QCSL)", () => {
  it("OCW-FR-QCSL: reads nothing while History is closed, and reads when it opens and on Refresh history", async () => {
    const user = await (async () => {
      const u = userEvent.setup();
      drawChange();
      await screen.findAllByTestId("openspec-artifact");
      expect(calls("list_change_versions")).toHaveLength(0);
      await u.click(screen.getByRole("button", { name: "History" }));
      await waitFor(() => expect(calls("list_change_versions")).toHaveLength(1));
      return u;
    })();
    await waitFor(() => expect(screen.getByRole("button", { name: "Refresh history" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Refresh history" }));
    await waitFor(() => expect(calls("list_change_versions")).toHaveLength(2));
  });

  it("OCW-FR-QCSL: reads again on a committed or decided event for its change, and on no other", async () => {
    await openHistory();
    await waitFor(() => expect(listeners.get("new-change-proposals-changed")?.size ?? 0).toBeGreaterThan(0));
    const before = calls("list_change_versions").length;
    act(() => {
      emit("change-edit-proposals-changed", { streamId: STREAM_ID, change: "other", proposal: changeProposal(), cause: "committed" });
      emit("change-edit-proposals-changed", { streamId: STREAM_ID, change: CHANGE, proposal: changeProposal(), cause: "recorded" });
      emit("new-change-proposals-changed", { streamId: "s-2", change: CHANGE, cause: "decided" });
      emit("new-change-proposals-changed", { streamId: STREAM_ID, change: CHANGE, cause: "confirmed" });
    });
    expect(calls("list_change_versions")).toHaveLength(before);
    act(() => {
      emit("change-edit-proposals-changed", { streamId: STREAM_ID, change: CHANGE, proposal: changeProposal(), cause: "committed" });
    });
    await waitFor(() => expect(calls("list_change_versions")).toHaveLength(before + 1));
    act(() => {
      emit("new-change-proposals-changed", {
        streamId: STREAM_ID,
        change: CHANGE,
        proposal: newChangeProposal({ state: "accepted" }),
        cause: "decided",
      });
    });
    await waitFor(() => expect(calls("list_change_versions")).toHaveLength(before + 2));
  });
});

describe("a version (OCW-FR-DRQD)", () => {
  it("OCW-FR-DRQD: selecting a version reads it and shows it with a banner, its own artifact list, and Rich and Source", async () => {
    const user = await openHistory();
    await user.click(versionRows()[2]);
    await waitFor(() => expect(calls("read_change_version")).toHaveLength(1));
    expect(calls("read_change_version")[0][1]).toEqual({
      streamId: STREAM_ID,
      change: CHANGE,
      commitId: changeVersions().versions[2].commitId,
    });
    const banner = await screen.findByTestId("openspec-version-banner");
    expect(banner).toHaveTextContent("Version c111111");
    expect(banner).toHaveTextContent("Propose add-logout-3f2a");
    expect(banner).toHaveTextContent("read-only");
    expect(versionRows()[2]).toHaveAttribute("aria-current", "true");

    const nav = screen.getByRole("navigation", { name: "Artifacts of this version" });
    const items = within(nav).getAllByTestId("openspec-artifact");
    // The artifacts of the schema in order, and the files no artifact claims.
    expect(items.map((b) => b.querySelector(".openspec-change__item-id")?.textContent)).toEqual([
      "proposal",
      "specs",
      "design",
      "tasks",
      "other files",
    ]);
    expect(items[0]).toHaveTextContent("1 file");
    // The first artifact that holds a file shows first, in Rich.
    const file = await screen.findByTestId("openspec-version-file");
    expect(file).toHaveAttribute("data-path", "proposal.md");
    expect(within(file).getByRole("heading", { name: "Why" })).toBeInTheDocument();
    await user.click(within(file).getByRole("button", { name: "Source" }));
    expect(within(file).getByTestId("openspec-source-text").textContent).toBe("# Why\n\nUsers cannot leave.\n");

    // A file that is not text in this version says so.
    await user.click(items[4]);
    expect(await screen.findByTestId("openspec-unreadable")).toHaveTextContent("This file is not text");
    // The specs artifact holds no file in this version.
    await user.click(items[1]);
    expect(await screen.findByTestId("openspec-no-file")).toHaveTextContent("This version holds no file for specs.");
    await user.click(items[3]);
    expect((await screen.findByTestId("openspec-version-file")).getAttribute("data-path")).toBe("tasks.md");
  });

  it("OCW-FR-DRQD: while a version is shown no decision, no fragment discussion, and no edit is offered", async () => {
    backend.proposals = [changeProposal()];
    const user = userEvent.setup();
    drawChange({ file: "proposal.md" });
    await screen.findByTestId("openspec-review-bar");
    // Control: in the current change the same gesture offers a discussion.
    await user.click(screen.getByRole("button", { name: "Source" }));
    selectAndRelease(screen.getByTestId("openspec-source"), "Why");
    expect(await screen.findByRole("button", { name: "Discuss selection" })).toBeInTheDocument();
    window.getSelection()!.removeAllRanges();

    await user.click(screen.getByRole("button", { name: "History" }));
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await user.click(versionRows()[0]);
    await screen.findByTestId("openspec-version-file");
    expect(screen.queryByTestId("openspec-review-bar")).toBeNull();
    expect(screen.queryByTestId("source-review")).toBeNull();
    expect(screen.queryByTestId("openspec-artifact-hunks")).toBeNull();
    expect(screen.queryByRole("button", { name: "Open in editor" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Discuss selection" })).toBeNull();
    // The same gesture in the Source of a version offers no discussion.
    const file = screen.getByTestId("openspec-version-file");
    await user.click(within(file).getByRole("button", { name: "Source" }));
    selectAndRelease(within(file).getByTestId("openspec-source-text"), "cannot leave");
    // The selection stays, so the check below is not vacuous.
    expect(window.getSelection()!.toString()).toBe("cannot leave");
    expect(screen.queryByRole("button", { name: "Discuss selection" })).toBeNull();
  });

  it("OCW-FR-DRQD: Back to current gives the focus to the reading pane of the current change, not to the document body", async () => {
    const user = await openHistory();
    await user.click(versionRows()[1]);
    await screen.findByTestId("openspec-version-file");
    const back = screen.getByRole("button", { name: "← Back to current" });
    await user.click(back);
    expect(screen.queryByTestId("openspec-version-view")).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("openspec-reading")));
  });

  it("OCW-FR-DRQD, OCW-FR-EJGH: a review bar that a shown version hides comes back after Back to current, and the focus goes to the reading pane", async () => {
    backend.proposals = [changeProposal()];
    const user = await openHistory();
    await screen.findByTestId("openspec-review-bar");
    await user.click(versionRows()[1]);
    await screen.findByTestId("openspec-version-file");
    expect(screen.queryByTestId("openspec-review-bar")).toBeNull();
    await user.click(screen.getByRole("button", { name: "← Back to current" }));
    expect(await screen.findByTestId("openspec-review-bar")).toBeInTheDocument();
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("openspec-reading")));
  });

  it("OCW-FR-EJGH: a review bar that goes away while the focus is on another control leaves the focus on that control", async () => {
    backend.proposals = [changeProposal()];
    drawChange();
    await screen.findByTestId("openspec-review-bar");
    const information = screen.getByRole("button", { name: "Information" });
    information.focus();
    backend.proposals = [changeProposal(["rejected", "rejected"], { state: "rejected" })];
    act(() => {
      emit("change-edit-proposals-changed", {
        streamId: STREAM_ID,
        change: CHANGE,
        proposal: backend.proposals[0],
        cause: "decided",
      });
    });
    await waitFor(() => expect(screen.queryByTestId("openspec-review-bar")).toBeNull());
    expect(document.activeElement).toBe(information);
  });

  it("OCW-FR-DRQD: the Current change row keeps the focus on itself", async () => {
    const user = await openHistory();
    await user.click(versionRows()[1]);
    await screen.findByTestId("openspec-version-file");
    const current = screen.getByRole("button", { name: "Current change" });
    await user.click(current);
    expect(screen.queryByTestId("openspec-version-view")).toBeNull();
    await screen.findByRole("navigation", { name: "Artifacts" });
    expect(document.activeElement).toBe(current);
  });

  it("OCW-FR-DRQD: Back to current and the Current change row show the current change again", async () => {
    const user = await openHistory();
    await user.click(versionRows()[1]);
    await screen.findByTestId("openspec-version-banner");
    await user.click(screen.getByRole("button", { name: "← Back to current" }));
    expect(screen.queryByTestId("openspec-version-view")).toBeNull();
    expect(screen.getByRole("navigation", { name: "Artifacts" })).toBeInTheDocument();

    await user.click(versionRows()[1]);
    await screen.findByTestId("openspec-version-banner");
    await user.click(screen.getByRole("button", { name: "Current change" }));
    expect(screen.queryByTestId("openspec-version-view")).toBeNull();
  });

  it("OCW-FR-DRQD: closing History shows the current change again", async () => {
    const user = await openHistory();
    await user.click(versionRows()[1]);
    await screen.findByTestId("openspec-version-banner");
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(screen.queryByTestId("openspec-history")).toBeNull();
    expect(screen.queryByTestId("openspec-version-view")).toBeNull();
  });

  it("OCW-FR-DRQD: a version that cannot be read states it in place of the version", async () => {
    backend.versionFiles = { reject: "unknown_version" };
    const user = await openHistory();
    await user.click(versionRows()[0]);
    expect(await screen.findByText("This version is no longer on the stream branch.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Compare with current" })).toBeDisabled();
  });
});

describe("the comparison (OCW-FR-ABAR)", () => {
  it("OCW-FR-ABAR: Compare with current shows each file beside the current one as a line diff, with added, removed, and changed files marked", async () => {
    const user = await openHistory();
    await user.click(versionRows()[2]);
    await screen.findByTestId("openspec-version-file");
    await user.click(screen.getByRole("button", { name: "Compare with current" }));
    expect(screen.getByRole("button", { name: "Compare with current" })).toHaveAttribute("aria-pressed", "true");
    const files = screen.getAllByTestId("openspec-compare-file");
    const mark = (path: string) => files.find((f) => f.getAttribute("data-path") === path)?.getAttribute("data-mark");
    // The version holds diagram.png, proposal.md, and tasks.md. The current
    // change holds proposal.md, the two spec files, and design.md.
    expect(files.map((f) => f.getAttribute("data-path"))).toEqual([
      "design.md",
      "diagram.png",
      "proposal.md",
      "specs/auth/spec.md",
      "specs/ui/spec.md",
      "tasks.md",
    ]);
    expect(mark("proposal.md")).toBe("changed");
    expect(mark("tasks.md")).toBe("removed");
    expect(mark("design.md")).toBe("added");
    expect(mark("specs/auth/spec.md")).toBe("added");

    const proposal = files.find((f) => f.getAttribute("data-path") === "proposal.md")!;
    expect(within(proposal).getByText("changed since this version")).toBeInTheDocument();
    const table = within(proposal).getByRole("table", { name: "Changes in proposal.md" });
    const changed = table.querySelectorAll("tr[data-changed]");
    expect(changed.length).toBeGreaterThan(0);
    expect(within(table).getByText("Users cannot leave.")).toHaveAttribute("data-side", "old");
  });

  it("OCW-FR-ABAR: a file that is not text in either is named with no diff", async () => {
    const user = await openHistory();
    await user.click(versionRows()[2]);
    await screen.findByTestId("openspec-version-file");
    await user.click(screen.getByRole("button", { name: "Compare with current" }));
    const byPath = (path: string) =>
      screen.getAllByTestId("openspec-compare-file").find((f) => f.getAttribute("data-path") === path)!;
    // Not text in the version alone, and in the current change alone.
    for (const path of ["diagram.png", "design.md"]) {
      const file = byPath(path);
      expect(file).toHaveTextContent(path);
      expect(within(file).queryByRole("table")).toBeNull();
      expect(file).toHaveTextContent("This file is not text in one of the two, so no diff is shown.");
    }
    expect(byPath("diagram.png")).toHaveAttribute("data-mark", "removed");
    // A text file that only the version holds still shows its lines.
    expect(within(byPath("tasks.md")).getByRole("table")).toBeInTheDocument();
  });

  it("OCW-FR-ABAR: Compare with current again shows the version again", async () => {
    const user = await openHistory();
    await user.click(versionRows()[2]);
    await screen.findByTestId("openspec-version-file");
    await user.click(screen.getByRole("button", { name: "Compare with current" }));
    expect(screen.queryByTestId("openspec-version-file")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Compare with current" }));
    expect(await screen.findByTestId("openspec-version-file")).toBeInTheDocument();
    expect(screen.queryByTestId("openspec-compare-file")).toBeNull();
  });
});
