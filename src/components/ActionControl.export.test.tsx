/**
 * **Export to...** in the action control (`ACT-action-control.md` ACT-FR-04,
 * ACT-FR-05; `EXD-export-dialog.md` EXD-FR-UQNO).
 *
 * The control is mounted alone, so what is checked is the entry's place in the
 * fixed order and the states that withhold or disable it. Which tab offers it
 * is the Editor's, and `Editor.exportAction.test.tsx` covers that.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) =>
    cmd === "append_log_records"
      ? Promise.resolve()
      : Promise.reject(new Error(`unexpected command ${cmd}`)),
}));

import { ActionControl } from "./ActionControl";
import type { HostAction, HostCopyAction } from "./ActionControl";
import type { UseDiscussionsResult } from "../hooks/useDiscussions";
import { resetLogBufferForTest } from "../logging";
import { resetDiscussionFocus } from "../state/discussionFocus";

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
    setResolved: async () => {},
  };
}

const host = (label: string): HostAction => ({ label, onActivate: vi.fn() });
const copy: HostCopyAction = { label: "Copy as Source", onActivate: async () => true };

beforeEach(() => {
  resetLogBufferForTest();
  resetDiscussionFocus();
});
afterEach(cleanup);

function expand() {
  fireEvent.click(screen.getByRole("button", { name: "Actions" }));
  return screen.getByRole("menu");
}

describe("Export to... in the action control", () => {
  it("ACT-FR-05, EXD-FR-UQNO: Export to... comes directly after Discuss and before the draft entries", () => {
    render(
      <ActionControl
        discussions={discussions()}
        discussionsInRail
        exportTo={host("Export to...")}
        graduate={host("Graduate")}
        copySource={copy}
        publish={host("Publish to GitHub")}
        archive={host("Archive")}
      />,
    );
    const names = within(expand())
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.trim());
    expect(names).toEqual([
      "Discuss",
      "Export to...",
      "Graduate",
      "Copy as Source",
      "Publish to GitHub",
      "Archive",
    ]);
  });

  it("ACT-FR-04: a host that contributes no Export to... gets no such entry", () => {
    render(<ActionControl discussions={discussions()} discussionsInRail />);
    const names = within(expand())
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.trim());
    expect(names).toEqual(["Discuss"]);
  });

  it("ACT-FR-04: a read-only tab never affords Export to...", () => {
    render(
      <ActionControl
        discussions={discussions()}
        discussionsInRail
        readOnly
        exportTo={host("Export to...")}
      />,
    );
    expect(within(expand()).queryByRole("menuitem", { name: /Export to/ })).toBeNull();
  });

  it("ACT-FR-04, EXD-FR-UQNO: activating it collapses the control and hands the action to the host", () => {
    const action = host("Export to...");
    render(<ActionControl discussions={discussions()} discussionsInRail exportTo={action} />);
    fireEvent.click(within(expand()).getByRole("menuitem", { name: /Export to/ }));
    expect(action.onActivate).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("ACT-FR-07: the entry is disabled when the file is gone", () => {
    render(
      <ActionControl
        discussions={discussions()}
        discussionsInRail
        missing
        exportTo={host("Export to...")}
      />,
    );
    const entry = within(expand()).getByRole("menuitem", { name: /Export to/ });
    expect(entry).toBeDisabled();
    expect(entry).toHaveAttribute("title", "This file no longer exists in the project");
  });
});
