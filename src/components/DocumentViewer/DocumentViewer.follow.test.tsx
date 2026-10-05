// The Document text viewer follows the collection (`DTV-document-text-viewer.md`
// DTV-FR-SAIG, DTV-FR-BEQL, DTV-FR-EKFD): a changed revision reloads the text,
// and an unavailable document replaces the content.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetViewStateForTest } from "../../state/documentViewState";
import { DocumentTextViewer } from ".";
import {
  ID,
  documentText,
  emitDocuments,
  entry,
  invokeMock,
  listenerCount,
  readCalls,
  readReturns,
  resetFixtures,
} from "./viewerFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./viewerFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./viewerFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("./viewerFixtures")).loggingMock);

beforeEach(() => {
  resetFixtures();
  resetViewStateForTest();
});
afterEach(cleanup);

const viewer = () => <DocumentTextViewer documentId={ID} />;
const region = () => screen.getByRole("region", { name: "Document text" });

describe("DTV-FR-SAIG: a changed revision", () => {
  it("DTV-FR-SAIG: reads the new text and replaces the view when the revision differs", async () => {
    readReturns(documentText("# First", "markdown", "r1"));
    render(viewer());
    expect(
      await screen.findByRole("heading", { level: 1, name: "First" }),
    ).toBeInTheDocument();
    readReturns(documentText("# Second", "markdown", "r2"));
    await emitDocuments([entry({ revision: "r2" })]);
    expect(
      await screen.findByRole("heading", { level: 1, name: "Second" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "First" })).toBeNull();
    expect(readCalls()).toHaveLength(2);
  });

  it("DTV-FR-SAIG: the same revision reads nothing again", async () => {
    readReturns(documentText("# First", "markdown", "r1"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    await emitDocuments([entry({ revision: "r1" })]);
    await emitDocuments([entry({ revision: "r1", path: "/refs/other/notes.md" })]);
    expect(readCalls()).toHaveLength(1);
  });

  it("DTV-FR-SAIG: the Rich or Source choice and the text region stay as they were, and the old text stays on show while the new text loads", async () => {
    readReturns(documentText("# First", "markdown", "r1"));
    const user = userEvent.setup();
    render(viewer());
    await user.click(await screen.findByRole("radio", { name: "Source" }));
    const before = region();
    before.scrollTop = 40;
    let release: (value: unknown) => void = () => {};
    invokeMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    await emitDocuments([entry({ revision: "r2" })]);
    // No flash: the old text and the same region are still there.
    expect(region()).toBe(before);
    expect(region().textContent).toBe("# First");
    expect(screen.queryByText("Loading document…")).toBeNull();
    release(documentText("# Second", "markdown", "r2"));
    await waitFor(() => expect(region().textContent).toBe("# Second"));
    expect(region()).toBe(before);
    expect(screen.getByRole("radio", { name: "Source" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("DTV-FR-SAIG: a text document reloads in the source view", async () => {
    readReturns(documentText("one", "text", "r1"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    readReturns(documentText("two\nlines", "text", "r2"));
    await emitDocuments([entry({ format: "text", revision: "r2" })]);
    await waitFor(() => expect(region().textContent).toBe("two\nlines"));
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("DTV-FR-SAIG: the viewer follows only the event and stops following when it closes", async () => {
    readReturns(documentText("x", "text", "r1"));
    const view = render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    expect(listenerCount()).toBe(1);
    expect(invokeMock.mock.calls.every((c) => c[0] === "read_document")).toBe(true);
    view.unmount();
    await waitFor(() => expect(listenerCount()).toBe(0));
  });
});

describe("DTV-FR-BEQL: the unavailable state follows the collection", () => {
  it("DTV-FR-BEQL: an entry that turns unavailable replaces the content and keeps no stale text", async () => {
    readReturns(documentText("# Secret text"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    await emitDocuments([entry({ status: "unavailable", revision: undefined })]);
    expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
    expect(screen.getByText(/may have been moved, deleted/)).toBeInTheDocument();
    expect(screen.queryByText("Secret text")).toBeNull();
    expect(screen.queryByRole("region")).toBeNull();
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("DTV-FR-BEQL: a document that is no longer in the collection shows the unavailable state", async () => {
    readReturns(documentText("# Gone"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    await emitDocuments([]);
    expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
    expect(screen.queryByRole("region")).toBeNull();
  });

  it("DTV-FR-BEQL: an event that lists other documents only leaves the viewer as it is", async () => {
    readReturns(documentText("# Mine"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    await emitDocuments([
      entry({ revision: "r1" }),
      entry({ id: "doc-ffffffffffffffffffffffffffffffff", path: "/x/y.md", revision: "zzz" }),
    ]);
    expect(readCalls()).toHaveLength(1);
    expect(screen.getByRole("region", { name: "Document text" })).toBeInTheDocument();
  });
});

describe("DTV-FR-EKFD: an unavailable viewer stays open", () => {
  it("DTV-FR-EKFD: reads and shows the text when the document becomes available again with a revision", async () => {
    readReturns(documentText("# Back"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    await emitDocuments([entry({ status: "unavailable", revision: undefined })]);
    await screen.findByText("This document is unavailable.");
    readReturns(documentText("# Again", "markdown", "r9"));
    await emitDocuments([entry({ revision: "r9" })]);
    expect(
      await screen.findByRole("heading", { level: 1, name: "Again" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("This document is unavailable.")).toBeNull();
  });

  it("DTV-FR-EKFD: stays unavailable while the entry is still unavailable, and does not read", async () => {
    readReturns(documentText("# Back"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    await emitDocuments([entry({ status: "unavailable", revision: undefined })]);
    await emitDocuments([entry({ status: "unavailable", revision: undefined })]);
    expect(screen.getByText("This document is unavailable.")).toBeInTheDocument();
    expect(readCalls()).toHaveLength(1);
  });

  it("DTV-FR-EKFD, DTV-FR-XMRL: the Rich or Source choice survives an unavailable period", async () => {
    readReturns(documentText("# Back"));
    const user = userEvent.setup();
    render(viewer());
    await user.click(await screen.findByRole("radio", { name: "Source" }));
    await emitDocuments([entry({ status: "unavailable", revision: undefined })]);
    await screen.findByText("This document is unavailable.");
    readReturns(documentText("# Again", "markdown", "r9"));
    await emitDocuments([entry({ revision: "r9" })]);
    expect(await screen.findByRole("radio", { name: "Source" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(region().textContent).toBe("# Again");
  });
});
