import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { TopChrome } from "./Shell";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { TYPING_PAUSE_MS } from "../hooks/useSearch";
import { QUERY_MODES } from "../hooks/useSearchQueryMode";
import {
  createEventBus,
  emitEnded,
  emitResults,
  hit,
  resetHitOrdinals,
} from "../test/searchEvents";
import type { SearchHit, ThemePreference, WorktreeEntry } from "../types";

// The top chrome hosts the universal search bar and its overlay (SCH-FR-07).
// The chrome also mounts the project switcher, whose import chain reaches the
// Tauri core/window/dialog/event plugins at module load — stub them all so the
// overlay behaviour can be exercised under jsdom without the runtime.
//
// Search results arrive on the EVENT bus rather than as a command return value
// (SCC-FR-19), so `listen` is a real, drivable bus: a stub that never delivers
// would leave every overlay assertion below testing an empty overlay.
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

/** The id `start_search` handed back for the Nth dispatch of this test. */
const lastSearchId = () => searchIds[searchIds.length - 1];

/** Dispatch a query and stream `hits` into the overlay, then end the search. */
async function streamResults(
  user: ReturnType<typeof userEvent.setup>,
  query: string,
  hits: SearchHit[],
) {
  await user.type(searchInput(), query);
  await waitFor(() => expect(searchIds.length).toBeGreaterThan(0));
  const id = lastSearchId();
  await act(async () => {
    emitResults(bus, id, hits);
    emitEnded(bus, id);
  });
  if (hits.length > 0) {
    await screen.findByText(hits[0].name);
  }
}
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

// Drives `searchOpen` through real state so the overlay mounts/unmounts exactly
// as it does in the app, and provides an element *fully outside* the top chrome
// to click for the outside-dismiss scenario.
function Harness({
  onBeforeSwitchProject = async () => true,
  activeWorktree = null,
  onOpenSearchResults = () => {},
  onActivateSearchHit = () => {},
  onOverlayOpening = () => {},
}: {
  onBeforeSwitchProject?: () => Promise<boolean>;
  activeWorktree?: WorktreeEntry | null;
  onOpenSearchResults?: (query: string, mode: string) => void;
  onActivateSearchHit?: (hit: SearchHit) => void;
  onOverlayOpening?: () => void;
}) {
  const [searchOpen, setSearchOpen] = useState(false);
  // SNV-FR-56: the roster's open state is the shell's, so the harness holds it —
  // which is what lets these scenarios assert the real mutual exclusion rather
  // than a callback having been called.
  const [agentsRosterOpen, setAgentsRosterOpen] = useState(false);
  const [themePref, setThemePref] = useState<ThemePreference>("system");
  return (
    <div>
      <button type="button">outside-area</button>
      <TopChrome
        projectName="acme"
        projectPath="~/dev/acme"
        onSwitchProject={() => {}}
        onBeforeSwitchProject={onBeforeSwitchProject}
        onOpenAnother={() => {}}
        onRequestMergeCommit={async () => null}
        onOpenChanges={() => {}}
        onOpenRun={() => {}}
        // NAW-FR-01: in the app this closes the status bar's in-flight
        // operations overlay (STB-FR-12); these scenarios have no such overlay.
        onOverlayOpening={onOverlayOpening}
        // WTS-FR-02: no active worktree means the project is not inside a Git
        // repository, so no selector renders — which keeps these search-overlay
        // scenarios exactly as they were before the selector existed.
        activeWorktree={activeWorktree}
        onSwitchWorktree={async () => ({ ok: true })}
        themePref={themePref}
        onSelectTheme={setThemePref}
        searchOpen={searchOpen}
        setSearchOpen={setSearchOpen}
        onOpenSearchResults={onOpenSearchResults}
        onActivateSearchHit={onActivateSearchHit}
        // WTS-FR-35: the two routes out of a refresh whose remote leg resolved
        // no credential. Neither is reached in these scenarios — the refresh
        // control's own behaviour is exercised in `WorktreeSelector.test.tsx`.
        onRequestGithubToken={async () => false}
        onOpenGlobalSettings={() => {}}
        // SNV-FR-59 / AGT-FR-06: the agents control sits in the same trailing
        // cluster as the theme selector. Its roster's own behaviour is exercised
        // in `Agents.test.tsx`; what these scenarios need is only that it is
        // there, and that opening it says so through `onOverlayOpening`.
        onOpenAgentInSettings={() => {}}
        onOpenProjectSettings={() => {}}
        agentsRosterOpen={agentsRosterOpen}
        setAgentsRosterOpen={(open) => {
          // The shell closes every other overlay before opening one; the search
          // overlay is the only other one this harness mounts.
          if (open) setSearchOpen(false);
          setAgentsRosterOpen(open);
        }}
      />
    </div>
  );
}

beforeEach(() => {
  bus.reset();
  searchIds = [];
  resetHitOrdinals();
  resetAppPreferencesCache();
  let counter = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "start_search") {
      const id = `search-${counter++}`;
      searchIds.push(id);
      return id;
    }
    if (cmd === "load_app_preferences") {
      return { theme: "system", mainWindowFullscreen: false };
    }
    if (cmd === "list_recent_projects") return [];
    // SNV-FR-59 / AGT-FR-02: the agents control reads its count on mount.
    if (cmd === "list_project_agents") return [];
    if (cmd === "list_agent_turns") return [];
    return undefined;
  });
});

/** Escape a literal for use inside an accessible-name RegExp. */
const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const overlayMarker = () =>
  screen.queryByRole("button", { name: /View all results/i });
const searchInput = () => screen.getByPlaceholderText(/Search artifacts/i);

// NOTE: overlay dismissal is bound to `mousedown` (not `click`), so these tests
// must use `userEvent.click` / `fireEvent.mouseDown` to trigger it — a
// `fireEvent.click` would skip `mousedown` and silently not dismiss.
async function openOverlay(user: ReturnType<typeof userEvent.setup>) {
  await user.click(searchInput());
  expect(overlayMarker()).toBeInTheDocument();
}

describe("search overlay dismissal (SCH-FR-07)", () => {
  afterEach(cleanup);

  it("SCH-FR-07: Escape closes the overlay and focus returns to the input", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);

    await user.keyboard("{Escape}");

    expect(overlayMarker()).not.toBeInTheDocument();
    expect(searchInput()).toHaveFocus();
  });

  it("SCH-FR-07: a pointer-down on an in-chrome control outside the search region closes it", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);

    // The History button lives inside the top chrome but outside the search
    // ref — the spec's own "elsewhere in the top chrome" example.
    await user.click(screen.getByTitle("History"));

    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("SCH-FR-07: a pointer-down fully outside the chrome closes it", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);

    await user.click(screen.getByText("outside-area"));

    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("SCH-FR-07: the search input losing keyboard focus to an outside control closes it", async () => {
    render(<Harness />);
    fireEvent.focus(searchInput());
    expect(overlayMarker()).toBeInTheDocument();

    // Tab-out: focus moves to a control outside the search ref.
    fireEvent.blur(searchInput(), { relatedTarget: screen.getByTitle("History") });

    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("SCH-FR-07: the window losing focus (switching apps) closes the overlay", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);

    // Switching to another macOS app blurs the whole webview window; the
    // input's blur in this case has a null relatedTarget.
    fireEvent.blur(window);

    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("SCH-FR-07: the input blurring with a null relatedTarget does not itself dismiss (deferred to the window-blur handler)", () => {
    render(<Harness />);
    fireEvent.focus(searchInput());
    expect(overlayMarker()).toBeInTheDocument();

    // A null relatedTarget alone (e.g. clicking a non-focusable spot) must not
    // close — only an actual window blur does. This pins the two-path design.
    fireEvent.blur(searchInput(), { relatedTarget: null });

    expect(overlayMarker()).toBeInTheDocument();
  });

  it("SCH-FR-07: a window blur after the overlay is closed is a no-op (listener cleaned up)", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);
    await user.keyboard("{Escape}");
    expect(overlayMarker()).not.toBeInTheDocument();

    // The window-blur listener is detached once closed, so this neither
    // reopens the overlay nor updates an unmounted handler.
    fireEvent.blur(window);

    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("SCH-FR-07: focus moving to an in-overlay control does not dismiss the overlay", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", [hit({ path: "src/a.rs" })]);
    const header = screen.getByRole("button", { name: /Files/i });

    // Focus crosses from the input into the overlay (a group header).
    fireEvent.blur(searchInput(), { relatedTarget: header });

    expect(overlayMarker()).toBeInTheDocument();
  });

  it("clicking the input while open does not dismiss the overlay", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);

    await user.click(searchInput());

    expect(overlayMarker()).toBeInTheDocument();
  });

  it("SCH-FR-06, SCH-FR-07, SCH-FR-08: 'View all results' opens the results tab with the same query and mode, and closes the overlay", async () => {
    const user = userEvent.setup();
    const onOpenSearchResults = vi.fn();
    render(<Harness onOpenSearchResults={onOpenSearchResults} />);
    await streamResults(user, "needle", [hit({ path: "src/a.rs" })]);

    await user.click(overlayMarker()!);

    expect(onOpenSearchResults).toHaveBeenCalledWith("needle", "literal_insensitive");
    expect(overlayMarker()).not.toBeInTheDocument();
  });

  it("re-opens after being dismissed", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await openOverlay(user);
    await user.click(screen.getByText("outside-area"));
    expect(overlayMarker()).not.toBeInTheDocument();

    await user.click(searchInput());
    expect(overlayMarker()).toBeInTheDocument();
  });

  it("SCH-FR-09: activating a result routes it and dismisses the overlay", async () => {
    const user = userEvent.setup();
    const onActivateSearchHit = vi.fn();
    render(<Harness onActivateSearchHit={onActivateSearchHit} />);
    await streamResults(user, "needle", [hit({ path: "src/main.rs" })]);

    await user.click(screen.getByText("main.rs"));

    expect(onActivateSearchHit).toHaveBeenCalledWith(
      expect.objectContaining({ id: "src/main.rs", group: "file" }),
    );
    expect(overlayMarker()).not.toBeInTheDocument();
  });
});

describe("search overlay group collapse (SCH-FR-05 / SCH-FR-07)", () => {
  afterEach(cleanup);

  /** Two groups' worth of hits, so collapsing one can be shown not to touch the other. */
  const twoGroups = () => [
    hit({
      path: ".claude/skills/design-review.md",
      group: "artifact",
      subtype: "skill",
      editContext: "standalone",
    }),
    hit({ path: "src/main.rs", group: "file" }),
  ];

  it("SCH-FR-05, SCH-FR-07: collapsing a group keeps the overlay open and isolates the group", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", twoGroups());
    expect(screen.getByText("design-review.md")).toBeInTheDocument();
    expect(screen.getByText("main.rs")).toBeInTheDocument();

    await user.click(screen.getByText("Artifacts"));

    // The clicked group collapsed (its items hidden)...
    expect(screen.queryByText("design-review.md")).not.toBeInTheDocument();
    // ...other groups are unaffected...
    expect(screen.getByText("main.rs")).toBeInTheDocument();
    // ...and the overlay itself stayed open.
    expect(overlayMarker()).toBeInTheDocument();
  });

  it("re-expands a collapsed group on a second activation", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", twoGroups());
    await user.click(screen.getByText("Artifacts"));
    expect(screen.queryByText("design-review.md")).not.toBeInTheDocument();

    await user.click(screen.getByText("Artifacts"));

    expect(screen.getByText("design-review.md")).toBeInTheDocument();
    expect(overlayMarker()).toBeInTheDocument();
  });

  it("the collapse header is operable by keyboard (Enter)", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", twoGroups());
    const header = screen.getByRole("button", { name: /Artifacts/i });

    header.focus();
    await user.keyboard("{Enter}");

    expect(screen.queryByText("design-review.md")).not.toBeInTheDocument();
    expect(overlayMarker()).toBeInTheDocument();
  });

  it("SCH-FR-03: the Artifacts group splits by subtype and unclassified files land under Files", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", [
      hit({ path: "s/plan.scenario.md", group: "artifact", subtype: "scenario" }),
      hit({ path: "s/notes.scratchpad.md", group: "artifact", subtype: "scratchpad" }),
      hit({ path: "src/main.rs", group: "file" }),
    ]);

    // SCH-FR-03: Artifacts, split by subtype; a seventh Files group for the rest.
    expect(screen.getByText("Artifacts")).toBeInTheDocument();
    expect(screen.getByText("Scenario")).toBeInTheDocument();
    expect(screen.getByText("Scratchpad")).toBeInTheDocument();
    expect(screen.getByText("Files")).toBeInTheDocument();
    // SCH-FR-04: a group with no hits is not rendered at all.
    expect(screen.queryByText("Playbooks")).not.toBeInTheDocument();
    expect(screen.queryByText("Runs")).not.toBeInTheDocument();
    expect(screen.queryByText("Skill")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Dispatch, streaming and the query-mode toggles (SCH-FR-12..FR-21)
// ---------------------------------------------------------------------------

describe("universal search dispatch (SCH-FR-15 / SCH-FR-16 / SCH-FR-17)", () => {
  afterEach(cleanup);

  it("SCH-FR-15: a burst of typing dispatches exactly one search, carrying the whole query", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(searchInput());
    await user.type(searchInput(), "abcdefgh");

    await waitFor(() => expect(searchIds).toHaveLength(1));
    const call = invokeMock.mock.calls.find((c) => c[0] === "start_search");
    expect(call?.[1]).toEqual({
      query: "abcdefgh",
      mode: "literal_insensitive",
      // SCH-FR-18: the overlay's search is capped so typing stays cheap.
      scope: "capped",
    });
    // `waitFor` returns at the FIRST success, so on its own it proves "at least
    // one". Settling past the typing pause is what rules out a second dispatch
    // arriving late — "exactly one" is the claim SCH-FR-15 actually makes.
    await new Promise((resolve) => setTimeout(resolve, TYPING_PAUSE_MS * 2));
    expect(searchIds).toHaveLength(1);
  });

  it("SCH-FR-10 (negative half): the overlay offers no filter controls", async () => {
    // SCH-FR-10: the filters belong to the full results page. Offering them in
    // the overlay would make the two levels answer differently.
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", [hit({ path: "src/a.rs" })]);

    const overlay = document.querySelector(".search-overlay") as HTMLElement;
    expect(within(overlay).queryByRole("combobox")).toBeNull();
    expect(
      within(overlay).queryByLabelText(/Filter by artifact type/i),
    ).toBeNull();
  });

  it("SCH-FR-15, SCH-FR-20: emptying the query dispatches nothing and cancels the search in flight", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "needle", [hit({ path: "src/a.rs" })]);
    const dispatched = searchIds.length;

    await user.clear(searchInput());

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "cancel_search" && c[1]?.searchId === lastSearchId(),
        ),
      ).toBe(true),
    );
    expect(searchIds).toHaveLength(dispatched);
    expect(screen.queryByText("a.rs")).not.toBeInTheDocument();
  });

  it("SCH-FR-16: batches arriving out of order render in ascending ordinal, and nothing already visible moves", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.type(searchInput(), "needle");
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const id = lastSearchId();

    // Ordinals 2 and 0 arrive first, then 1 fills the gap between them.
    emitResults(bus, id, [
      hit({ path: "src/c.rs", ordinal: 2 }),
      hit({ path: "src/a.rs", ordinal: 0 }),
    ]);
    await screen.findByText("c.rs");
    emitResults(bus, id, [hit({ path: "src/b.rs", ordinal: 1 })]);
    await screen.findByText("b.rs");

    const rendered = screen
      .getAllByText(/^[abc]\.rs$/)
      .map((el) => el.textContent);
    expect(rendered).toEqual(["a.rs", "b.rs", "c.rs"]);
  });

  it("SCH-FR-17: a content match shows the line number and snippet; a name match shows neither", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await streamResults(user, "find me", [
      hit({
        path: "src/main.rs",
        matchKind: "content",
        line: 87,
        snippet: "// find me: the walker starts here",
      }),
      hit({
        path: "src/find-me-notes.rs",
        matchKind: "name",
        line: undefined,
        snippet: undefined,
      }),
    ]);

    expect(screen.getByText("87")).toBeInTheDocument();
    expect(
      screen.getByText("// find me: the walker starts here"),
    ).toBeInTheDocument();
    // SCH-FR-17: a path-only match renders no snippet line, and there is no
    // match count anywhere in the overlay.
    const nameRow = screen.getByText("find-me-notes.rs").closest(".search-result-entry");
    expect(nameRow?.querySelector(".search-result__snippet")).toBeNull();
  });

  it("SCH-FR-04 / SCH-FR-19: the empty state appears only once the search has ended", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.type(searchInput(), "needle");
    await waitFor(() => expect(searchIds).toHaveLength(1));

    // Still running with nothing found yet: an in-progress state, NOT a claim
    // that there are no results.
    expect(screen.getByText(/Searching/i)).toBeInTheDocument();
    expect(screen.queryByText(/No results/i)).not.toBeInTheDocument();

    emitEnded(bus, lastSearchId());

    expect(await screen.findByText(/No results/i)).toBeInTheDocument();
  });

  it("SCH-FR-21: an uncompilable regular expression renders in place, leaving query and mode intact", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "start_search") throw new Error("invalid query");
      if (cmd === "load_app_preferences")
        return { theme: "system", searchQueryMode: "regex" };
      return undefined;
    });
    render(<Harness />);
    await waitFor(() =>
      expect(
        screen.getByRole("radio", { name: /Regular expression/i }),
      ).toHaveAttribute("aria-checked", "true"),
    );

    await user.type(searchInput(), "foo(");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /Invalid regular expression/i,
    );
    expect(searchInput()).toHaveValue("foo(");
    expect(
      screen.getByRole("radio", { name: /Regular expression/i }),
    ).toHaveAttribute("aria-checked", "true");
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
    expect(toggles).toHaveLength(3);
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

    // And the three descriptions are distinct — a shared blurb would leave the
    // two literal modes indistinguishable, which is the actual confusion.
    expect(new Set(QUERY_MODES.map((m) => m.description)).size).toBe(3);
  });

  it("SCH-FR-13: the mode is read from app preferences on mount, defaulting to case-insensitive literal", async () => {
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

    // And a user who has never chosen one starts in case-insensitive literal.
    cleanup();
    resetAppPreferencesCache();
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_app_preferences" ? { theme: "system" } : undefined,
    );
    render(<Harness />);
    await waitFor(() =>
      expect(
        screen.getByRole("radio", { name: /Case-insensitive/i }),
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

    expect(onOpenSearchResults).toHaveBeenCalledWith("needle", "literal_insensitive");
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
describe("TopChrome project-switch flush gate", () => {
  afterEach(cleanup);
  it("hands the flush gate to the project switcher", async () => {
    const onBeforeSwitchProject = vi.fn(async () => false);
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "list_recent_projects"
        ? [
            { name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-06-20T10:00:00Z" },
            { name: "beta", path: "~/dev/beta", lastOpenedAt: "2026-06-19T10:00:00Z" },
          ]
        : undefined,
    );
    render(<Harness onBeforeSwitchProject={onBeforeSwitchProject} />);

    await userEvent.click(screen.getByTestId("project-switcher"));
    const menu = await screen.findByTestId("project-switcher-menu");
    await userEvent.click(await within(menu).findByText("beta"));

    await waitFor(() => expect(onBeforeSwitchProject).toHaveBeenCalled());
    // It returned false, so the switch never reached the backend.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "open_project_at_path"),
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Worktree selector placement (SNV-FR-32)
// ---------------------------------------------------------------------------

const ON_MAIN: WorktreeEntry = {
  path: "~/dev/acme",
  name: "acme",
  branch: "main",
  headShortHash: "4f2a10c",
  isDetached: false,
  isActive: true,
  isPrimary: true,
  isMissing: false,
};

describe("worktree selector placement in the top chrome (SNV-FR-32)", () => {
  afterEach(cleanup);

  // SNV-FR-32, WTS-FR-02, WTS-FR-29, first half: between the project switcher and the search bar.
  it("sits immediately after the project switcher and before the search bar", () => {
    render(<Harness activeWorktree={ON_MAIN} />);

    const switcher = screen.getByTestId("project-switcher");
    const selector = screen.getByTestId("worktree-selector");
    const search = searchInput();

    // `compareDocumentPosition` reads document order, which is what "sits
    // between" means for a row of chrome controls.
    expect(
      switcher.compareDocumentPosition(selector) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      selector.compareDocumentPosition(search) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // The two read as a pair: same leading cluster.
    expect(selector.closest(".top-chrome__left")).toBe(
      switcher.closest(".top-chrome__left"),
    );
  });

  // SNV-FR-32, WTS-FR-02, WTS-FR-29, second half: outside a Git repository the switcher is followed
  // directly by the search bar — neither the selector nor the refresh control
  // appears anywhere in the chrome (WTS-FR-02 / WTS-FR-29).
  it("neither the selector nor the refresh control appears outside a Git repository", () => {
    render(<Harness activeWorktree={null} />);

    expect(screen.getByTestId("project-switcher")).toBeInTheDocument();
    expect(screen.queryByTestId("worktree-selector")).toBeNull();
    expect(screen.queryByTestId("worktree-refresh")).toBeNull();
    expect(searchInput()).toBeInTheDocument();
  });

  // SNV-FR-32, WTS-FR-02, WTS-FR-29: the leading cluster is four items in a fixed order — switcher,
  // selector, refresh control, search bar.
  it("orders the leading cluster switcher, selector, refresh, search bar", () => {
    render(<Harness activeWorktree={ON_MAIN} />);

    const cluster = [
      screen.getByTestId("project-switcher"),
      screen.getByTestId("worktree-selector"),
      screen.getByTestId("worktree-refresh"),
      searchInput(),
    ];
    // `compareDocumentPosition` reads document order, which is what a fixed
    // left-to-right order means for a row of chrome controls.
    for (let i = 0; i < cluster.length - 1; i += 1) {
      expect(
        cluster[i].compareDocumentPosition(cluster[i + 1]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
    // The refresh control reads as a sibling of the selector, not part of it:
    // it is its own control, outside the selector's activation area, so pressing
    // it cannot open the dropdown (WTS-FR-30).
    expect(
      screen.getByTestId("worktree-selector").contains(
        screen.getByTestId("worktree-refresh"),
      ),
    ).toBe(false);
  });
});

describe("Push control placement in the top chrome (SNV-FR-32, GIT-FR-XXLE)", () => {
  afterEach(cleanup);

  // SNV-FR-32, GIT-FR-XXLE, WSS-FR-JVUF: Push follows the refresh control and
  // precedes the work stream selector.
  it("SNV-FR-32, GIT-FR-XXLE, WSS-FR-JVUF orders the cluster refresh, Push, streams, search bar", () => {
    render(<Harness activeWorktree={ON_MAIN} />);

    const cluster = [
      screen.getByTestId("project-switcher"),
      screen.getByTestId("worktree-selector"),
      screen.getByTestId("worktree-refresh"),
      screen.getByTestId("top-push"),
      screen.getByTestId("stream-selector"),
      searchInput(),
    ];
    for (let i = 0; i < cluster.length - 1; i += 1) {
      expect(
        cluster[i].compareDocumentPosition(cluster[i + 1]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
    expect(screen.getByTestId("top-push")).toHaveAccessibleName("Push");
  });

  // SNV-FR-32, GIT-FR-XXLE: the control shares the selector's visibility rule.
  it("SNV-FR-32, GIT-FR-XXLE is absent outside a Git repository", () => {
    render(<Harness activeWorktree={null} />);
    expect(screen.queryByTestId("top-push")).toBeNull();
  });

  // WTS-FR-GVBD: Refresh stays fetch-only, and Push never refreshes.
  it("WTS-FR-GVBD keeps Refresh fetch-only and Push free of refresh and switch commands", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_upstream_sync_state")
        return { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 };
      if (cmd === "refresh_worktrees_and_branches")
        return { remoteState: "refreshed" };
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "list_recent_projects") return [];
      if (cmd === "list_project_agents") return [];
      if (cmd === "list_agent_turns") return [];
      return undefined;
    });
    render(<Harness activeWorktree={ON_MAIN} />);
    const calls = (name: string) =>
      invokeMock.mock.calls.filter((c) => c[0] === name).length;

    await user.click(screen.getByTestId("worktree-refresh"));
    await waitFor(() => expect(calls("refresh_worktrees_and_branches")).toBe(1));
    expect(calls("push_current_branch")).toBe(0);
    expect(calls("pull_current_branch")).toBe(0);

    await waitFor(() =>
      expect(screen.getByTestId("top-push")).toHaveAttribute(
        "aria-disabled",
        "false",
      ),
    );
    await user.click(screen.getByTestId("top-push"));
    await waitFor(() => expect(calls("push_current_branch")).toBe(1));
    expect(calls("refresh_worktrees_and_branches")).toBe(1);
    expect(calls("activate_worktree")).toBe(0);
    expect(calls("check_out_branch_in_active_worktree")).toBe(0);
  });
});

describe("the trailing cluster (SNV-FR-59)", () => {
  afterEach(cleanup);

  it("SNV-FR-59, SNV-FR-56, AGT-FR-02, AGT-FR-03 puts the agents control after the search bar and before the theme selector", async () => {
    // SNV-FR-59: the trailing cluster holds, in fixed order, the agents control
    // and then the theme selector — the two things that belong to the author
    // rather than to the repository.
    render(<Harness activeWorktree={ON_MAIN} />);

    const control = await screen.findByTestId("chrome-agents-control");
    const theme = screen.getByTestId("chrome-theme-select");
    const search = searchInput();

    expect(
      search.compareDocumentPosition(control) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      control.compareDocumentPosition(theme) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // The two read as a pair: same trailing cluster.
    expect(control.closest(".top-chrome__right")).toBe(
      theme.closest(".top-chrome__right"),
    );
  });

  it("is present whether or not the project has enrolled anyone", async () => {
    // SNV-FR-59 / AGT-FR-02: a project enrolling none carries the control with
    // a count of none rather than no control at all.
    render(<Harness activeWorktree={ON_MAIN} />);
    const control = await screen.findByTestId("chrome-agents-control");
    await waitFor(() => expect(control).toHaveTextContent("0"));
  });

  it("SNV-FR-59, SNV-FR-56, AGT-FR-02, AGT-FR-03 leaves only one overlay mounted, in either direction", async () => {
    // SNV-FR-59, AGT-FR-02, AGT-FR-03's second half / SNV-FR-56. The roster's open state is the
    // shell's, so this asserts what is actually *mounted* rather than that a
    // callback fired — which is the only form of the claim that could catch two
    // overlays coexisting.
    const user = userEvent.setup();
    const onOverlayOpening = vi.fn();
    render(
      <Harness activeWorktree={ON_MAIN} onOverlayOpening={onOverlayOpening} />,
    );

    // The search overlay first, then the roster over it.
    await user.click(searchInput());
    expect(overlayMarker()).toBeInTheDocument();
    await user.click(await screen.findByTestId("chrome-agents-control"));
    await screen.findByTestId("agents-roster");
    expect(overlayMarker()).not.toBeInTheDocument();
    // And the roster took part in the shell's contract on the way in, which is
    // what closes the overlays this harness does not mount.
    expect(onOverlayOpening).toHaveBeenCalled();

    // The other direction: the shell closes the roster when the next one opens.
    await user.click(searchInput());
    await waitFor(() =>
      expect(screen.queryByTestId("agents-roster")).toBeNull(),
    );
    expect(overlayMarker()).toBeInTheDocument();
  });
});
