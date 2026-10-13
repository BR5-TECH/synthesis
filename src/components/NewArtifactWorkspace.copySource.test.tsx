/**
 * **Copy as Source** in the New Artifact tab
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-28, NAW-FR-44,
 * NAW-FR-AXFQ, NAW-FR-NVYZ, NAW-FR-KEAL, NAW-FR-FJXX, NAW-FR-QPTN, and
 * `NTF-notifications.md` NTF-FR-RGVU).
 *
 * The Copied interval and the collapse are the control's own, and
 * `ActionControl.copySource.test.tsx` covers them with fake timers.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  draft,
  editSource,
  makeStubs,
  openActions,
  renderWorkspace,
  showRail,
  threeVersions,
  versionRows,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetToasts, visibleToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import type { DraftStatus } from "../types";
import {
  COPY_FAILED_BODY,
  COPY_FAILED_KEY,
  COPY_FAILED_TITLE,
} from "./NewArtifactWorkspace/copySource";

const { stub } = makeStubs(invokeMock);

/** The commands the tab sent, without the log flush that runs in the background. */
const sent = () =>
  invokeMock.mock.calls
    .map(([cmd]) => cmd as string)
    .filter((cmd) => cmd !== "append_log_records");

/**
 * NAW-FR-FJXX: what the tab sent after `mark` that it did not already send on
 * mount. A late mount read repeats a name in `mark`, so it is not counted, and
 * the order of the mount reads has no effect.
 */
const sentSince = (mark: string[]) =>
  sent()
    .slice(mark.length)
    .filter((cmd) => !mark.includes(cmd));

/** NAW-FR-FJXX: the commands that change a draft. None of them may follow a copy. */
const WRITES = [
  "save_draft_file_contents",
  "set_draft_status",
  "rename_draft",
  "store_draft_image",
  "open_discussion_thread",
  "dispatch_agent_turn",
  "publish_draft_to_github",
  "graduate_draft",
];

/**
 * Put a clipboard in front of the tab. Installed after `openActions`, because
 * `userEvent` puts its own clipboard stub on `navigator` when it runs.
 */
function installClipboard(
  writeText: (text: string) => Promise<void> = async () => {},
) {
  const spy = vi.fn(writeText);
  Object.defineProperty(window.navigator, "clipboard", {
    configurable: true,
    value: { writeText: spy },
  });
  return spy;
}

/** The base stub, with the prompt's saved bytes replaced by `body`. */
function stubPrompt(body: string, opts: Parameters<typeof stub>[0] = {}) {
  stub(opts);
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_draft_file_contents") return { body, checksum: "c1" };
    return base(cmd, args);
  });
}

async function copy(writeText?: (text: string) => Promise<void>) {
  await openActions();
  const spy = installClipboard(writeText);
  fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
  return spy;
}

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetToasts();
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  Reflect.deleteProperty(window.navigator, "clipboard");
});

describe("New Artifact workspace — Copy as Source", () => {
  it("NAW-FR-28: the draft's entries read Discuss, Graduate, Copy as Source, Publish to GitHub, Archive", async () => {
    stub();
    renderWorkspace();
    await openActions();
    expect(
      screen.getAllByRole("menuitem").map((e) => e.textContent?.trim()),
    ).toEqual([
      "Discuss",
      "Graduate",
      "Copy as Source",
      "Publish to GitHub",
      "Archive",
    ]);
  });

  it("NAW-FR-NVYZ: the saved prompt goes to the clipboard byte for byte, line endings included", async () => {
    const body = "# Title\r\n\r\nFirst line.\r\n  - item with `code`\r\n";
    stubPrompt(body);
    renderWorkspace();
    await screen.findByText("Title");
    const writeText = await copy();
    expect(
      await screen.findByRole("menuitem", { name: "Copied" }),
    ).toBeInTheDocument();
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith(body);
  });

  it("NAW-FR-NVYZ: unsaved edits are part of the copy", async () => {
    stub();
    // The write never lands, so the edit stays unsaved however slow the runner.
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "save_draft_file_contents") return new Promise(() => {});
      return base(cmd, args);
    });
    renderWorkspace();
    await screen.findByText("body text");
    await editSource("an edit nobody has saved yet");
    const writeText = await copy();
    await screen.findByRole("menuitem", { name: "Copied" });
    expect(writeText).toHaveBeenCalledWith("an edit nobody has saved yet");
  });

  it("NAW-FR-NVYZ: while a version is read, the copy is the live prompt and not that version", async () => {
    stub({ history: threeVersions() });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "save_draft_file_contents") return new Promise(() => {});
      return base(cmd, args);
    });
    renderWorkspace();
    await screen.findByText("body text");
    // An unsaved edit, so a copy taken from disk would not match.
    await editSource("the live prompt, edited");
    await showRail();
    const older = versionRows().find((row) =>
      row.textContent?.includes("Original"),
    );
    await userEvent.click(older ?? versionRows()[versionRows().length - 1]);
    await screen.findByText(/the prompt as of/);
    const writeText = await copy();
    await screen.findByRole("menuitem", { name: "Copied" });
    expect(writeText).toHaveBeenCalledWith("the live prompt, edited");
  });

  it("NAW-FR-NVYZ, NAW-FR-FJXX: an empty prompt copies empty text", async () => {
    stubPrompt("");
    renderWorkspace();
    await openActions();
    const entry = screen.getByRole("menuitem", { name: "Copy as Source" });
    expect(entry).toBeEnabled();
    const writeText = installClipboard();
    fireEvent.click(entry);
    await screen.findByRole("menuitem", { name: "Copied" });
    expect(writeText).toHaveBeenCalledWith("");
  });

  it("NAW-FR-KEAL: the copy holds the prompt alone, also when the draft carries discussions", async () => {
    stub();
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_discussions")
        return [
          {
            id: "t1",
            target: { kind: "draft", draftId: "d1" },
            fragmentTarget: null,
            comments: [
              {
                id: "c1",
                author: { kind: "human", login: "raver119" },
                body: "a comment that is not the prompt",
                quotes: [],
                attachments: [
                  {
                    kind: "url",
                    url: "https://example.com/spec.pdf",
                    mediaType: "application/pdf",
                    label: "spec.pdf",
                  },
                ],
                createdAt: "2026-01-01T00:00:00Z",
              },
            ],
            locked: false,
            resolved: false,
            createdAt: "2026-01-01T00:00:00Z",
            updatedAt: "2026-01-01T00:00:00Z",
          },
        ];
      return base(cmd, args);
    });
    renderWorkspace();
    await screen.findByText("body text");
    // The discussion is on screen before the copy.
    await screen.findByText("a comment that is not the prompt");
    const writeText = await copy();
    await screen.findByRole("menuitem", { name: "Copied" });
    expect(writeText).toHaveBeenCalledWith("body text");
  });

  /** NAW-FR-44: what `get_draft_graduation` answers for a run that holds the draft. */
  const held = { id: "run-9", state: "working", commits: [] };
  const cases: [string, DraftStatus, typeof held | null][] = [
    ["active", "active", null],
    ["archived", "archived", null],
    ["published", "published", null],
    ["graduated", "graduated", null],
    ["github_shadow", "github_shadow", null],
    ["held by a graduation run", "active", held],
  ];
  it.each(cases)(
    "NAW-FR-FJXX, NAW-FR-44, NAW-FR-AXFQ: Copy as Source is enabled on a draft that is %s",
    async (_label, status, run) => {
      stub({ status });
      const base = invokeMock.getMockImplementation()!;
      invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === "get_draft_graduation") return run;
        if (cmd === "open_draft" && status === "github_shadow")
            return {
              ...draft(status),
              githubIssue: {
                repositoryOwner: "acme",
                repositoryName: "platform",
                issueNumber: 9,
                issueUrl: "https://github.com/acme/platform/issues/9",
                projectNodeId: "P1",
                claimState: "claimed",
              },
            };
        return base(cmd, args);
      });
      renderWorkspace();
      await screen.findByText("body text");
      if (status === "github_shadow") {
        await screen.findByTestId("draft-github-shadow-banner");
      }
      await openActions();
      if (status === "archived") {
        // NAW-FR-28: the status has landed, so the last entry reads Restore.
        await screen.findByRole("menuitem", { name: "Restore" });
      }
      if (status === "graduated" || run !== null) {
        // NAW-FR-44: the lock has landed, so Graduate is disabled by now.
        await waitFor(() =>
          expect(
            screen.getByRole("menuitem", { name: "Graduate" }),
          ).toBeDisabled(),
        );
      }
      const entry = screen.getByRole("menuitem", { name: "Copy as Source" });
      expect(entry).toBeEnabled();
      const writeText = installClipboard();
      fireEvent.click(entry);
      await screen.findByRole("menuitem", { name: "Copied" });
      expect(writeText).toHaveBeenCalledWith("body text");
    },
  );

  it("NAW-FR-FJXX: the copy invokes no backend operation, saves nothing, and changes no status", async () => {
    stub();
    renderWorkspace();
    await screen.findByText("body text");
    await openActions();
    // Every read the tab makes on mount has been sent by now.
    await waitFor(() => expect(sent()).toContain("get_draft_publication"));
    const before = sent();
    const writeText = installClipboard();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await screen.findByRole("menuitem", { name: "Copied" });
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(sentSince(before)).toEqual([]);
    for (const write of WRITES) expect(sent()).not.toContain(write);
    expect(visibleToasts()).toEqual([]);
  });

  it("NAW-FR-QPTN: further activations while the clipboard write runs do nothing", async () => {
    stub();
    renderWorkspace();
    await screen.findByText("body text");
    let finish: () => void = () => {};
    const writeText = await copy(
      () => new Promise<void>((resolve) => (finish = resolve)),
    );
    const entry = screen.getByRole("menuitem", { name: "Copy as Source" });
    fireEvent.click(entry);
    fireEvent.click(entry);
    expect(writeText).toHaveBeenCalledTimes(1);
    finish();
    await screen.findByRole("menuitem", { name: "Copied" });
    expect(writeText).toHaveBeenCalledTimes(1);
  });

  it("NAW-FR-QPTN, NTF-FR-RGVU: a failed write collapses the control and shows the Error toast, and the draft does not change", async () => {
    stub();
    renderWorkspace();
    await screen.findByText("body text");
    await openActions();
    await waitFor(() => expect(sent()).toContain("get_draft_publication"));
    const before = sent();
    const writeText = installClipboard(() =>
      Promise.reject(new DOMException("denied", "NotAllowedError")),
    );
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menuitem", { name: "Copied" })).toBeNull();
    // NTF-FR-RGVU: a fixed key, the level Error, a title and a body, no address.
    expect(visibleToasts()).toEqual([
      {
        key: COPY_FAILED_KEY,
        level: "Error",
        title: COPY_FAILED_TITLE,
        body: COPY_FAILED_BODY,
        address: null,
        revision: 0,
      },
    ]);
    expect(COPY_FAILED_KEY).toBe("draft-copy-failed");
    expect(COPY_FAILED_TITLE).toBe("Draft not copied");
    expect(COPY_FAILED_BODY).toBe(
      "The prompt of the draft was not copied to the clipboard.",
    );
    // No other message shows, and nothing reached the backend.
    expect(screen.queryAllByRole("alert")).toEqual([]);
    expect(sentSince(before)).toEqual([]);
    for (const write of WRITES) expect(sent()).not.toContain(write);
    expect(screen.getByText("body text")).toBeInTheDocument();
  });

  it("NTF-FR-RGVU: a platform with no clipboard is a failure too", async () => {
    stub();
    renderWorkspace();
    await screen.findByText("body text");
    await openActions();
    Object.defineProperty(window.navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(visibleToasts().map((t) => t.key)).toEqual([COPY_FAILED_KEY]);
  });

  it("NTF-FR-RGVU: a second failed copy replaces the toast that shows", async () => {
    stub();
    renderWorkspace();
    await screen.findByText("body text");
    for (let attempt = 0; attempt < 2; attempt += 1) {
      await openActions();
      installClipboard(() => Promise.reject(new Error("refused")));
      fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
      await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    }
    expect(visibleToasts()).toHaveLength(1);
    expect(visibleToasts()[0]).toMatchObject({
      key: COPY_FAILED_KEY,
      revision: 1,
    });
  });

  it("NAW-FR-QPTN, NTF-FR-RGVU: a prompt whose read failed is not copied as empty text", async () => {
    stub();
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_draft_file_contents")
        return Promise.reject(new Error("read failed"));
      return base(cmd, args);
    });
    renderWorkspace();
    await waitFor(() => expect(sent()).toContain("load_draft_file_contents"));
    await openActions();
    const writeText = installClipboard();
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy as Source" }));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(writeText).not.toHaveBeenCalled();
    expect(visibleToasts().map((t) => t.key)).toEqual([COPY_FAILED_KEY]);
  });
});
