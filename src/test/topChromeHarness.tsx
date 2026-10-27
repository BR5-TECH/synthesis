/**
 * The top chrome under test (SCH-FR-07): a harness that drives `searchOpen`
 * through real state, and the helpers the TopChrome test files share. Each test
 * file keeps its own `vi.mock` calls, its event bus, and its `invoke` mock.
 */
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect } from "vitest";

import { TopChrome } from "../components/Shell";
import type { SearchHit, ThemePreference, WorktreeEntry } from "../types";
import { createEventBus, emitEnded, emitResults } from "./searchEvents";

type Bus = ReturnType<typeof createEventBus>;

// Drives `searchOpen` through real state so the overlay mounts/unmounts exactly
// as it does in the app, and provides an element *fully outside* the top chrome
// to click for the outside-dismiss scenario.
export function Harness({
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
  // SNV-FR-QXTH / SNV-FR-56: so is the theme selector's dropdown.
  const [themeMenuOpen, setThemeMenuOpen] = useState(false);
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
          if (open) {
            setSearchOpen(false);
            setThemeMenuOpen(false);
          }
          setAgentsRosterOpen(open);
        }}
        themeMenuOpen={themeMenuOpen}
        setThemeMenuOpen={(open) => {
          if (open) {
            setSearchOpen(false);
            setAgentsRosterOpen(false);
          }
          setThemeMenuOpen(open);
        }}
      />
    </div>
  );
}

/**
 * The `invoke` answers every TopChrome test starts from. `start_search` hands
 * out a new id and records it in `searchIds`.
 */
export function topChromeInvoke(searchIds: string[]) {
  let counter = 0;
  return async (cmd: string) => {
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
  };
}

/** Escape a literal for use inside an accessible-name RegExp. */
export const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export const overlayMarker = () =>
  screen.queryByRole("button", { name: /View all results/i });
export const searchInput = () => screen.getByPlaceholderText(/Search artifacts/i);

// NOTE: overlay dismissal is bound to `mousedown` (not `click`), so these tests
// must use `userEvent.click` / `fireEvent.mouseDown` to trigger it — a
// `fireEvent.click` would skip `mousedown` and silently not dismiss.
export async function openOverlay(user: ReturnType<typeof userEvent.setup>) {
  await user.click(searchInput());
  expect(overlayMarker()).toBeInTheDocument();
}

/** Dispatch a query and stream `hits` into the overlay, then end the search. */
export async function streamResults(
  bus: Bus,
  searchIds: string[],
  user: ReturnType<typeof userEvent.setup>,
  query: string,
  hits: SearchHit[],
) {
  await user.type(searchInput(), query);
  await waitFor(() => expect(searchIds.length).toBeGreaterThan(0));
  const id = searchIds[searchIds.length - 1];
  await act(async () => {
    emitResults(bus, id, hits);
    emitEnded(bus, id);
  });
  if (hits.length > 0) {
    await screen.findByText(hits[0].name);
  }
}
