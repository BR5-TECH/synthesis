/**
 * Revealing a draft another surface named by id
 * (`../../specifications/ui/DRP-drafts-panel.md`, DRP-FR-34).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import type { Mock } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
} from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The panel opens surfaces that subscribe to backend channels — the Information
 * modal listens for `"draft statistics changed"` (DFI-FR-ZGBU) — and `listen`
 * reaches for a Tauri IPC context that does not exist under jsdom. Mocked
 * beside `invoke` for the same reason `invoke` is: a UI test must not need the
 * Tauri runtime, and an un-mocked subscription rejects on a microtask nothing
 * in the test is awaiting, which fails the run without failing an assertion.
 */
const listenMock = vi.fn(async (_name: string, _handler: unknown) => () => {});
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: unknown) => listenMock(name, handler),
}));

import {
  DRAFTS,
  flat,
  nested,
  renderPanel,
} from "../test/draftsPanelFixtures";
import { selectorValue } from "../test/selectors";
import { resetPanelReveals } from "../state/panelReveal";

/** Every `invoke` of one operation, in the order they were made. */
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "list_drafts") return flat();
    // DRS-FR-17: the real backend matches names *and* file contents. This
    // stand-in matches names alone, which is enough for every test that is not
    // about content matching; the ones that are override it.
    if (cmd === "search_drafts") {
      const text = String((args as { text: string }).text).toLowerCase();
      return DRAFTS.filter((d) => d.name.toLowerCase().includes(text)).map(
        (d) => ({ draftId: d.id, matchedIn: "name" }),
      );
    }
    return undefined;
  });
});
afterEach(cleanup);

/**
 * DRP-FR-34 / DRP-FR-30, DRP-FR-14, DRP-FR-07: revealing a draft another surface named by id.
 *
 * The one caller today is the shell following the active tab (SNV-FR-64), which
 * is why every case below is phrased as a request arriving at a mounted panel
 * rather than as a prop the panel was born with.
 */
describe("reveal a draft by id (DRP-FR-34)", () => {
  /**
   * jsdom implements `scrollIntoView` not at all — which is why the panel calls
   * it optionally — so `vi.spyOn` cannot be used and the stub is installed by
   * assignment, then removed rather than left on the prototype for the next file
   * to inherit.
   */
  let scrollSpy: Mock<() => void>;
  beforeEach(() => {
    scrollSpy = vi.fn();
    (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView = scrollSpy;
  });
  afterEach(() => {
    delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
  });

  it("expands every ancestor folder and selects the row, scrolled into view", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return nested();
      return undefined;
    });
    // `UI` and `UI/Components` both collapsed, and something else selected.
    const { revealDraft } = renderPanel({ expanded: [] });
    await screen.findByRole("treeitem", { name: "Folder UI" });
    expect(screen.queryByRole("treeitem", { name: /^Draft button-lens/ })).toBeNull();

    act(() => revealDraft("d-button"));

    const row = await screen.findByRole("treeitem", {
      name: /^Draft button-lens/,
    });
    expect(row).toHaveAttribute("aria-selected", "true");
    // DRP-FR-34: the reveal ends with the row in view. jsdom implements no
    // layout, so the call itself is what can be observed here.
    await waitFor(() => expect(scrollSpy).toHaveBeenCalled());
    // SNV-FR-64: a *followed* row is not focused — the author's focus intent is
    // the tab they just activated, and taking it here would mean their next
    // keystroke moved this panel's selection instead.
    expect(row).not.toHaveFocus();
    // Only the ancestors it needed: `backend` is nobody's ancestor here.
    expect(screen.getByRole("treeitem", { name: "Folder backend" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("DRP-FR-30: a reveal the author asked for directly does land focus on the row", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return nested();
      return undefined;
    });
    const { revealDraft } = renderPanel({ expanded: [] });
    await screen.findByRole("treeitem", { name: "Folder UI" });

    act(() => revealDraft("d-button", { focus: true }));

    const row = await screen.findByRole("treeitem", {
      name: /^Draft button-lens/,
    });
    await waitFor(() => expect(row).toHaveFocus());
  });

  it("moves the status filter to all drafts when it is hiding the row", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return flat();
      return undefined;
    });
    // The default position: an archived draft is not in the tree at all.
    const { revealDraft } = renderPanel({ filter: "active" });
    await screen.findByRole("treeitem", { name: /^Draft editor-tweaks/ });
    expect(screen.queryByRole("treeitem", { name: /^Draft artifact-window/ })).toBeNull();

    act(() => revealDraft("d-archived"));

    const row = await screen.findByRole("treeitem", {
      name: /^Draft artifact-window/,
    });
    expect(row).toHaveAttribute("aria-selected", "true");
    // **all drafts** rather than **archived**: nothing else leaves the tree to
    // make room for the revealed row.
    expect(selectorValue("Draft status")).toBe("all");
    expect(
      screen.getByRole("treeitem", { name: /^Draft editor-tweaks/ }),
    ).toBeInTheDocument();
  });

  it("clears a text filter that is hiding the row, and only when it is", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts") return flat();
      if (cmd === "search_drafts") {
        const text = String((args as { text: string }).text).toLowerCase();
        return DRAFTS.filter((d) => d.name.toLowerCase().includes(text)).map(
          (d) => ({ draftId: d.id, matchedIn: "name" }),
        );
      }
      return undefined;
    });
    const { revealDraft } = renderPanel({ filter: "all", text: "library" });
    // Wait for the debounced search to actually land: until it does, `matches`
    // is null and the tree admits everything, so asserting a row is absent would
    // pass on an unrendered panel rather than on a filtered one.
    await screen.findByRole("treeitem", { name: /^Draft library-lens/ });
    await waitFor(() =>
      expect(screen.queryByRole("treeitem", { name: /^Draft editor-tweaks/ })).toBeNull(),
    );

    act(() => revealDraft("d-active"));

    await screen.findByRole("treeitem", { name: /^Draft editor-tweaks/ });
    expect(screen.getByLabelText("Filter drafts")).toHaveValue("");
  });

  it("leaves both filters alone when neither is hiding the row", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return flat();
      return undefined;
    });
    const { revealDraft } = renderPanel({ filter: "active" });
    await screen.findByRole("treeitem", { name: /^Draft editor-tweaks/ });

    act(() => revealDraft("d-active"));

    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft editor-tweaks/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );
    expect(selectorValue("Draft status")).toBe("active");
    expect(screen.getByLabelText("Filter drafts")).toHaveValue("");
  });

  it("SNV-FR-67: an id the tree does not hold changes nothing and says nothing", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return flat();
      return undefined;
    });
    const { revealDraft } = renderPanel({ filter: "active" });
    const first = await screen.findByRole("treeitem", {
      name: /^Draft editor-tweaks/,
    });
    fireEvent.click(first);
    await waitFor(() => expect(first).toHaveAttribute("aria-selected", "true"));

    act(() => revealDraft("d-deleted"));

    // The author's own selection, the filter, and the tree are all as they were,
    // and nothing was rendered to explain it.
    expect(
      screen.getByRole("treeitem", { name: /^Draft editor-tweaks/ }),
    ).toHaveAttribute("aria-selected", "true");
    expect(selectorValue("Draft status")).toBe("active");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("DRP-FR-34: revealing opens no draft and invokes no operation", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return nested();
      return undefined;
    });
    const onOpenDraft = vi.fn();
    const { revealDraft } = renderPanel({ onOpenDraft });
    await screen.findByRole("treeitem", { name: "Folder UI" });
    const before = invokeMock.mock.calls.length;

    act(() => revealDraft("d-window"));
    await screen.findByRole("treeitem", { name: /^Draft artifact-window/ });

    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(invokeMock.mock.calls.length).toBe(before);
  });

  it("SNV-FR-68: a second request for the same draft re-selects it", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return flat();
      return undefined;
    });
    const { revealDraft } = renderPanel({ filter: "all" });
    await screen.findByRole("treeitem", { name: /^Draft editor-tweaks/ });

    act(() => revealDraft("d-active"));
    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft editor-tweaks/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );

    // The author selects something else, and then returns to that tab.
    fireEvent.click(screen.getByRole("treeitem", { name: /^Draft library-lens/ }));
    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft library-lens/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );

    act(() => revealDraft("d-active"));

    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft editor-tweaks/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );
  });

  it("DRP-FR-14: a re-list does not re-assert a finished reveal", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return flat();
      return undefined;
    });
    const { revealDraft, bump } = renderPanel({ filter: "all" });
    await screen.findByRole("treeitem", { name: /^Draft editor-tweaks/ });

    act(() => revealDraft("d-active"));
    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft editor-tweaks/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );

    fireEvent.click(screen.getByRole("treeitem", { name: /^Draft library-lens/ }));
    await waitFor(() =>
      expect(
        screen.getByRole("treeitem", { name: /^Draft library-lens/ }),
      ).toHaveAttribute("aria-selected", "true"),
    );

    // A `"drafts changed"` event re-lists the tree. The author's selection must
    // survive it — the reveal is long finished.
    act(() => bump(1));
    await waitFor(() => expect(calls("list_drafts").length).toBeGreaterThan(1));

    expect(
      screen.getByRole("treeitem", { name: /^Draft library-lens/ }),
    ).toHaveAttribute("aria-selected", "true");
  });
});
