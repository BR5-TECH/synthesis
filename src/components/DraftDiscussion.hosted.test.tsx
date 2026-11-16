/**
 * The discussion column under a host that keeps its own state
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-YLRM, CMT-FR-17).
 * The Map tab keeps the opening composer, names its tabs, and adds a control to
 * each composer (`SMP-specification-map.md` SMP-FR-IHFM, SMP-FR-EIUP,
 * SMP-FR-DUUU), so the column must take what the host holds as it mounts and
 * leave the composer only when the host focuses another discussion.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { resetLogBufferForTest } from "../logging";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { CONTEXT_ESTIMATE, otherCommand } from "../test/invokeMocks";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import {
  columnElement,
  discussionSaying as thread,
  type ColumnProps,
} from "../test/draftDiscussionFixtures";

const FIRST = thread("disc-1", ["opening"]);
const SECOND = thread("disc-2", ["second"]);

beforeEach(() => {
  invokeMock.mockReset();
  // The column reads the context of a turn and logs; anything else is refused.
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "estimate_discussion_context") return Promise.resolve(CONTEXT_ESTIMATE);
    if (cmd === "list_agent_turns") return Promise.resolve([]);
    return otherCommand(cmd);
  });
  resetLogBufferForTest();
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
});

afterEach(cleanup);

function hosted(over: Partial<ColumnProps>) {
  return columnElement({ discussions: [FIRST, SECOND], ...over });
}

describe("a column whose host keeps the opening composer (SMP-FR-IHFM, DDS-FR-YLRM)", () => {
  it("SMP-FR-IHFM: mounting with a focused discussion keeps the composer the host holds", async () => {
    const onComposingChange = vi.fn();
    render(hosted({ selectedThreadId: "disc-1", composing: true, onComposingChange }));
    expect(await screen.findByTestId("draft-open-discussion")).toBeInTheDocument();
    expect(screen.queryByTestId("comment-thread-disc-1")).toBeNull();
    expect(onComposingChange).not.toHaveBeenCalledWith(false);
  });

  it("DDS-FR-YLRM: focusing another discussion leaves the composer and shows that discussion", async () => {
    const onComposingChange = vi.fn();
    const view = render(hosted({ selectedThreadId: "disc-1", composing: true, onComposingChange }));
    await screen.findByTestId("draft-open-discussion");
    act(() => {
      view.rerender(hosted({ selectedThreadId: "disc-2", composing: true, onComposingChange }));
    });
    expect(onComposingChange).toHaveBeenCalledWith(false);
  });

  it("DDS-FR-YLRM: an unhosted column leaves the composer of another discussion when the focus moves", async () => {
    const view = render(hosted({ selectedThreadId: "disc-1" }));
    await screen.findByTestId("comment-thread-disc-1");
    act(() => screen.getByTestId("discussion-new").click());
    expect(await screen.findByTestId("draft-open-discussion")).toBeInTheDocument();
    act(() => {
      view.rerender(hosted({ selectedThreadId: "disc-2" }));
    });
    expect(await screen.findByTestId("comment-thread-disc-2")).toBeInTheDocument();
  });

  it("CMT-FR-17: mounting on a resolved discussion expands the resolved disclosure", async () => {
    const onShowResolved = vi.fn();
    render(
      hosted({
        discussions: [FIRST, { ...SECOND, resolved: true }],
        selectedThreadId: "disc-2",
        onShowResolved,
      }),
    );
    expect(await screen.findByTestId("comment-thread-disc-2")).toBeInTheDocument();
    expect(onShowResolved).toHaveBeenCalledWith(true);
  });
});

describe("what a host adds (SMP-FR-EIUP, SMP-FR-DUUU)", () => {
  it("SMP-FR-EIUP: a tab reads the name the host gives it, with the name as its tooltip", async () => {
    render(hosted({ tabLabel: (d) => (d.id === "disc-1" ? "Map" : "Decline message") }));
    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((t) => t.textContent?.trim())).toEqual(["Map", "Decline message"]);
    expect(tabs[1]).toHaveAttribute("title", "Decline message");
  });

  it("SMP-FR-DUUU: the header and each composer carry what the host adds", async () => {
    const view = render(
      hosted({
        headerAccessory: <span data-testid="host-header" />,
        composerAccessory: <span data-testid="host-chip" />,
      }),
    );
    expect(await screen.findByTestId("host-header")).toBeInTheDocument();
    expect(await screen.findByTestId("host-chip")).toBeInTheDocument();
    act(() => {
      view.rerender(
        hosted({
          discussions: [],
          composerAccessory: <span data-testid="host-chip" />,
          opening: {
            statement: () => ({ lead: "Host lead.", hint: "Host hint." }),
            ariaLabel: "Discuss the map",
          },
        }),
      );
    });
    expect(await screen.findByText("Host lead.")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Discuss the map" })).toBeInTheDocument();
    expect(screen.getByTestId("host-chip")).toBeInTheDocument();
  });
});
