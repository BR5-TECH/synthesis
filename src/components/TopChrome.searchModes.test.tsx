import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAppPreferencesCache } from "../state/appPreferences";
import { QUERY_MODES } from "../hooks/useSearchQueryMode";
import { createEventBus, hit, resetHitOrdinals } from "../test/searchEvents";
import {
  Harness,
  escapeRegExp,
  overlayMarker,
  searchInput,
  streamResults as streamInto,
  topChromeInvoke,
} from "../test/topChromeHarness";
import type { SearchHit } from "../types";

// The query modes of the search bar, the Semantic results, and the route to
// the full results tab (SCH-FR-08, SCH-FR-12 to SCH-FR-14, SCH-FR-LTSO,
// SCH-FR-RAAU, SCH-FR-IESG). The harness and the stubs are those of
// `TopChrome.test.tsx`.
const bus = createEventBus();
let searchIds: string[] = [];
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) =>
    bus.listen(name, handler),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
  }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}));

/** Dispatch a query and stream `hits` into the overlay, then end the search. */
const streamResults = (
  user: ReturnType<typeof userEvent.setup>,
  query: string,
  hits: SearchHit[],
) => streamInto(bus, searchIds, user, query, hits);

beforeEach(() => {
  bus.reset();
  searchIds = [];
  resetHitOrdinals();
  resetAppPreferencesCache();
  invokeMock.mockReset();
  invokeMock.mockImplementation(topChromeInvoke(searchIds));
});

describe("Semantic results (SCH-FR-LTSO / SCH-FR-RAAU)", () => {
  afterEach(cleanup);

  it("SCH-FR-LTSO, SCH-FR-RAAU: a Semantic hit shows its chunk as the snippet, with no line number", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "teardown", [
      {
        id: "doc-1",
        documentId: "doc-1",
        name: "manual.md",
        path: "/refs/manual.md",
        ordinal: 0,
        group: "document",
        matchKind: "content",
        snippet: "How a worktree is torn down.",
      },
    ]);
    expect(screen.getByText("Documents")).toBeInTheDocument();
    expect(screen.getByText("/refs/manual.md")).toBeInTheDocument();
    const snippet = screen.getByText("How a worktree is torn down.");
    expect(snippet.closest(".search-result__snippet--chunk")).not.toBeNull();
    expect(document.querySelector(".search-result__line")).toBeNull();
  });
});

describe("activating Semantic hits in the overlay (SCH-FR-RAAU)", () => {
  afterEach(cleanup);
  const semanticHits = () => [
    hit({
      id: "specifications/a.md",
      path: "specifications/a.md",
      group: "artifact",
      subtype: "spec",
      line: undefined,
      snippet: "How a worktree is torn down.",
    }),
    hit({
      id: "doc-1",
      documentId: "doc-1",
      name: "manual.md",
      path: "/refs/manual.md",
      group: "document",
      line: undefined,
      snippet: "Rate limits of the interface.",
    }),
    hit({
      id: "draft-1",
      draftId: "draft-1",
      name: "Search",
      path: "Core",
      group: "draft",
      line: undefined,
      snippet: "Hybrid retrieval for search.",
    }),
  ];

  it.each([
    ["manual.md", { group: "document", documentId: "doc-1" }],
    ["Search", { group: "draft", draftId: "draft-1" }],
    ["a.md", { group: "artifact", subtype: "spec", id: "specifications/a.md" }],
  ])("SCH-FR-RAAU: activating the %s hit routes it with its ids", async (name, expected) => {
    const user = userEvent.setup();
    const onActivateSearchHit = vi.fn();
    render(<Harness onActivateSearchHit={onActivateSearchHit} />);
    await streamResults(user, "teardown", semanticHits());
    await user.click(screen.getByText(name));
    expect(onActivateSearchHit).toHaveBeenCalledWith(expect.objectContaining(expected));
  });
});

describe("Semantic mode with embeddings off (SCH-FR-IESG)", () => {
  afterEach(cleanup);

  it("SCH-FR-IESG: results show with no error and no special state", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "teardown", [
      hit({
        id: "specifications/a.md",
        path: "specifications/a.md",
        group: "artifact",
        subtype: "spec",
        matchKind: "content",
        line: undefined,
        snippet: "How a worktree is torn down.",
      }),
    ]);
    expect(screen.getByText("specifications/a.md")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(document.querySelector(".search-state--error")).toBeNull();
  });
});

describe("query-mode toggles (SCH-FR-12 / SCH-FR-13 / SCH-FR-14)", () => {
  afterEach(cleanup);

  it("SCH-FR-12: exactly one toggle is active, and activating another makes it the only one", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const toggles = within(screen.getByTestId("query-mode-toggles")).getAllByRole(
      "radio",
    );
    expect(toggles).toHaveLength(4);
    expect(toggles.filter((t) => t.getAttribute("aria-checked") === "true")).toHaveLength(1);

    await user.click(screen.getByRole("radio", { name: /Regular expression/i }));

    const after = within(screen.getByTestId("query-mode-toggles")).getAllByRole("radio");
    expect(after.filter((t) => t.getAttribute("aria-checked") === "true")).toHaveLength(1);
    expect(
      screen.getByRole("radio", { name: /Regular expression/i }),
    ).toHaveAttribute("aria-checked", "true");
  });

  it("each toggle describes what it does, on hover and to assistive tech", async () => {
    // `Aa` and `aA` are a character apart and mean genuinely different things,
    // so the difference has to be readable somewhere other than the glyph.
    render(<Harness />);
    const group = within(screen.getByTestId("query-mode-toggles"));

    for (const option of QUERY_MODES) {
      const toggle = group.getByRole("radio", {
        name: new RegExp(escapeRegExp(option.title), "i"),
      });
      const tip = toggle.querySelector(".search-input__tip");
      expect(tip, `${option.title} carries a tooltip`).not.toBeNull();
      // It says what the mode DOES, not just what it is called.
      expect(tip).toHaveTextContent(option.description);
      // The same text reaches assistive tech through the accessible name, and
      // the tooltip itself is hidden from it rather than announced twice.
      expect(tip).toHaveAttribute("aria-hidden", "true");
      expect(toggle).toHaveAttribute(
        "aria-label",
        `${option.title} — ${option.description}`,
      );
    }

    // And the four descriptions are distinct — a shared blurb would leave the
    // two literal modes indistinguishable, which is the actual confusion.
    expect(new Set(QUERY_MODES.map((m) => m.description)).size).toBe(4);
  });

  it("SCH-FR-13: the mode is read from app preferences on mount, defaulting to Semantic", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences"
        ? { theme: "system", searchQueryMode: "regex" }
        : undefined,
    );
    render(<Harness />);

    await waitFor(() =>
      expect(
        screen.getByRole("radio", { name: /Regular expression/i }),
      ).toHaveAttribute("aria-checked", "true"),
    );

    // And a user who has never chosen one starts in Semantic.
    cleanup();
    resetAppPreferencesCache();
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences" ? { theme: "system" } : undefined,
    );
    render(<Harness />);
    await waitFor(() =>
      expect(
        screen.getByRole("radio", { name: /^Semantic/i }),
      ).toHaveAttribute("aria-checked", "true"),
    );
  });

  it("SCH-FR-13 / GLS-FR-14: changing the mode patches the record rather than replacing it", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences"
        ? { theme: "dark", mainWindowFullscreen: true }
        : undefined,
    );
    render(<Harness />);
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "load_app_preferences")).toBe(true),
    );

    await user.click(screen.getByRole("radio", { name: /Smart case/i }));

    await waitFor(() => {
      const save = invokeMock.mock.calls.find((c) => c[0] === "save_app_preferences");
      expect(save?.[1]?.preferences).toEqual({
        theme: "dark",
        mainWindowFullscreen: true,
        searchQueryMode: "smart_case",
        diffVisualizationMode: "unified",
        diffRenderingMode: "source",
        changesCommitAction: "commit",
        notificationsEnabled: true,
        selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
      });
    });
  });

  it("SCH-FR-14: changing the mode re-dispatches immediately, without waiting for a typing pause", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", [hit({ path: "src/a.rs" })]);
    expect(searchIds).toHaveLength(1);

    await user.click(screen.getByRole("radio", { name: /Regular expression/i }));

    await waitFor(() => expect(searchIds).toHaveLength(2));
    const dispatches = invokeMock.mock.calls.filter((c) => c[0] === "start_search");
    expect(dispatches[dispatches.length - 1][1]).toEqual({
      query: "needle",
      mode: "regex",
      scope: "capped",
    });
  });
});

describe("escalating to the full results tab (SCH-FR-08)", () => {
  afterEach(cleanup);

  it("SCH-FR-08: Enter opens the Search results tab and closes the overlay", async () => {
    const user = userEvent.setup();
    const onOpenSearchResults = vi.fn();
    render(<Harness onOpenSearchResults={onOpenSearchResults} />);
    await user.type(searchInput(), "needle");

    await user.keyboard("{Enter}");

    expect(onOpenSearchResults).toHaveBeenCalledWith("needle", "semantic");
    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("Enter on an empty query opens nothing", async () => {
    const user = userEvent.setup();
    const onOpenSearchResults = vi.fn();
    render(<Harness onOpenSearchResults={onOpenSearchResults} />);
    await user.click(searchInput());

    await user.keyboard("{Enter}");

    expect(onOpenSearchResults).not.toHaveBeenCalled();
  });
});

// OVW-FR-11 / EDT-FR-33: the top chrome is the only thing between the shell's
// flush gate and the switcher that consults it. If this hand-off is dropped, a
// project switch discards the outgoing project's unsaved edits silently — so the
// pass-through is asserted here rather than assumed.
