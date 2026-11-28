/**
 * The Changes panel over a project with git submodules: the repository groups
 * (CHG-FR-BYWJ, CHG-FR-IGSD, CHG-FR-UMCU, CHG-FR-EQYC), the pointer row and
 * its implied check (CHG-FR-GBTE, CHG-FR-AYTR, CHG-FR-DGHI), the reveal of a
 * file of a submodule (CHG-FR-54), Push over more than one repository
 * (CHG-FR-KQWD), and the repositories a push skipped (CHG-FR-NUZB).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GIT_OPERATION_FINISHED } from "../events";
import { resetLogBufferForTest } from "../logging";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import { readStylesheet } from "../test/readStylesheet";
import type { RepositoryInfo, UpstreamSyncState } from "../types";
import {
  LIB,
  change,
  changeSet,
  checkbox,
  cssRule,
  makeBackend,
  onOpenDiff,
  onRequestCommitMessage,
  onRequestRollback,
  primary,
  renderPanel,
  resetHandlers,
  row,
  rowNames,
  showAllFiles,
} from "../test/changesFixtures";

const invokeMock = vi.fn();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    // The panel and the window's push state both follow these events.
    const previous = listeners[event];
    listeners[event] = previous
      ? (ev) => {
          previous(ev);
          cb(ev);
        }
      : cb;
    return () => {};
  }),
}));

const backend = makeBackend(invokeMock);
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const MAIN: RepositoryInfo = {
  path: "",
  name: "acme",
  branch: "feature/x",
  headId: "aaaaaaa1111111",
};
const KIT: RepositoryInfo = {
  path: "vendor/kit",
  name: "kit",
  parent: "",
  headId: "9c1d2e3f4a5b6c7d",
};
/** A member with no change, which renders no repository group. */
const QUIET: RepositoryInfo = {
  path: "vendor/quiet",
  name: "quiet",
  parent: "",
  branch: "main",
  headId: "bbbbbbb2222222",
};
const REPOSITORIES = [MAIN, KIT, QUIET];

const KIT_FILE = change("vendor/kit/docs/guide.md", { repository: "vendor/kit" });
const POINTER = change("vendor/kit", {
  repository: "",
  entryKind: "submodule",
  previousCommit: "1a2b3c4d5e6f7a8b",
  currentCommit: "9c1d2e3f4a5b6c7d",
  removedLines: 1,
});

const multiSet = (entries = [LIB, POINTER, KIT_FILE]) =>
  changeSet(entries, undefined, REPOSITORIES);

/** The repository group rows, in render order. */
const repositoryRows = () =>
  Array.from(document.querySelectorAll<HTMLElement>(".tree-row--repository"));

/**
 * The panel's `"git operation finished"` handler. Asserted present, so a test
 * that delivers a push end never passes because nothing received it.
 */
const finishedListener = () => {
  const handler = listeners[GIT_OPERATION_FINISHED];
  expect(handler, "no git-operation-finished listener is registered").toBeDefined();
  return handler!;
};

const rollbackButton = () =>
  screen.getByRole("button", { name: "Discard selected files" }) as HTMLButtonElement;

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  resetAppPreferencesCache();
  resetHandlers();
  invokeMock.mockReset();
  listeners = {};
  backend({ strict: true, uncommitted: multiSet() });
});

afterEach(cleanup);

describe("repository groups (CHG-FR-BYWJ, CHG-FR-IGSD, CHG-FR-UMCU, CHG-FR-EQYC)", () => {
  it("CHG-FR-IGSD: one repository renders no repository group, and the two groups are the top level", async () => {
    backend({ strict: true, uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("Library.tsx"));
    expect(repositoryRows()).toHaveLength(0);
    expect(rowNames()[0]).toBe("Revisioned");
  });

  it("CHG-FR-BYWJ, CHG-FR-UMCU: one group for each repository with a visible file, main first, labelled with name or path and branch", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    const [main, kit] = repositoryRows();
    // CHG-FR-UMCU: the name of the main repository, then its branch.
    expect(main.querySelector(".tree-row__name")).toHaveTextContent("acme");
    expect(main.querySelector(".tree-row__branch")).toHaveTextContent("feature/x");
    // CHG-FR-UMCU: the path of a submodule, then `detached` and the short id.
    expect(kit.querySelector(".tree-row__name")).toHaveTextContent("vendor/kit");
    expect(kit.querySelector(".tree-row__branch")).toHaveTextContent("detached 9c1d2e3");
    // CHG-FR-UMCU: the header states the full label on hover, as the panel
    // width can truncate it.
    expect(main).toHaveAttribute("title", "acme · feature/x");
    expect(kit).toHaveAttribute("title", "vendor/kit · detached 9c1d2e3");
    // CHG-FR-50: each group carries the count of what it holds.
    expect(main.querySelector(".tree-row__count")).toHaveTextContent("2 files");
    expect(kit.querySelector(".tree-row__count")).toHaveTextContent("1 file");
    // CHG-FR-BYWJ: a repository with no visible changed file has no group.
    expect(rowNames()).not.toContain("vendor/quiet");
    // CHG-FR-BYWJ / CHG-FR-09: each repository holds its own groups.
    expect(rowNames().filter((n) => n === "Revisioned")).toHaveLength(2);
  });

  it("CHG-FR-BYWJ: a repository whose files the filters hide renders no group", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    await userEvent.type(screen.getByLabelText("Filter files"), "guide");
    await waitFor(() => expect(repositoryRows()).toHaveLength(1));
    expect(repositoryRows()[0].querySelector(".tree-row__name")).toHaveTextContent(
      "vendor/kit",
    );
  });

  it("CHG-FR-EQYC: the folder tree of a submodule is built from paths relative to its root", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    const names = rowNames();
    const kitAt = names.indexOf("vendor/kit");
    // Beneath the submodule group: its Revisioned group, then `docs`, then the
    // file — no `vendor` or `kit` folder of the project path.
    expect(names.slice(kitAt + 1)).toEqual(["Revisioned", "docs", "guide.md"]);
    // The pointer row sits in the main repository at the submodule path.
    expect(names.slice(0, kitAt)).toEqual([
      "acme",
      "Revisioned",
      "src",
      "components",
      "Library.tsx",
      "vendor",
      "kit",
    ]);
  });

  it("CHG-FR-UMCU, CHG-FR-28: the checkbox of a repository group cascades over its own visible files only", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    await userEvent.click(screen.getByLabelText("Include repository vendor/kit"));
    expect(checkbox("guide.md")).toBeChecked();
    expect(checkbox("Library.tsx")).not.toBeChecked();
    // CHG-FR-AYTR: the implied pointer row is inert, so the main group counts
    // only its file and stays unchecked.
    const mainBox = screen.getByLabelText("Include repository acme") as HTMLInputElement;
    expect(mainBox.indeterminate).toBe(false);
    expect(mainBox).not.toBeChecked();
  });

  it("CHG-FR-UMCU: Branch mode renders the repository groups with no checkbox", async () => {
    backend({
      strict: true,
      panelState: { mode: "branch", targetBranch: "main" },
      branchChanges: changeSet([LIB, KIT_FILE], "main", REPOSITORIES),
    });
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    // CHG-FR-26: no checkbox anywhere in the tree — groups, sections, or rows.
    expect(document.querySelectorAll(".tree-row input[type=checkbox]")).toHaveLength(0);
  });

  it("CHG-FR-BYWJ, CHG-FR-IGSD: a listed set of more than one repository renders a repository group also when only the main one has changes", async () => {
    backend({ strict: true, uncommitted: multiSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(1));
    expect(repositoryRows()[0].querySelector(".tree-row__name")).toHaveTextContent("acme");
    expect(rowNames()[0]).toBe("acme");
  });
});

describe("the pointer row (CHG-FR-GBTE, CHG-FR-AYTR, CHG-FR-DGHI)", () => {
  it("CHG-FR-GBTE: renders with the branch icon and the two short ids in place of the diffstat", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("kit"));
    const pointer = row("kit");
    expect(pointer).toHaveAttribute("data-entry-kind", "submodule");
    // The branch glyph has three circles; the file glyph has none.
    expect(pointer.querySelectorAll(".tree-row__icon circle")).toHaveLength(3);
    expect(pointer.querySelector(".change-row__pointer-previous")).toHaveTextContent(
      /^1a2b3c4$/,
    );
    expect(pointer.querySelector(".change-row__pointer")).toHaveTextContent(/^→ 9c1d2e3$/);
    expect(pointer.querySelector(".change-row__stat")).toHaveAttribute(
      "title",
      "Submodule commit: 1a2b3c4 → 9c1d2e3",
    );
    expect(pointer.querySelector(".change-row__added")).toBeNull();
  });

  it("CHG-FR-GBTE: an added submodule states `+` and its current short id", async () => {
    backend({
      strict: true,
      uncommitted: multiSet([
        { ...POINTER, changeStatus: "added", previousCommit: undefined, removedLines: 0 },
      ]),
    });
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("kit"));
    expect(row("kit").querySelector(".change-row__pointer")).toHaveTextContent(/^\+ 9c1d2e3$/);
    expect(row("kit").querySelector(".change-row__pointer-previous")).toBeNull();
  });

  it("CHG-FR-GBTE: activating the pointer row selects it and opens no Diff tab", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("kit"));
    await userEvent.click(row("kit"));
    expect(row("kit")).toHaveAttribute("data-selected", "true");
    expect(onOpenDiff).not.toHaveBeenCalled();
    // A file row still opens its Diff tab.
    await userEvent.click(row("guide.md"));
    expect(onOpenDiff).toHaveBeenCalledOnce();
  });

  it("CHG-FR-AYTR: a checked file of the submodule renders the pointer checked and inert, and the commit carries it", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    expect(checkbox("kit")).not.toBeChecked();
    expect(checkbox("kit")).toBeEnabled();

    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("kit")).toBeChecked();
    expect(checkbox("kit")).toBeDisabled();
    // An inert checkbox takes no input.
    await userEvent.click(checkbox("kit"));
    expect(checkbox("kit")).toBeChecked();

    // Unchecking the file gives the pointer row its own check back.
    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("kit")).not.toBeChecked();
    expect(checkbox("kit")).toBeEnabled();
    await userEvent.click(checkbox("guide.md"));
    // CHG-FR-55: the count row counts the commit set, the implied pointer in it.
    expect(screen.getByText("2 files selected")).toBeInTheDocument();

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalledOnce());
    const [files] = onRequestCommitMessage.mock.calls[0];
    expect(files.map((f) => f.path).sort()).toEqual(["vendor/kit", "vendor/kit/docs/guide.md"]);
    // CMW-FR-WZRQ: each file names the repositories its commit is made in.
    const guide = files.find((f) => f.path === "vendor/kit/docs/guide.md");
    expect(guide?.repositoryChain).toEqual([
      { path: "vendor/kit", label: "vendor/kit" },
      { path: "", label: "acme" },
    ]);
  });

  it("CHG-FR-AYTR: the pointer row's own check survives an implied check that ends", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    await userEvent.click(checkbox("kit"));
    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("kit")).toBeDisabled();
    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("kit")).toBeChecked();
    expect(checkbox("kit")).toBeEnabled();
    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalledOnce());
    expect(onRequestCommitMessage.mock.calls[0][0].map((f) => f.path)).toEqual(["vendor/kit"]);
  });

  it("CHG-FR-AYTR, CHG-FR-28: a cascade over a group with an implied pointer leaves no stuck check", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    await userEvent.click(checkbox("guide.md"));
    const mainBox = () => screen.getByLabelText("Include repository acme") as HTMLInputElement;
    await userEvent.click(mainBox());
    expect(checkbox("Library.tsx")).toBeChecked();
    expect(mainBox()).toBeChecked();
    await userEvent.click(mainBox());
    expect(checkbox("Library.tsx")).not.toBeChecked();
    // The pointer stays implied by the submodule file, and the repository
    // checkbox, which counts only the rows the author can toggle, unchecks.
    expect(checkbox("kit")).toBeChecked();
    expect(checkbox("kit")).toBeDisabled();
    expect(mainBox().indeterminate).toBe(false);
    expect(mainBox()).not.toBeChecked();
    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("kit")).not.toBeChecked();
    expect(checkbox("kit")).toBeEnabled();
  });

  it("CHG-FR-AYTR, CHG-FR-28: a folder whose only visible rows are implied pointers renders checked and inert", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    // `vendor` in acme › Revisioned holds the pointer row alone.
    expect(checkbox("vendor")).not.toBeChecked();
    expect(checkbox("vendor")).toBeEnabled();
    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("vendor")).toBeChecked();
    expect(checkbox("vendor")).toBeDisabled();
    // `src` holds a file the author can toggle, so its check is not touched.
    expect(checkbox("src")).not.toBeChecked();
    expect(checkbox("src")).toBeEnabled();
    await userEvent.click(checkbox("guide.md"));
    expect(checkbox("vendor")).not.toBeChecked();
    expect(checkbox("vendor")).toBeEnabled();
  });

  it("CHG-FR-AYTR, CHG-FR-28: a group cascade does not add an implied pointer as a check of its own", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    await userEvent.click(checkbox("guide.md"));
    // Check, then uncheck, the main repository while the pointer is implied.
    await userEvent.click(screen.getByLabelText("Include repository acme"));
    await userEvent.click(checkbox("guide.md"));
    // The pointer row now holds no check: the cascade passed it by.
    expect(checkbox("kit")).not.toBeChecked();
    expect(checkbox("Library.tsx")).toBeChecked();
  });

  it("CHG-FR-AYTR, CHG-FR-27: a checked file the filters hide implies no pointer", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    await userEvent.click(checkbox("guide.md"));
    // `kit` matches the pointer and not the file.
    await userEvent.type(screen.getByLabelText("Filter files"), "kit");
    await waitFor(() => expect(rowNames()).not.toContain("guide.md"));
    expect(checkbox("kit")).not.toBeChecked();
    expect(checkbox("kit")).toBeEnabled();
    expect(screen.getByText("0 files selected")).toBeInTheDocument();
  });

  it("CHG-FR-AYTR: a file of a nested submodule implies both pointers in the panel", async () => {
    const DEEP: RepositoryInfo = { path: "vendor/kit/deep", name: "deep", parent: "vendor/kit", branch: "dev" };
    const DEEP_FILE = change("vendor/kit/deep/x.md", { repository: DEEP.path });
    const DEEP_POINTER = change("vendor/kit/deep", {
      repository: "vendor/kit",
      entryKind: "submodule",
      previousCommit: "dddddddd",
      currentCommit: "eeeeeeee",
    });
    backend({
      strict: true,
      uncommitted: changeSet([POINTER, DEEP_POINTER, DEEP_FILE], undefined, [MAIN, KIT, DEEP]),
    });
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("x.md"));
    await userEvent.click(checkbox("x.md"));
    expect(checkbox("kit")).toBeChecked();
    expect(checkbox("kit")).toBeDisabled();
    expect(checkbox("deep")).toBeChecked();
    expect(checkbox("deep")).toBeDisabled();
    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalledOnce());
    const file = onRequestCommitMessage.mock.calls[0][0].find((f) => f.path === DEEP_FILE.path);
    expect(file?.repositoryChain?.map((r) => r.path)).toEqual(["vendor/kit/deep", "vendor/kit", ""]);
  });

  it("CHG-FR-AYTR: a pointer the filters hide is not offered as a hidden file, because the commit records it", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("guide.md"));
    await userEvent.click(checkbox("guide.md"));
    // Narrow the text so the pointer row is hidden and the file stays.
    await userEvent.type(screen.getByLabelText("Filter files"), "guide");
    await waitFor(() => expect(rowNames()).not.toContain("kit"));
    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalledOnce());
    const [files, hidden] = onRequestCommitMessage.mock.calls[0];
    expect(files.map((f) => f.path)).toEqual(["vendor/kit/docs/guide.md"]);
    expect(hidden.map((f) => f.path)).toEqual([LIB.path]);
  });

  it("CHG-FR-DGHI: with only pointer rows checked the rollback button is unavailable and says why", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("kit"));
    await userEvent.click(checkbox("kit"));
    expect(rollbackButton()).toBeDisabled();
    expect(rollbackButton().title).toContain("a submodule pointer cannot be rolled back");
    expect(rollbackButton().title).not.toContain("no files are selected");
  });

  it("CHG-FR-DGHI, CHG-FR-57: a checked pointer row never joins the rollback set", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(rowNames()).toContain("kit"));
    await userEvent.click(checkbox("kit"));
    await userEvent.click(checkbox("Library.tsx"));
    expect(rollbackButton()).toBeEnabled();
    await userEvent.click(rollbackButton());
    await waitFor(() => expect(onRequestRollback).toHaveBeenCalledOnce());
    const [files] = onRequestRollback.mock.calls[0];
    expect(files.map((f) => f.path)).toEqual([LIB.path]);
  });
});

describe("the pointer row at a narrow width (CHG-FR-GBTE)", () => {
  it("CHG-FR-GBTE: the current id never shrinks, and the previous id yields and ellipsises first", () => {
    const css = readStylesheet("components.css");
    expect(cssRule(css, ".change-row__stat--pointer")).toMatch(/display:\s*contents/);
    const previous = cssRule(css, ".change-row__pointer-previous");
    expect(previous).toMatch(/min-width:\s*0/);
    expect(previous).toMatch(/text-overflow:\s*ellipsis/);
    expect(previous).toMatch(/flex:\s*1 1 0/);
    expect(cssRule(css, ".change-row__pointer")).toMatch(/flex:\s*none/);
  });
});

describe("reveal (CHG-FR-54)", () => {
  it("CHG-FR-54: a reveal of a submodule file expands its repository group and selects the row, with no check", async () => {
    const view = renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    // Collapse the submodule group and its folder first.
    await userEvent.click(row("docs"));
    await userEvent.click(repositoryRows()[1]);
    expect(rowNames()).not.toContain("guide.md");

    view.revealFile(KIT_FILE.path);
    await waitFor(() => expect(row("guide.md")).toHaveAttribute("data-selected", "true"));
    expect(row("guide.md")).toHaveAttribute(
      "data-node-key",
      "m:vendor/kit|r:f:vendor/kit/docs/guide.md",
    );
    expect(checkbox("guide.md")).not.toBeChecked();
  });
});

describe("Push over more than one repository (CHG-FR-KQWD, CHG-FR-NUZB)", () => {
  const sync = (repositories: UpstreamSyncState["repositories"]): UpstreamSyncState => ({
    hasRemote: true,
    hasUpstream: true,
    ahead: 0,
    behind: 0,
    repositories,
  });

  it("CHG-FR-KQWD: Push is enabled while a submodule on a branch holds commits its remote does not", async () => {
    backend({
      strict: true,
      uncommitted: multiSet(),
      prefs: { changesCommitAction: "push" },
      sync: sync([
        { repository: "", branch: "feature/x", hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 },
        { repository: "vendor/kit", branch: "main", hasRemote: true, hasUpstream: true, ahead: 3, behind: 0 },
      ]),
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());
  });

  it("CHG-FR-KQWD: a detached submodule never makes Push enabled", async () => {
    backend({
      strict: true,
      uncommitted: multiSet(),
      prefs: { changesCommitAction: "push" },
      sync: sync([
        { repository: "", branch: "feature/x", hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 },
        { repository: "vendor/kit", hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      ]),
    });
    renderPanel();
    // Wait for the reason only the applied answer gives: an unread state
    // states another reason.
    await waitFor(() =>
      expect(primary().title).toContain("no repository on a branch holds commits"),
    );
    expect(primary()).toBeDisabled();
    expect(calls("get_upstream_sync_state").length).toBeGreaterThan(0);
  });

  it("CHG-FR-NUZB: when a push ends, the panel states each skipped repository and the reason", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    await act(async () => {
      finishedListener()({
        payload: {
          operation: "push",
          ok: true,
          pushSummary: {
            pushed: [""],
            skipped: [
              { repository: "vendor/kit", reason: "detached_head" },
              { repository: "vendor/quiet", reason: "no_remote" },
            ],
          },
        },
      });
    });
    const list = await screen.findByRole("list", { name: "Skipped repositories" });
    expect(list).toHaveTextContent("Skipped vendor/kit: detached HEAD.");
    expect(list).toHaveTextContent("Skipped vendor/quiet: no remote configured.");
    expect(screen.getByText("Push complete.", { exact: false })).toBeInTheDocument();
  });

  it("CHG-FR-NUZB: a push that skipped no repository states nothing more than CHG-FR-43", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    await act(async () => {
      finishedListener()({
        payload: { operation: "push", ok: true, pushSummary: { pushed: ["", "vendor/kit"], skipped: [] } },
      });
    });
    expect(await screen.findByRole("status")).toHaveTextContent(/^Push complete\.$/);
    expect(screen.queryByRole("list", { name: "Skipped repositories" })).toBeNull();
  });

  const skipKit = (ok: boolean) =>
    act(async () => {
      finishedListener()({
        payload: {
          operation: "push",
          ok,
          ...(ok ? {} : { error: "remote rejected" }),
          pushSummary: {
            pushed: [],
            skipped: [
              { repository: "vendor/kit", reason: "detached_head" },
              { repository: "", reason: "no_remote" },
            ],
          },
        },
      });
    });

  it("CHG-FR-NUZB: a failed push that skipped repositories states the failure and the skips", async () => {
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    await skipKit(false);
    expect(await screen.findByText(/Push failed/)).toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Skipped repositories" });
    expect(list).toHaveTextContent("Skipped vendor/kit: detached HEAD.");
    // The main repository is named by its name.
    expect(list).toHaveTextContent("Skipped acme: no remote configured.");
  });

  it("CHG-FR-NUZB: the skips of an ended push do not stay under a later rollback note", async () => {
    onRequestRollback.mockImplementation(async () => ({
      outcome: {
        entries: [
          {
            id: LIB.path,
            path: LIB.path,
            outcome: "restored",
            restoredPaths: [LIB.path],
            removedPaths: [],
            failures: [],
          },
        ],
      },
      saveFailures: [],
    }));
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(repositoryRows()).toHaveLength(2));
    await skipKit(true);
    expect(await screen.findByRole("list", { name: "Skipped repositories" })).toBeInTheDocument();
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(rollbackButton());
    expect(await screen.findByText("Discarded 1 file.")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Skipped repositories" })).toBeNull();
  });

  it("CHG-FR-NUZB: a new push clears the skips of the push before it", async () => {
    backend({
      strict: true,
      uncommitted: multiSet(),
      prefs: { changesCommitAction: "push" },
      sync: sync([
        { repository: "", branch: "feature/x", hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 },
        { repository: "vendor/kit", branch: "main", hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 },
      ]),
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());
    await skipKit(true);
    expect(await screen.findByRole("list", { name: "Skipped repositories" })).toBeInTheDocument();
    await userEvent.click(primary());
    await waitFor(() => expect(calls("push_current_branch")).toHaveLength(1));
    await waitFor(() =>
      expect(screen.queryByRole("list", { name: "Skipped repositories" })).toBeNull(),
    );
  });
});
