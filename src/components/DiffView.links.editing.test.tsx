// The links of a Diff tab's editable Rich target, through the tab's own host
// (`DFV-diff-viewer.md` DFV-FR-QJRW).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { DiffView } from "./DiffView";
import { EditSessionStore } from "../state/editSessions";
import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";
import { makeWireBackend, revs, target } from "../test/diffViewFixtures";
import { LinkOpenerContext, type LinkOpener } from "./linkFollow";
import { resetToasts } from "../state/toasts";

const wireBackend = makeWireBackend(invokeMock);
const TEXT = "# Usage\n\nSee [b](b.md) and [go](#usage).\n";

let shell: LinkOpener;
let scrolled: Element[];
const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView");

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetDiffModes();
  resetAppPreferencesCache();
  resetToasts();
  scrolled = [];
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value(this: Element) {
      scrolled.push(this);
    },
  });
  shell = {
    roots: ["/p"],
    openFile: vi.fn(),
    takePendingFragment: vi.fn(() => null),
    activeFileChanged: vi.fn(),
  };
});

afterEach(() => {
  cleanup();
  if (original) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", original);
  else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
});

/** The link `text` of the editable run, once it has mounted. */
async function runLink(text: string): Promise<HTMLAnchorElement> {
  let found: HTMLAnchorElement | undefined;
  await waitFor(() => {
    found = Array.from(document.querySelectorAll<HTMLAnchorElement>(".diff-run a[href]")).find(
      (a) => a.textContent === text,
    );
    expect(found).toBeDefined();
  });
  return found!;
}

describe("the editable Rich target", () => {
  it("DFV-FR-QJRW, EDT-FR-YHKG, EDT-FR-RFCU: a modifier click follows a file link against the compared file, a plain click does not", async () => {
    wireBackend({ revisions: revs(TEXT, `${TEXT}\nMore.\n`), prefs: { diffRenderingMode: "rich" } });
    render(
      <LinkOpenerContext.Provider value={shell}>
        <DiffView target={target("docs/a.md")} sessions={new EditSessionStore()} />
      </LinkOpenerContext.Provider>,
    );
    const b = await runLink("b");
    fireEvent.click(b, { button: 0 });
    expect(shell.openFile).not.toHaveBeenCalled();
    fireEvent.click(b, { button: 0, ctrlKey: true });
    expect(shell.openFile).toHaveBeenCalledWith("docs/b.md", null);
    // EDT-FR-BBIF: the editable target shows the gesture hint.
    expect(b.querySelector("[title]")?.getAttribute("title")).toBe("b.md\nCtrl+click to follow");
  });

  it("DFV-FR-QJRW, EDT-FR-HMJI: a fragment in the editable target scrolls to the heading of the target", async () => {
    wireBackend({ revisions: revs(TEXT, `${TEXT}\nMore.\n`), prefs: { diffRenderingMode: "rich" } });
    render(
      <LinkOpenerContext.Provider value={shell}>
        <DiffView target={target("docs/a.md")} sessions={new EditSessionStore()} />
      </LinkOpenerContext.Provider>,
    );
    fireEvent.click(await runLink("go"), { button: 0, ctrlKey: true });
    expect(scrolled).toHaveLength(1);
    expect(scrolled[0].textContent).toBe("Usage");
    expect(scrolled[0].closest(".diff-rich")).not.toBeNull();
    expect(shell.openFile).not.toHaveBeenCalled();
    expect(screen.queryByText("Link target not found")).toBeNull();
  });
});
