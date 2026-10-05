/**
 * Publishing a draft to GitHub from the New Artifact tab
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-GKBP … NAW-FR-UDPM,
 * served by `../../specifications/core/GHP-github-publication.md`).
 *
 * Driven through the real tab rather than through the hook, because what the
 * requirements state is what the author reaches: an entry in the one action
 * control, a picker that explains every remote it refuses, a recovery choice,
 * a read-only tag in the tab's chrome row, and the band under it.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import {
  confirmPublicationChooser,
  editSource,
  makeStubs,
  noPublication,
  openActions,
  publicationRecord,
  publicationRefused,
  renderWorkspace,
  fourRemotes,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type { DraftPublicationView, PublicationOutcome } from "../types";

const { stub } = makeStubs(invokeMock);

const calls = (name: string) =>
  invokeMock.mock.calls.filter(([cmd]) => cmd === name);

/** GHP-FR-RUYT: one attempt on disk, before GitHub has answered. */
function openAttempt(
  state: "open" | "awaiting_choice" = "open",
  marker = "pub-1",
) {
  return {
    marker,
    remoteName: "origin",
    remoteUrl: "github.com/acme/widgets",
    repositoryOwner: "acme",
    repositoryName: "widgets",
    state,
    startedAt: "2026-09-12T10:00:00.000Z",
    updatedAt: "2026-09-12T10:00:00.000Z",
  } as const;
}

/** The **Publish to GitHub** entry of the tab's one action control. */
async function publishEntry() {
  const menu = await openActions();
  return within(menu).getByRole("menuitem", { name: /Publish to GitHub/ });
}

/** Emit a `"draft publication changed"` for this draft. */
async function publicationChanged() {
  await act(async () => {
    for (const handler of listeners.get("draft-publication-changed") ?? []) {
      handler({ payload: { draftId: "d1" } });
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

describe("New Artifact workspace — publishing to GitHub", () => {
  it("NAW-FR-TSQE, NAW-FR-ZQMX: the tab reads publication once on mount and offers the action", async () => {
    renderWorkspace();
    const entry = await publishEntry();

    expect(entry).toBeEnabled();
    expect(calls("get_draft_publication")).toHaveLength(1);
    expect(calls("get_draft_publication")[0][1]).toEqual({ draftId: "d1" });
    // NAW-FR-DWKA: nothing is enumerated, and nothing is created, until the
    // action is activated.
    expect(calls("list_publication_remotes")).toHaveLength(0);
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });

  it("NAW-FR-TSQE: a publication change re-reads rather than the tab polling", async () => {
    renderWorkspace();
    await publishEntry();
    expect(calls("get_draft_publication")).toHaveLength(1);

    await publicationChanged();
    await waitFor(() =>
      expect(calls("get_draft_publication").length).toBeGreaterThan(1),
    );
  });

  it("NAW-FR-28, ACT-FR-05: the control offers Discuss, Graduate, Publish to GitHub, and Archive in that order", async () => {
    renderWorkspace();
    const menu = await openActions();
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent?.trim()),
    ).toEqual(["Discuss", "Graduate", "Publish to GitHub", "Archive"]);
  });

  it("NAW-FR-ZQMX, NAW-FR-VBHT: an archived draft states that it must be restored first", async () => {
    stub({
      status: "archived",
      publication: publicationRefused(
        "draft_archived",
        "Restore this draft before you publish it.",
      ),
    });
    renderWorkspace();
    const entry = await publishEntry();

    expect(entry).toBeDisabled();
    expect(entry).toHaveAttribute(
      "title",
      "Restore this draft before you publish it.",
    );
  });

  it("NAW-FR-VBHT: each refusal names the one that holds", async () => {
    const cases: [string, string][] = [
      ["issues_inaccessible", "The token cannot read this repository."],
      [
        "issues_disabled",
        "This repository does not accept new issues. Check that Issues are turned " +
          "on and that the repository is neither archived nor disabled.",
      ],
      ["issues_create_forbidden", "The token cannot create issues in this repository."],
      ["no_remote_configured", "This project has no configured Git remote."],
      ["no_github_remote", "This project has no GitHub remote."],
      ["attempt_in_progress", "A publication attempt for this draft is already in progress."],
    ];
    for (const [code, reason] of cases) {
      cleanup();
      invokeMock.mockReset();
      listeners.clear();
      stub({
        publication: publicationRefused(
          code as DraftPublicationView["eligibility"]["reasonCode"] & string,
          reason,
        ),
      });
      renderWorkspace();
      const entry = await publishEntry();
      expect(entry, code).toBeDisabled();
      expect(entry, code).toHaveAttribute("title", reason);
    }
  });

  it("NAW-FR-PNCL: a draft holding local asset references is refused and every path is listed", async () => {
    stub({
      publication: publicationRefused(
        "local_assets_unsupported",
        "Local media attachments are not supported.",
        ["../assets/a1.png", "./local/b.png"],
      ),
    });
    renderWorkspace();
    const entry = await publishEntry();

    expect(entry).toBeDisabled();
    const title = entry.getAttribute("title") ?? "";
    expect(title).toContain("Local media attachments are not supported.");
    expect(title).toContain("../assets/a1.png");
    expect(title).toContain("./local/b.png");
  });

  it("NAW-FR-DWKA, NAW-FR-HNVR, NAW-FR-GKBP: one configured remote opens the chooser with no picker, and Publish sends a root choice", async () => {
    let published: DraftPublicationView = noPublication();
    stub({ publication: () => published });
    invokeMock.mockImplementation(
      (
        (base) =>
        async (cmd: string, args?: unknown) => {
          if (cmd === "publish_draft_to_github") {
            const record = publicationRecord(418, "pub-1");
            published = { ...noPublication(), current: record, history: [record] };
            return { kind: "published", record } satisfies PublicationOutcome;
          }
          return base(cmd, args);
        }
      )(invokeMock.getMockImplementation() as (c: string, a?: unknown) => Promise<unknown>),
    );

    renderWorkspace();
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();

    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toEqual({
      draftId: "d1",
      remoteName: "origin",
      persistRemote: false,
      publicationChoice: { parentIssueNumber: null, issueType: null, milestoneNumber: null },
    });
    expect(calls("load_publication_metadata")).toHaveLength(1);
    expect(calls("load_publication_metadata")[0][1]).toEqual({
      draftId: "d1",
      remoteName: "origin",
    });
    expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull();
  });

  it("NAW-FR-RVGT, NAW-FR-MFXO: two or more remotes open a picker that explains every refusal", async () => {
    stub({ remotes: fourRemotes() });
    renderWorkspace();
    await userEvent.click(await publishEntry());

    const dialog = await screen.findByRole("dialog", { name: "Publish to GitHub" });
    // Every configured remote is listed rather than the ineligible ones dropped.
    const radios = within(dialog).getAllByRole("radio");
    expect(radios.map((r) => (r as HTMLInputElement).value)).toEqual([
      "origin",
      "upstream",
      "fork",
      "mirror",
    ]);
    expect(radios[0]).toBeEnabled();
    expect(radios[1]).toBeDisabled();
    expect(radios[2]).toBeDisabled();
    expect(radios[3]).toBeDisabled();
    // Each refusal is explained where it sits, and a repository that accepts no
    // issue says so about the repository rather than about the token.
    expect(
      within(dialog).getByText("The token cannot create issues in this repository."),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "This repository does not accept new issues. Check that Issues are " +
          "turned on and that the repository is neither archived nor disabled.",
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("Not a GitHub repository.")).toBeInTheDocument();
    // NAW-FR-MFXO: the picker states how the standing choice was reached.
    expect(
      within(dialog).getByText("This choice applies to this publication only."),
    ).toBeInTheDocument();
    // Nothing has been published by opening it.
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });

  it("NAW-FR-MFXO, NAW-FR-JBHV: the persist option decides what the publish call carries", async () => {
    stub({ remotes: fourRemotes() });
    renderWorkspace();
    await userEvent.click(await publishEntry());
    const dialog = await screen.findByRole("dialog", { name: "Publish to GitHub" });

    await userEvent.click(
      within(dialog).getByLabelText("Use this remote for future publications"),
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    // NAW-FR-JBHV: confirming the picker creates nothing; the chooser follows.
    expect(calls("publish_draft_to_github")).toHaveLength(0);
    await confirmPublicationChooser();

    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toEqual({
      draftId: "d1",
      remoteName: "origin",
      persistRemote: true,
      publicationChoice: { parentIssueNumber: null, issueType: null, milestoneNumber: null },
    });
  });

  it("NAW-FR-JBHV: dismissing the picker invokes nothing and persists no choice", async () => {
    stub({ remotes: fourRemotes() });
    renderWorkspace();
    await userEvent.click(await publishEntry());
    const dialog = await screen.findByRole("dialog", { name: "Publish to GitHub" });

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull(),
    );
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });

  it("NAW-FR-CBUJ, NAW-FR-XRLD, NAW-FR-QTVX: the tag names the current issue and its one control leaves the app", async () => {
    const current = publicationRecord(418, "pub-c", "2026-09-12T10:04:00.000Z");
    stub({
      publication: {
        current,
        history: [
          current,
          publicationRecord(402, "pub-b", "2026-09-04T16:20:00.000Z"),
          publicationRecord(377, "pub-a", "2026-08-28T09:11:00.000Z"),
        ],
        attempt: null,
        eligibility: noPublication().eligibility,
      },
    });
    renderWorkspace();

    const tag = await screen.findByTestId("draft-publication-tag");
    expect(tag).toHaveAttribute("data-state", "published");
    expect(tag).toHaveTextContent("Published #418");
    // NAW-FR-QTVX: the repository, the issue, and the instant, in both the
    // hover text and the accessible name.
    const named = `Open issue #418 in acme/widgets, published ${new Date(
      "2026-09-12T10:04:00.000Z",
    ).toLocaleString()}`;
    expect(tag).toHaveAttribute("title", named);
    expect(screen.getByRole("button", { name: named })).toBe(tag);

    // NAW-FR-CBUJ: the earlier records are read in Draft Information, so this
    // tab states the current publication and lists nothing.
    expect(screen.queryByText(/issue #402/)).toBeNull();
    expect(screen.queryByText(/marker pub-c/)).toBeNull();
    expect(screen.queryByRole("region", { name: "Published to GitHub" })).toBeNull();

    // NAW-FR-XRLD: the tag's one control opens the current issue outside the
    // application and opens no page, tab, or overlay of its own.
    await userEvent.click(tag);
    expect(calls("open_publication_issue")[0][1]).toEqual({
      draftId: "d1",
      url: "https://github.com/acme/widgets/issues/418",
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("NAW-FR-CBUJ, NAW-FR-LQAF: the tag is in the chrome row and the band is under it", async () => {
    stub({
      publication: {
        ...noPublication(),
        attempt: openAttempt(),
      },
    });
    renderWorkspace();

    // NAW-FR-CBUJ: the tag stands in the chrome row, with the archived marker
    // and the run-state tag — not at the foot of the document column, which is
    // the placement this surface is moved out of.
    const chrome = await screen.findByTestId("draft-workspace-chrome");
    const tag = await screen.findByTestId("draft-publication-tag");
    expect(chrome).toContainElement(tag);

    // NAW-FR-LQAF: the band is under that row rather than inside it, and
    // outside the two columns the tab's body holds.
    const band = screen.getByTestId("draft-publication-band");
    expect(chrome).not.toContainElement(band);
    expect(
      band.compareDocumentPosition(chrome) & Node.DOCUMENT_POSITION_PRECEDING,
    ).toBeTruthy();
    expect(
      band.closest(".draft-workspace__body, .draft-editor__surface"),
    ).toBeNull();
  });

  it("NAW-FR-CBUJ: a failure is named ahead of the record it leaves standing", async () => {
    const record = publicationRecord(418, "pub-c");
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    stub({
      publication: { ...noPublication(), current: record, history: [record] },
    });
    const withRecord = invokeMock.getMockImplementation() as typeof base;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") throw new Error("github_unreachable");
      return withRecord(cmd, args);
    });

    renderWorkspace();
    expect(await screen.findByTestId("draft-publication-tag")).toHaveTextContent(
      "Published #418",
    );

    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();
    // The record is still the draft's current publication, so a tag that named
    // it would be true and useless: the failure is what the author must act on.
    await waitFor(() =>
      expect(screen.getByTestId("draft-publication-tag")).toHaveAttribute(
        "data-state",
        "failed",
      ),
    );
  });

  it("NAW-FR-XPUJ: an attempt over a published draft is named ahead of its record", async () => {
    const record = publicationRecord(418, "pub-c");
    stub({
      publication: {
        ...noPublication(),
        current: record,
        history: [record],
        attempt: openAttempt(),
      },
    });
    renderWorkspace();

    const tag = await screen.findByTestId("draft-publication-tag");
    expect(tag).toHaveAttribute("data-state", "unfinished");
    expect(tag).not.toHaveTextContent("Published #418");
  });

  it("NAW-FR-KDMW, NAW-FR-XPUJ: a publication the tab waits on is the one that is in progress", async () => {
    // Hold the publish open, so the assertion lands while the call is in
    // flight rather than after it has settled.
    let release: (() => void) | null = null;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const record = publicationRecord(418, "pub-c");
    let view: DraftPublicationView = noPublication();
    stub({ publication: () => view });
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") {
        await held;
        view = { ...view, current: record, history: [record] };
        return { kind: "published", record } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();

    // The publish is deliberately held open, so `findBy*` would sit out its
    // whole timeout flushing a promise that never settles. The tag is already
    // rendered by the time the click has been awaited.
    const tag = screen.getByTestId("draft-publication-tag");
    expect(tag).toHaveAttribute("data-state", "publishing");
    expect(tag).toHaveTextContent("Publishing…");
    expect(tag).toHaveAttribute("title", "A publication is in progress.");
    // The live dot is the application's own signal for work in flight, and
    // this is the one publication state that carries it.
    expect(tag.querySelector(".dot--live")).not.toBeNull();

    await act(async () => {
      release?.();
      await held;
    });
    await waitFor(() =>
      expect(screen.getByTestId("draft-publication-tag")).toHaveTextContent(
        "Published #418",
      ),
    );
  });

  it("NAW-FR-KDMW: abandoning an attempt is not a publication in progress", async () => {
    let release: (() => void) | null = null;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    stub({ publication: { ...noPublication(), attempt: openAttempt() } });
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "cancel_draft_publication_attempt") await held;
      return base(cmd, args);
    });

    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");
    await userEvent.click(within(band).getByRole("button", { name: "Abandon" }));

    // An abandon reaches GitHub for nothing, so it must not read as a
    // publication being made.
    expect(screen.getByTestId("draft-publication-tag")).toHaveAttribute(
      "data-state",
      "unfinished",
    );
    await act(async () => {
      release?.();
      await held;
    });
  });

  it("NAW-FR-CBUJ: a draft with no record, no attempt, and no failure carries no tag", async () => {
    renderWorkspace();
    await publishEntry();
    expect(screen.queryByTestId("draft-publication-tag")).toBeNull();
    expect(screen.queryByTestId("draft-publication-band")).toBeNull();
  });

  it("NAW-FR-KDMW, NAW-FR-QTVX: an attempt in the read is unfinished rather than in progress", async () => {
    stub({
      publication: {
        ...noPublication(),
        attempt: {
          marker: "pub-1",
          remoteName: "origin",
          remoteUrl: "github.com/acme/widgets",
          repositoryOwner: "acme",
          repositoryName: "widgets",
          state: "open",
          startedAt: "2026-09-12T10:00:00.000Z",
          updatedAt: "2026-09-12T10:00:00.000Z",
        },
      },
    });
    renderWorkspace();

    // NAW-FR-KDMW: nothing is working on this attempt — the tab made no call
    // for it — so a tag reading "Publishing…" would say work is happening.
    const condition = "This publication did not finish. Try it again or abandon it.";
    const tag = await screen.findByTestId("draft-publication-tag");
    expect(tag).toHaveAttribute("data-state", "unfinished");
    expect(tag).toHaveTextContent("Publication unfinished");
    expect(tag).not.toHaveTextContent("Publishing");
    // It does not claim the publication failed either: whether the issue was
    // created is exactly what Retry settles.
    expect(tag).not.toHaveTextContent("failed");
    // NAW-FR-QTVX: the condition in the hover text AND in the accessible name.
    // The short label has no room for it, so a reader who cannot see the tag
    // would otherwise get less than a reader who can.
    expect(tag).toHaveAttribute("title", condition);
    expect(screen.getByRole("note", { name: condition })).toBe(tag);
    // It names no issue, so it is not a control: activating it must reach
    // nothing at all.
    expect(tag.tagName).toBe("SPAN");
    expect(tag).not.toHaveAttribute("href");
    await userEvent.click(tag);
    expect(calls("open_publication_issue")).toHaveLength(0);
  });

  it("NAW-FR-QTVX, NAW-FR-XRLD: every tag that names no record states its condition and reaches nothing", async () => {
    const cases = [
      ["awaiting_choice", "Needs a choice", "This publication needs a recovery choice."],
      ["failed", "Publish failed", "The last publication did not finish."],
    ] as const;

    for (const [state, label, condition] of cases) {
      invokeMock.mockReset();
      stub(
        state === "awaiting_choice"
          ? { publication: { ...noPublication(), attempt: openAttempt("awaiting_choice") } }
          : {},
      );
      if (state === "failed") {
        const base = invokeMock.getMockImplementation() as (
          cmd: string,
          args?: unknown,
        ) => Promise<unknown>;
        invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
          if (cmd === "list_publication_remotes") throw new Error("no_github_remote");
          return base(cmd, args);
        });
      }
      renderWorkspace();
      if (state === "failed") await userEvent.click(await publishEntry());

      const tag = await screen.findByTestId("draft-publication-tag");
      expect(tag).toHaveTextContent(label);
      expect(tag).toHaveAttribute("title", condition);
      expect(screen.getByRole("note", { name: condition })).toBe(tag);
      expect(tag.tagName).toBe("SPAN");
      await userEvent.click(tag);
      expect(calls("open_publication_issue")).toHaveLength(0);
      cleanup();
    }
  });

  it("NAW-FR-CBUJ: an awaiting_choice attempt is named ahead of the record it would replace", async () => {
    stub({
      publication: {
        ...noPublication(),
        current: publicationRecord(418, "pub-c"),
        history: [publicationRecord(418, "pub-c")],
        attempt: {
          marker: "pub-2",
          remoteName: "origin",
          remoteUrl: "github.com/acme/widgets",
          repositoryOwner: "acme",
          repositoryName: "widgets",
          state: "awaiting_choice",
          startedAt: "2026-09-12T10:00:00.000Z",
          updatedAt: "2026-09-12T10:00:00.000Z",
        },
      },
    });
    renderWorkspace();

    const tag = await screen.findByTestId("draft-publication-tag");
    expect(tag).toHaveAttribute("data-state", "awaiting_choice");
    expect(tag).toHaveTextContent("Needs a choice");
    expect(tag).not.toHaveTextContent("Published #418");
  });

  it("NAW-FR-LQAF, NAW-FR-NQVD: a standing attempt read back offers Retry and Abandon", async () => {
    stub({
      publication: {
        ...publicationRefused(
          "attempt_in_progress",
          "A publication attempt for this draft is already in progress.",
        ),
        attempt: {
          marker: "pub-1",
          remoteName: "origin",
          remoteUrl: "github.com/acme/widgets",
          repositoryOwner: "acme",
          repositoryName: "widgets",
          state: "open",
          startedAt: "2026-09-12T10:00:00.000Z",
          updatedAt: "2026-09-12T10:00:00.000Z",
        },
      },
    });
    renderWorkspace();

    const band = await screen.findByTestId("draft-publication-band");
    // NAW-FR-XPUJ: the band states the condition the tag states.
    expect(band).toHaveTextContent("This publication did not finish.");
    expect(band).not.toHaveTextContent("A publication is in progress.");
    await userEvent.click(within(band).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(calls("retry_draft_publication")).toHaveLength(1));

    await userEvent.click(within(band).getByRole("button", { name: "Abandon" }));
    await waitFor(() =>
      expect(calls("cancel_draft_publication_attempt")).toHaveLength(1),
    );
  });

  it("NAW-FR-EOTB: a recovery answer reaches the backend, and cancelling leaves the attempt recoverable", async () => {
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github" || cmd === "retry_draft_publication") {
        return {
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["body"],
        } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();

    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });
    expect(dialog).toHaveTextContent("Issue #418");

    await userEvent.click(
      within(dialog).getByRole("button", { name: "Update existing issue" }),
    );
    await waitFor(() =>
      expect(calls("resolve_draft_publication_conflict")).toHaveLength(1),
    );
    expect(calls("resolve_draft_publication_conflict")[0][1]).toEqual({
      draftId: "d1",
      choice: "update_existing",
    });
  });

  it("NAW-FR-EOTB: cancelling the recovery choice adds no history entry and keeps the attempt", async () => {
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") {
        return {
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["body"],
        } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();
    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() =>
      expect(calls("cancel_draft_publication_conflict")).toHaveLength(1),
    );
    expect(calls("resolve_draft_publication_conflict")).toHaveLength(0);
  });

  it("NAW-FR-HZSW: a failed publication is stated inline and changes nothing about the prompt", async () => {
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") throw new Error("github_unreachable");
      if (cmd === "get_draft_publication") {
        return {
          ...noPublication(),
          attempt: {
            marker: "pub-1",
            remoteName: "origin",
            remoteUrl: "github.com/acme/widgets",
            repositoryOwner: "acme",
            repositoryName: "widgets",
            state: "open" as const,
            startedAt: "2026-09-12T10:00:00.000Z",
            updatedAt: "2026-09-12T10:00:00.000Z",
          },
        };
      }
      return base(cmd, args);
    });

    renderWorkspace();
    await screen.findByRole("button", { name: "Draft actions" });
    // Type first, so "changes nothing about the prompt" has something to be
    // true of. Without an edit standing, a zero-save count is true of an
    // implementation that wiped the buffer.
    const typed = "# Widget\n\nstill here after the failure\n";
    const source = await editSource(typed);
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();

    const band = await screen.findByTestId("draft-publication-band");
    expect(within(band).getByRole("alert")).toHaveTextContent("github_unreachable");
    // The keystroke survives the failure, in the buffer the author typed it in.
    expect(source).toHaveValue(typed);
    // The attempt stays recoverable, so the retry affordance is still there.
    expect(within(band).getByRole("button", { name: "Retry" })).toBeInTheDocument();
    // NAW-FR-GKBP writes the prompt before it publishes, so exactly that one
    // write stands and the failure adds none of its own.
    expect(calls("save_draft_file_contents")).toHaveLength(1);
    // The draft's status is what it was: a failure sets none.
    expect(calls("set_draft_status")).toHaveLength(0);
    // NAW-FR-CBUJ: a failure that leaves the attempt open is not work in
    // flight, so the tag says which of the two holds.
    expect(screen.getByTestId("draft-publication-tag")).toHaveAttribute(
      "data-state",
      "failed",
    );
    // NAW-FR-LQAF / NAW-FR-HZSW: and the band says the same thing. A band
    // reading "in progress" under a tag reading "failed" states two conditions
    // for one publication, and one of them is wrong.
    expect(band).toHaveAttribute("data-state", "failed");
    expect(within(band).getByRole("status")).toHaveTextContent(
      "The last publication did not finish.",
    );
    expect(band).not.toHaveTextContent("A publication is in progress.");
  });

  it("NAW-FR-GKBP: an unsaved prompt is written before the issue is published", async () => {
    renderWorkspace();
    await screen.findByRole("button", { name: "Draft actions" });
    // Type into the prompt and publish at once, without waiting the autosave
    // out. The backend publishes the *saved* prompt, so the write has to land
    // first or the issue carries the previous save.
    await editSource("# Widget\n\none more line\n");
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();

    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    const cmds = invokeMock.mock.calls.map(([cmd]) => String(cmd));
    const lastSaveAt = cmds.lastIndexOf("save_draft_file_contents");
    const publishAt = cmds.indexOf("publish_draft_to_github");
    expect(lastSaveAt).toBeGreaterThan(-1);
    expect(lastSaveAt).toBeLessThan(publishAt);
  });

  it("NAW-FR-HZSW: a refusal that never opened an attempt is still stated inline", async () => {
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_publication_remotes") throw new Error("no_github_remote");
      return base(cmd, args);
    });

    renderWorkspace();
    await userEvent.click(await publishEntry());

    // The draft holds no record and no attempt, so the band is what carries the
    // failure; without it the refusal would be silent.
    const band = await screen.findByTestId("draft-publication-band");
    expect(within(band).getByRole("alert")).toHaveTextContent("no_github_remote");
    expect(screen.getByTestId("draft-publication-tag")).toHaveTextContent(
      "Publish failed",
    );
  });

  it("NAW-FR-LQAF, GHP-FR-ZFPI: an awaiting_choice attempt read back offers the recovery route", async () => {
    stub({
      publication: {
        ...publicationRefused(
          "attempt_in_progress",
          "A publication attempt for this draft is already in progress.",
        ),
        attempt: {
          marker: "pub-1",
          remoteName: "origin",
          remoteUrl: "github.com/acme/widgets",
          repositoryOwner: "acme",
          repositoryName: "widgets",
          state: "awaiting_choice",
          startedAt: "2026-09-12T10:00:00.000Z",
          updatedAt: "2026-09-12T10:00:00.000Z",
        },
      },
    });
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "retry_draft_publication") {
        return {
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["body"],
        } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");
    expect(band).toHaveTextContent("This publication needs a recovery choice.");
    // NAW-FR-LQAF: Abandon stands for either attempt state, so an author who
    // will not answer the recovery choice is not stuck with it either.
    expect(within(band).getByRole("button", { name: "Abandon" })).toBeEnabled();
    // The route is reachable rather than the draft being stuck.
    await userEvent.click(within(band).getByRole("button", { name: "Recover" }));
    expect(
      await screen.findByRole("dialog", { name: "Recover publication" }),
    ).toBeInTheDocument();
  });

  it("NAW-FR-EOTB: Publish as a new issue answers with publish_new", async () => {
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") {
        return {
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["body"],
        } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();
    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Publish as a new issue" }),
    );
    await waitFor(() =>
      expect(calls("resolve_draft_publication_conflict")).toHaveLength(1),
    );
    expect(calls("resolve_draft_publication_conflict")[0][1]).toEqual({
      draftId: "d1",
      choice: "publish_new",
    });
  });

  it("NAW-FR-JBHV: two activations in one tick are one attempt", async () => {
    stub({ remotes: fourRemotes() });
    renderWorkspace();
    const entry = await publishEntry();
    // Both clicks land before the enumeration settles.
    await act(async () => {
      entry.click();
      entry.click();
    });
    await waitFor(() => expect(calls("list_publication_remotes").length).toBeGreaterThan(0));
    expect(calls("list_publication_remotes")).toHaveLength(1);
  });

  it("NAW-FR-44, NAW-FR-VBHT: a graduated draft may still be published; a run holding it may not", async () => {
    // NAW-FR-44: publication changes the draft's metadata rather than its
    // prompt, so `graduated` does not disable it while Graduate and Archive
    // are disabled.
    stub({
      status: "graduated",
      graduation: { runId: "run-9", state: "completed", locked: false, graduated: true },
    });
    renderWorkspace();
    let menu = await openActions();
    expect(within(menu).getByRole("menuitem", { name: /Publish to GitHub/ })).toBeEnabled();
    expect(within(menu).getByRole("menuitem", { name: "Graduate" })).toBeDisabled();
    expect(within(menu).getByRole("menuitem", { name: "Archive" })).toBeDisabled();

    cleanup();
    invokeMock.mockReset();
    listeners.clear();
    stub({
      graduation: { runId: "run-9", state: "working", locked: true, graduated: false },
      publication: publicationRefused(
        "draft_locked_by_graduation",
        "A graduation run is working from this draft.",
      ),
    });
    renderWorkspace();
    menu = await openActions();
    const entry = within(menu).getByRole("menuitem", { name: /Publish to GitHub/ });
    expect(entry).toBeDisabled();
    expect(entry).toHaveAttribute("title", "A graduation run is working from this draft.");
  });

  it("NAW-FR-UDPM, NAW-FR-TSQE: a deliberate re-publication leaves the tag naming the new record", async () => {
    const first = publicationRecord(402, "pub-a", "2026-09-04T16:20:00.000Z");
    const second = publicationRecord(418, "pub-b", "2026-09-12T10:04:00.000Z");
    let view: DraftPublicationView = {
      current: first,
      history: [first],
      attempt: null,
      eligibility: noPublication().eligibility,
    };
    stub({ publication: () => view });
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") {
        view = {
          ...view,
          current: second,
          history: [second, first],
        };
        return { kind: "published", record: second } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    expect(await screen.findByTestId("draft-publication-tag")).toHaveTextContent(
      "Published #402",
    );

    await userEvent.click(await publishEntry());
    await confirmPublicationChooser();
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));

    // NAW-FR-TSQE: the tag follows the one read rather than the tab holding a
    // copy of what it published.
    await waitFor(() =>
      expect(screen.getByTestId("draft-publication-tag")).toHaveTextContent(
        "Published #418",
      ),
    );
    // The earlier record is untouched on disk and is read in Draft
    // Information; the tag names the current publication alone.
    expect(view.history).toEqual([second, first]);
    expect(screen.queryByText(/#402/)).toBeNull();
  });
});
