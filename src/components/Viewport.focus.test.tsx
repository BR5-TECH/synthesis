// The Viewport's routing of a Comments-panel click-through to the right Editor
// (`CMP-comments-panel.md` CMP-FR-11, `CMT-comments.md` CMT-FR-36).
//
// The focus is per artifact, not per shell: it must reach the tab whose artifact
// the panel named and no other. Without that gate, focusing any other tab would
// yank it to a thread id it does not hold.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { Viewport } from "./Viewport";
import { EditSessionStore } from "../state/editSessions";
import { DraftSessionStore } from "../state/draftSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { SearchSessionStore } from "../state/searchSessions";
import { SpecMapSessionStore } from "../state/specMap/session";
import type { Tab } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

/** Records what the Editor was handed, without rendering the real one. */
const editorProps = vi.fn();
vi.mock("./Editor", () => ({
  Editor: (props: Record<string, unknown>) => {
    editorProps(props);
    return <div data-testid="editor-stub" />;
  },
}));

function tab(artifactId: string): Tab {
  return {
    id: `art:${artifactId}`,
    label: artifactId,
    kind: "editor",
    artifactId,
  };
}

function renderViewport(
  activeArtifact: string,
  focusThread: { artifactId: string; threadId: string } | null,
) {
  const onThreadFocused = vi.fn();
  render(
    <Viewport
      activeTab={`art:${activeArtifact}`}
      activeT={tab(activeArtifact)}
      onOpenArtifact={vi.fn()}
      onOpenRuns={vi.fn()}
      draftsRevision={0}
      onOpenGit={vi.fn()}
      sessions={new EditSessionStore()}
      flows={new FlowSessionStore()}
      drafts={new DraftSessionStore()}
      onDraftChanged={vi.fn()}
      onGraduationStarted={vi.fn()}
      onOpenRun={vi.fn()}
      onOpenDraft={vi.fn()}
      onDraftRenamed={vi.fn()}
      onCloseTab={vi.fn()}
      onActivateSearchHit={vi.fn()}
      searches={new SearchSessionStore()}
      specMap={new SpecMapSessionStore()}
      onNewDraftForMapNode={vi.fn()}
      focusThread={focusThread}
      onThreadFocused={onThreadFocused}
    />,
  );
  return { onThreadFocused };
}

/** The props of the most recent Editor render. */
function lastEditorProps(): Record<string, unknown> {
  const calls = editorProps.mock.calls;
  return calls[calls.length - 1][0] as Record<string, unknown>;
}

const lastFocusId = () => lastEditorProps().focusThreadId as string | null;

beforeEach(() => {
  invokeMock.mockReset();
  editorProps.mockClear();
});
afterEach(cleanup);

describe("CMP-FR-11 / CMT-FR-36: the focus reaches only the artifact it names", () => {
  it("hands the thread id to the Editor for the artifact the panel named", () => {
    renderViewport("specs/onboarding.md", {
      artifactId: "specs/onboarding.md",
      threadId: "t1",
    });
    expect(screen.getByTestId("editor-stub")).toBeInTheDocument();
    expect(lastFocusId()).toBe("t1");
  });

  it("withholds it from a tab for a different artifact", () => {
    // The panel named `onboarding.md`; the active tab is `other.md`. Handing the
    // id over here would focus a thread that tab does not hold.
    renderViewport("specs/other.md", {
      artifactId: "specs/onboarding.md",
      threadId: "t1",
    });
    expect(lastFocusId()).toBeNull();
  });

  it("hands over nothing when no click-through is pending", () => {
    renderViewport("specs/onboarding.md", null);
    expect(lastFocusId()).toBeNull();
  });

  it("passes the clear-callback through so the focus is consumed once", () => {
    const { onThreadFocused } = renderViewport("specs/onboarding.md", {
      artifactId: "specs/onboarding.md",
      threadId: "t1",
    });
    const handed = lastEditorProps().onThreadFocused as (() => void) | undefined;
    expect(handed).toBeDefined();
    handed?.();
    expect(onThreadFocused).toHaveBeenCalledTimes(1);
  });
});
