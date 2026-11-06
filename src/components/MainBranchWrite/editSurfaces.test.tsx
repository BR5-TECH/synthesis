/**
 * The hold of the first change in each editing surface, rendered with the
 * standalone confirmation (`EDT-editor.md` EDT-FR-QXEK, EDT-FR-UPNU;
 * `MBW-main-branch-write.md` MBW-FR-DBGA, MBW-FR-ILFN).
 *
 * The guard reads are scripted. They answer as the backend does: a confirm is
 * recorded, and a later read answers from that record. Each wait is for a
 * state, never for a time.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { MainBranchWriteHost } from ".";
import { Editor } from "../Editor";
import { EditSessionStore } from "../../state/editSessions";
import {
  currentMainBranchWrite,
  holdEdit,
  isEditGateReading,
  primeEditGate,
} from "../../state/mainBranchWrite";
import { resetLogBufferForTest } from "../../logging";
import { resetDiffModes } from "../../state/diffModes";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { hunkCandidateBuffers } from "../../state/candidateBuffers";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { mainGuard, scriptedReads } from "../../test/mainBranchWriteFixtures";
import { otherCommand } from "../../test/invokeMocks";
import {
  awaitDiff,
  makeWireBackend,
  renderDiff,
  revs,
  target,
  targetCell,
  typeInto,
} from "../../test/diffViewFixtures";
import { makeStubs, PROMPT, renderWorkspace } from "../../test/newArtifactFixtures";
import { draftDocKey } from "../../state/documentTransport";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetDiffModes();
  resetAppPreferencesCache();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  invokeMock.mockReset();
});
afterEach(cleanup);

/** The gate read has landed. */
const gateRead = () => waitFor(() => expect(isEditGateReading()).toBe(false));

const confirmDialog = () => screen.findByRole("dialog", { name: /Write to/ });

async function confirmWrite() {
  fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
  await waitFor(() => expect(currentMainBranchWrite()).toBeNull());
}

describe("the gate and the standalone confirmation (MBW-FR-ILFN)", () => {
  it("EDT-FR-QXEK, MBW-FR-XBJS: a confirm in the rendered dialog opens the gate, so the next change is not held", async () => {
    const reads = scriptedReads(mainGuard());
    render(<MainBranchWriteHost />);
    primeEditGate();
    await gateRead();

    expect(holdEdit()).toBe(true);
    await confirmDialog();
    await confirmWrite();

    expect(reads.confirms).toEqual(["main"]);
    expect(holdEdit()).toBe(false);
  });
});

describe("each editing surface holds its first change (EDT-FR-QXEK, EDT-FR-UPNU)", () => {
  it("EDT-FR-QXEK: the source editor of a file does not take the first change, shows the confirmation, and takes the next change after a confirm", async () => {
    scriptedReads(mainGuard());
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return Promise.resolve({ body: "first\n", checksum: "ck1" });
      if (cmd === "save_artifact_contents") return Promise.resolve({ checksum: "ck2" });
      return otherCommand(cmd);
    });
    const sessions = new EditSessionStore();
    render(
      <>
        <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />
        <MainBranchWriteHost />
      </>,
    );
    await screen.findByLabelText("artifact body");
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = screen.getByLabelText("Markdown source") as HTMLTextAreaElement;
    await gateRead();

    fireEvent.change(source, { target: { value: "first\nheld\n" } });
    await confirmDialog();
    expect(source.value).toBe("first\n");
    expect(sessions.get("a.md")?.buffer).toBe("first\n");

    await confirmWrite();
    fireEvent.change(source, { target: { value: "first\napplied\n" } });
    expect(source.value).toBe("first\napplied\n");
    expect(sessions.get("a.md")?.buffer).toBe("first\napplied\n");
  });

  it("EDT-FR-UPNU: the editable target of a Diff tab does not take the first change, shows the confirmation, and takes the next change after a confirm", async () => {
    scriptedReads(mainGuard());
    makeWireBackend(invokeMock)({ revisions: revs("one\nTWO\nthree\n", "one\ntwo\nthree\n") });
    const { sessions } = renderDiff(target("notes.md"));
    render(<MainBranchWriteHost />);
    await awaitDiff();
    await gateRead();

    typeInto(targetCell("two"), "two held");
    await confirmDialog();
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo\nthree\n");

    await confirmWrite();
    typeInto(targetCell("two held"), "two applied");
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo applied\nthree\n");
  });

  it("EDT-FR-UPNU: the draft prompt editor of the New Artifact tab does not take the first change, shows the confirmation, and takes the next change after a confirm", async () => {
    scriptedReads(mainGuard());
    makeStubs(invokeMock).stub();
    const { drafts } = renderWorkspace();
    render(<MainBranchWriteHost />);
    fireEvent.click(await screen.findByRole("button", { name: "Edit as Markdown source" }));
    const source = screen.getByLabelText("Markdown source") as HTMLTextAreaElement;
    await gateRead();
    const before = source.value;

    fireEvent.change(source, { target: { value: `${before}held\n` } });
    await confirmDialog();
    expect(source.value).toBe(before);

    await confirmWrite();
    fireEvent.change(source, { target: { value: `${before}applied\n` } });
    expect(source.value).toBe(`${before}applied\n`);
    expect(drafts.docs.get(draftDocKey("d1", PROMPT))?.buffer).toBe(`${before}applied\n`);
  });
});
