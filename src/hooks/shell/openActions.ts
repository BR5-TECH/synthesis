/**
 * The routes that put content in front of the author: an artifact, a diff, a
 * Search results tab, and a search hit followed to whichever of them it names.
 *
 * Held together because they share the tab-keying rule that TAB-FR-04,
 * TAB-FR-05 and TAB-FR-06 state between them — each keys its tab on what makes
 * the view distinct, so a second request for the same view focuses the tab
 * already showing it rather than opening a duplicate.
 *
 * Plain closures rather than a hook: the strip and the session stores stay
 * owned by `useShellSession`.
 */
import type { Dispatch, SetStateAction } from "react";
import { diffTabId } from "../../comparison";
import { routeForHit, subtypeOf } from "../../searchGroups";
import type { EditSessionStore } from "../../state/editSessions";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { SearchResultsTarget } from "../../components/SearchResults";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import { logDebug } from "../../logging";
import { SPEC_MAP_TAB } from "./tabRecords";
import type {
  BottomSurface,
  DiffTarget,
  OpenableArtifact,
  PanelSurface,
  SearchHit,
  SearchMode,
  Tab,
} from "../../types";

export interface OpenActionDeps {
  tabs: Tab[];
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  activateTab: (id: string) => void;
  sessions: EditSessionStore;
  flows: FlowSessionStore;
  setPanelSurface: (surface: PanelSurface) => void;
  setRevealArtifactId: (id: string | null) => void;
  showBottom: (surface: BottomSurface) => void;
  /** SMP-FR-QNUH: the map session a Project row selection is routed into. */
  specMap: SpecMapSessionStore;
  activeTab: string;
}

export interface OpenActions {
  openArtifact: (item: OpenableArtifact) => void;
  openDiff: (target: DiffTarget) => void;
  openSearchResults: (query: string, mode: SearchMode) => void;
  activateSearchHit: (hit: SearchHit) => void;
  openSpecMap: () => void;
  activateLibraryFile: (item: OpenableArtifact) => void;
}

export function createOpenActions(deps: OpenActionDeps): OpenActions {
  const {
    tabs,
    setTabs,
    activateTab,
    sessions,
    flows,
    setPanelSurface,
    setRevealArtifactId,
    showBottom,
    specMap,
    activeTab,
  } = deps;
  const openRuns = () => showBottom("runs");

  const openArtifact = (item: OpenableArtifact) => {
    // The filesystem Library tree supplies `id` (its stable, path-derived key
    // per ASC-FR-13) and `artifactType`; the Dashboard's canned rows supply
    // `type`. Key the tab by id when present so two files with the same
    // basename in different folders open distinct tabs (TAB-FR-04).
    const resolvedType = item.artifactType ?? item.type;
    const id = `art:${item.id ?? item.name}`;
    if (!tabs.find((t) => t.id === id)) {
      setTabs((ts) => [
        ...ts,
        {
          id,
          label: item.name,
          chip: item.chip,
          type: resolvedType,
          // LIB-FR-03: a Flow opens a Flow tab; every other recognised
          // artifact type opens an Editor tab.
          kind: resolvedType === "flow" ? "flow" : "editor",
          // Real filesystem nodes carry a backend id -> the Editor loads/saves
          // live (EDT-FR-01/02). Canned rows without an id keep the mock.
          artifactId: item.id,
        },
      ]);
    }
    if (item.id) {
      // EDT-FR-28: an Editor tab is a view onto the artifact's edit session.
      // FLO-FR-28: a Flow tab, by contrast, *owns* its Flow's session — and its
      // first open is what loads the document (FLO-FR-03).
      if (resolvedType === "flow") flows.openTab(item.id);
      else sessions.openTab(item.id);
    }
    activateTab(id);
  };

  /**
   * CHG-FR-18 / DFV-FR-04: open a Diff tab for one file under one comparison.
   * The tab is keyed on that pair, so a request for one already open jumps
   * focus to it rather than creating a duplicate — and the same file under a
   * different comparison gets its own tab.
   *
   * DFV-FR-02: the label is `Diff: <file>`, so a diff is distinguishable from
   * an Editor tab on the same artifact at a glance in the strip. Two Diff tabs
   * on one file under different comparisons therefore read identically there,
   * which is what the tooltip is for (DFV-FR-03).
   *
   * DFV-FR-42 / DFV-FR-05: the tab's **target** is the artifact's own editing
   * session, so the tab carries that artifact's id like an Editor tab does — and
   * with it the dirty indicator (EDT-FR-04), Save's enablement (SNV-FR-28), and
   * the write-before-close and refuse-while-blocked rules (TAB-FR-10,
   * TAB-FR-11). It is not a second session: however many tabs are open on the
   * file there is one buffer, one dirty state, one history and one write, which
   * is the whole of what the single-tab rule protects and why TAB-FR-06 exempts
   * this tab from it. Its `diff:` id keeps it distinct in the strip from the
   * `art:` id of an Editor tab on the same artifact.
   */
  const openDiff = (target: DiffTarget) => {
    const id = diffTabId(target);
    if (!tabs.find((t) => t.id === id)) {
      setTabs((ts) => [
        ...ts,
        {
          id,
          label: `Diff: ${target.name}`,
          tooltip: `Diff: ${target.path} — ${target.comparisonLabel}`,
          kind: "diff",
          diff: target,
          artifactId: target.path,
        },
      ]);
    }
    // DFV-FR-25: opening the tab establishes the artifact's editing session. One
    // the artifact already carries costs nothing at all.
    sessions.openTab(target.path);
    activateTab(id);
  };

  /**
   * SCH-FR-08: open a Search results tab over `query` in `mode`. The tab
   * dispatches its own `scope = full` search (SCH-FR-18) — it inherits nothing
   * from the overlay's capped result set.
   *
   * The tab is keyed on the query/mode pair, so re-submitting the same query
   * focuses the tab that is already showing it rather than opening a second one
   * (TAB-FR-05), and a different query or a different mode gets its own tab.
   */
  const openSearchResults = (query: string, mode: SearchMode) => {
    const target: SearchResultsTarget = { query, mode };
    const id = `search:${mode}:${query}`;
    if (!tabs.find((t) => t.id === id)) {
      setTabs((ts) => [
        ...ts,
        { id, label: `Search: ${query}`, kind: "search", search: target },
      ]);
    }
    activateTab(id);
  };

  /**
   * SCH-FR-09: follow a search result to its surface, by the same routing rule
   * the Dashboard uses for widget click-through (DSH-FR-06).
   *
   * A **file** hit — one carrying no artifact type — opens in the Editor as a
   * plain text file (ESH-FR-ATDS). `openArtifact` already does exactly that for a
   * node with an id and no type, and keying the tab by that id is what makes the
   * single-tab rule bind it like any other content (TAB-FR-04).
   */
  const activateSearchHit = (hit: SearchHit) => {
    const route = routeForHit(hit);
    switch (route.kind) {
      case "workstream":
        // The Library filtered to that workstream: reveal it in the panel.
        setPanelSurface("library");
        setRevealArtifactId(hit.id);
        return;
      case "runs":
        openRuns();
        return;
      case "history":
        // SCH-FR-09 asks for a History viewer scoped to the entity, which needs
        // a History detail tab this build does not have yet — so this reveals
        // the History surface without a scope. Unreachable in v1 either way:
        // the engine matches only files under the content root, and never
        // populates the `history` group at all (`../../specifications/core/
        // SCC-search.md` SCC-FR-09). Scoping lands with the History viewer.
        showBottom("history");
        return;
      default:
        openArtifact({
          id: hit.id,
          name: hit.name,
          artifactType: subtypeOf(hit),
        });
    }
  };

  /**
   * SMP-FR-WBNL / TAB-FR-KXMW: open the Map tab, or jump focus to the one open.
   * The presence test runs inside the updater, so two requests landing before a
   * render cannot add a second Map tab.
   */
  const openSpecMap = () => {
    setTabs((ts) => (ts.some((t) => t.id === SPEC_MAP_TAB.id) ? ts : [...ts, SPEC_MAP_TAB]));
    activateTab(SPEC_MAP_TAB.id);
  };

  /**
   * LIB-FR-SJDC / SMI-FR-PRSL: while the Map tab is active, a Spec file clicked
   * in the Project panel selects its node on the map and opens no tab. Every
   * other click follows LIB-FR-03.
   */
  const activateLibraryFile = (item: OpenableArtifact) => {
    if (activeTab === SPEC_MAP_TAB.id && item.artifactType === "spec" && item.id) {
      logDebug(["frontend"], "spec row routed to the specification map", { path: item.id });
      specMap.selectByPath(item.id);
      return;
    }
    openArtifact(item);
  };

  return {
    openArtifact,
    openDiff,
    openSearchResults,
    activateSearchHit,
    openSpecMap,
    activateLibraryFile,
  };
}
