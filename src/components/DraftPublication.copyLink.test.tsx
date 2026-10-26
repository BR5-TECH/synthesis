/**
 * The split publication tag: its open part and its copy part
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-XRLD, NAW-FR-QTVX,
 * NAW-FR-KVFM, NAW-FR-ZBZH, and `NTF-notifications.md` NTF-FR-EPOA,
 * NTF-FR-RQFW).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { DraftPublicationTag } from "./DraftPublication";
import {
  linkCopiedBody,
  linkCopyFailedBody,
  LINK_COPIED_KEY,
  LINK_COPY_FAILED_KEY,
} from "./NewArtifactWorkspace/copyIssueLink";
import { resetToasts, visibleToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import type { UseDraftPublicationResult } from "../hooks/useDraftPublication";
import type { DraftPublicationView, PublicationRecord } from "../types";

const RECORD: PublicationRecord = {
  provider: "github",
  repositoryOwner: "acme",
  repositoryName: "widgets",
  issueNumber: 418,
  issueUrl: "https://github.com/acme/widgets/issues/418",
  publishedAt: "2026-09-12T10:04:00.000Z",
  marker: "pub-c",
};

type Attempt = NonNullable<DraftPublicationView["attempt"]>;

function attempt(state: "open" | "awaiting_choice"): Attempt {
  return {
    marker: "pub-1",
    remoteName: "origin",
    remoteUrl: "github.com/acme/widgets",
    repositoryOwner: "acme",
    repositoryName: "widgets",
    state,
    startedAt: "2026-09-12T10:00:00.000Z",
    updatedAt: "2026-09-12T10:00:00.000Z",
  } as Attempt;
}

/** The hook's result for a draft whose current record is `RECORD`. */
function published(
  overrides: Partial<UseDraftPublicationResult> = {},
): UseDraftPublicationResult {
  return {
    view: {
      current: RECORD,
      history: [RECORD],
      attempt: null,
      eligibility: { publishable: true, reasonCode: null, reason: null, localAssets: [] },
    },
    remotes: null,
    recovery: null,
    chooser: null,
    busy: false,
    publishing: false,
    error: null,
    closedIssue: null,
    update: vi.fn(),
    confirmClosedUpdate: vi.fn(),
    cancelClosedUpdate: vi.fn(),
    begin: vi.fn(),
    closePicker: vi.fn(),
    confirmPicker: vi.fn(),
    closeChooser: vi.fn(),
    confirmChooser: vi.fn(),
    retry: vi.fn(),
    abandon: vi.fn(),
    answerRecovery: vi.fn(),
    cancelRecovery: vi.fn(),
    openIssue: vi.fn(),
    ...overrides,
  };
}

function installClipboard(writeText: (text: string) => Promise<void>) {
  const spy = vi.fn(writeText);
  Object.defineProperty(window.navigator, "clipboard", {
    configurable: true,
    value: { writeText: spy },
  });
  return spy;
}

/** A write the test settles itself, so no result depends on time. */
function deferred() {
  let settle!: { resolve: () => void; reject: (e: unknown) => void };
  const promise = new Promise<void>((resolve, reject) => {
    settle = { resolve, reject };
  });
  return { promise, ...settle };
}

/** The commands sent, without the log flush that runs in the background. */
const sent = () =>
  invokeMock.mock.calls
    .map(([cmd]) => cmd as string)
    .filter((cmd) => cmd !== "append_log_records");

const toastKeys = () => visibleToasts().map((t) => t.key);

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "append_log_records"
      ? Promise.resolve()
      : Promise.reject(new Error(`unexpected command ${cmd}`)),
  );
  resetToasts();
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  Reflect.deleteProperty(window.navigator, "clipboard");
  resetLogBufferForTest();
});

describe("DraftPublicationTag — the copy part", () => {
  it("NAW-FR-KVFM: a tag that names a record is one split control, the open part first", () => {
    render(<DraftPublicationTag publication={published()} />);

    const split = screen.getByTestId("draft-publication-split");
    const open = screen.getByTestId("draft-publication-tag");
    const copy = screen.getByTestId("draft-publication-copy");
    expect(Array.from(split.children)).toEqual([open, copy]);
    expect(open).toHaveTextContent("Published #418");
    expect(open).toHaveAttribute("type", "button");
    expect(copy).toHaveAttribute("type", "button");
    // The open part says it leaves the application; the copy part shows a link.
    expect(open.querySelector("svg")).toHaveAttribute("data-icon", "external-link");
    expect(copy.querySelector("svg")).toHaveAttribute("data-icon", "link");
  });

  it("NAW-FR-QTVX, NAW-FR-ZBZH: each part has its own hover text and accessible name", () => {
    render(<DraftPublicationTag publication={published()} />);

    const copy = screen.getByRole("button", { name: "Copy the link to issue #418" });
    expect(copy).toBe(screen.getByTestId("draft-publication-copy"));
    expect(copy).toHaveAttribute("title", "Copy the link to issue #418");
    const named = `Open issue #418 in acme/widgets, published ${new Date(
      RECORD.publishedAt,
    ).toLocaleString()}`;
    expect(screen.getByRole("button", { name: named })).toBe(
      screen.getByTestId("draft-publication-tag"),
    );
  });

  it("NAW-FR-XRLD, NAW-FR-KVFM, NAW-FR-ZBZH: the copy part writes the URL, opens nothing, and invokes no backend operation", async () => {
    const publication = published();
    render(<DraftPublicationTag publication={publication} />);
    const writeText = installClipboard(async () => {});

    fireEvent.click(screen.getByTestId("draft-publication-copy"));

    await waitFor(() => expect(toastKeys()).toEqual([LINK_COPIED_KEY]));
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith(RECORD.issueUrl);
    // No action of the publication runs, so the draft does not change.
    for (const action of Object.values(publication)) {
      if (vi.isMockFunction(action)) expect(action).not.toHaveBeenCalled();
    }
    expect(sent()).toEqual([]);
  });

  it("NAW-FR-XRLD: the open part opens the issue and writes no clipboard", () => {
    const publication = published();
    render(<DraftPublicationTag publication={publication} />);
    const writeText = installClipboard(async () => {});

    fireEvent.click(screen.getByTestId("draft-publication-tag"));

    expect(publication.openIssue).toHaveBeenCalledTimes(1);
    expect(publication.openIssue).toHaveBeenCalledWith(RECORD.issueUrl);
    expect(writeText).not.toHaveBeenCalled();
    expect(visibleToasts()).toEqual([]);
  });

  it("NAW-FR-ZBZH: while a write runs, further activations are ignored", async () => {
    render(<DraftPublicationTag publication={published()} />);
    const write = deferred();
    const writeText = installClipboard(() => write.promise);
    const copy = screen.getByTestId("draft-publication-copy");

    fireEvent.click(copy);
    fireEvent.click(copy);
    fireEvent.click(copy);
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(visibleToasts()).toEqual([]);

    write.resolve();
    await waitFor(() => expect(toastKeys()).toEqual([LINK_COPIED_KEY]));
    expect(writeText).toHaveBeenCalledTimes(1);
  });

  it("NAW-FR-ZBZH: a write that ends, in success or failure, lets the next activation write again", async () => {
    render(<DraftPublicationTag publication={published()} />);
    let outcome: "ok" | "refused" = "refused";
    const writeText = installClipboard(async () => {
      if (outcome === "refused") throw new DOMException("Denied.", "NotAllowedError");
    });
    const copy = screen.getByTestId("draft-publication-copy");

    fireEvent.click(copy);
    await waitFor(() => expect(toastKeys()).toContain(LINK_COPY_FAILED_KEY));

    outcome = "ok";
    fireEvent.click(copy);
    await waitFor(() => expect(toastKeys()).toContain(LINK_COPIED_KEY));

    fireEvent.click(copy);
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(3));
    // The third write ends inside this test: its success toast replaces the
    // second one, so the revision of that toast goes up.
    await waitFor(() => expect(visibleToasts()[0].revision).toBe(1));
  });

  it.each([
    ["succeeds", async () => {}, LINK_COPIED_KEY, "Info", linkCopiedBody(418)],
    [
      "is refused",
      async () => {
        throw new DOMException("Denied.", "NotAllowedError");
      },
      LINK_COPY_FAILED_KEY,
      "Error",
      linkCopyFailedBody(418),
    ],
  ] as const)(
    "NTF-FR-EPOA, NTF-FR-RQFW: a write that %s shows its toast with no address",
    async (_, writeText, key, level, body) => {
      render(<DraftPublicationTag publication={published()} />);
      installClipboard(writeText);

      fireEvent.click(screen.getByTestId("draft-publication-copy"));

      await waitFor(() => expect(toastKeys()).toEqual([key]));
      expect(visibleToasts()[0]).toEqual(
        expect.objectContaining({ level, body, address: null }),
      );
    },
  );

  it.each([
    ["publishing", published({ publishing: true })],
    ["failed", published({ error: "GitHub is unreachable." })],
    ["unfinished", published({ view: { ...published().view!, attempt: attempt("open") } })],
    [
      "awaiting_choice",
      published({ view: { ...published().view!, attempt: attempt("awaiting_choice") } }),
    ],
  ])("NAW-FR-KVFM, NAW-FR-QTVX: a tag in the %s state has no copy part", (state, publication) => {
    render(<DraftPublicationTag publication={publication} />);

    expect(screen.getByTestId("draft-publication-tag")).toHaveAttribute("data-state", state);
    expect(screen.queryByTestId("draft-publication-copy")).toBeNull();
    expect(screen.queryByTestId("draft-publication-split")).toBeNull();
  });
});
