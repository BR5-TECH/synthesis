/**
 * Publishing to GitHub when the project token and the remote are on different
 * hosts (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-VBHT,
 * `../../specifications/core/GHP-github-publication.md` GHP-FR-MZPR,
 * `../../specifications/ui/GHA-github-authentication.md` GHA-FR-LBLM).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  fourRemotes,
  makeStubs,
  openActions,
  publicationRefused,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type { DraftPublicationView } from "../types";

const { stub } = makeStubs(invokeMock);

const calls = (name: string) =>
  invokeMock.mock.calls.filter(([cmd]) => cmd === name);

/** The **Publish to GitHub** entry of the tab's one action control. */
async function publishEntry() {
  const menu = await openActions();
  return within(menu).getByRole("menuitem", { name: /Publish to GitHub/ });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

describe("New Artifact workspace — publishing across GitHub hosts", () => {
  it("NAW-FR-VBHT: a token of another host refuses the action and states why", async () => {
    const reason =
      "The project token belongs to another GitHub host than this remote. Pick or add a token for the host of the remote.";
    stub({
      publication: publicationRefused(
        "github_host_mismatch" as DraftPublicationView["eligibility"]["reasonCode"] & string,
        reason,
      ),
    });
    renderWorkspace();
    const entry = await publishEntry();
    expect(entry).toBeDisabled();
    expect(entry).toHaveAttribute("title", reason);
  });

  it("GHP-FR-MZPR, GHA-FR-LBLM: a host_mismatch remote is listed disabled with its reason", async () => {
    const resolution = fourRemotes();
    resolution.remotes[1] = {
      ...resolution.remotes[1],
      repositoryHost: "company.ghe.com",
      eligibility: "host_mismatch",
      reason: "The project token belongs to another GitHub host than this remote.",
    };
    stub({ remotes: resolution });
    renderWorkspace();
    await userEvent.click(await publishEntry());

    const dialog = await screen.findByRole("dialog", { name: "Publish to GitHub" });
    const radios = within(dialog).getAllByRole("radio");
    expect(radios[1]).toBeDisabled();
    expect(
      within(dialog).getByText(
        "The project token belongs to another GitHub host than this remote.",
      ),
    ).toBeInTheDocument();
  });
});
