import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import {
  discussionColumnState,
  draftColumn,
  expandColumnHistory,
  fileColumn,
  resetDiscussionColumns,
  restoreDiscussionColumns,
  setColumnHidden,
  setColumnRatio,
  useDiscussionColumn,
} from "./discussionColumn";
import {
  patchLayoutPreferences,
  resetLayoutPreferencesCache,
} from "./layoutPreferences";
import { MAX_FRACTION, MIN_FRACTION } from "../components/DraftDiscussion/ratio";
import type { LayoutPreferences } from "../types";

// The split of a tab between its document column and its discussion column
// (DDS-FR-PNXR, DDS-FR-XQMF), kept per draft and per file (CMT-FR-30) in the
// per-project layout record (GSS-FR-17). A draft and a file are two kinds of
// key, and each kind has its own two fields in the record.

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** Every record handed to `save_layout_preferences`, in order. */
let saved: LayoutPreferences[];
/** What `load_layout_preferences` gives back. */
let stored: LayoutPreferences | null;
let loadFails: boolean;
let saveFails: boolean;

const DEFAULT = { ratio: "even", hidden: false, historyExpanded: false };

beforeEach(() => {
  saved = [];
  stored = null;
  loadFails = false;
  saveFails = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_layout_preferences") {
        if (loadFails) throw new Error("store unreadable");
        return stored;
      }
      if (cmd === "save_layout_preferences") {
        if (saveFails) throw new Error("disk full");
        saved.push(args?.preferences as LayoutPreferences);
        return undefined;
      }
      // The log sink, and nothing else, is expected in the background.
      if (cmd === "append_log_records") return undefined;
      return Promise.reject(new Error(`unexpected command ${cmd}`));
    },
  );
  resetLayoutPreferencesCache();
  resetDiscussionColumns();
});

afterEach(() => {
  resetDiscussionColumns();
  resetLayoutPreferencesCache();
});

const saveCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "save_layout_preferences");

/** Wait until `count` records are saved, and give back the last one. */
async function lastSaved(count: number): Promise<LayoutPreferences> {
  await vi.waitFor(() => expect(saved).toHaveLength(count));
  return saved[saved.length - 1];
}

describe("the default state of a column", () => {
  it("DDS-FR-PNXR, DDS-FR-XQMF, CMT-FR-30: a column nothing was set on is split evenly and shown", () => {
    expect(discussionColumnState(fileColumn("specs/a.md"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("d1"))).toEqual(DEFAULT);
  });

  it("CMT-FR-30: a draft id and a file path that are the same text name two columns", () => {
    expect(draftColumn("a.md")).not.toBe(fileColumn("a.md"));
    setColumnHidden(fileColumn("a.md"), true);
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(true);
    expect(discussionColumnState(draftColumn("a.md")).hidden).toBe(false);
  });
});

describe("setting a column", () => {
  it("DDS-FR-PNXR: sets the ratio of one column and leaves the others", () => {
    setColumnRatio(fileColumn("a.md"), "chat70");
    setColumnRatio(draftColumn("d1"), 0.4);

    expect(discussionColumnState(fileColumn("a.md"))).toEqual({
      ...DEFAULT,
      ratio: "chat70",
    });
    expect(discussionColumnState(draftColumn("d1")).ratio).toBe(0.4);
    expect(discussionColumnState(fileColumn("b.md"))).toEqual(DEFAULT);
  });

  it("DDS-FR-XQMF: hides a column and shows it again, and the ratio stays", () => {
    const key = fileColumn("a.md");
    setColumnRatio(key, "doc70");
    setColumnHidden(key, true);
    expect(discussionColumnState(key)).toEqual({
      ...DEFAULT,
      ratio: "doc70",
      hidden: true,
    });

    setColumnHidden(key, false);
    expect(discussionColumnState(key)).toEqual({ ...DEFAULT, ratio: "doc70" });
  });

  it("DDS-FR-CLBK: expands the history of one column in memory only", async () => {
    await restoreDiscussionColumns("/p");
    expandColumnHistory(fileColumn("a.md"));

    expect(discussionColumnState(fileColumn("a.md")).historyExpanded).toBe(true);
    expect(discussionColumnState(fileColumn("b.md")).historyExpanded).toBe(false);
    // Whether a long head was expanded belongs to this sitting.
    expect(saveCalls()).toHaveLength(0);
  });

  it("DDS-FR-XQMF: a component that reads a column renders again when it changes", () => {
    const { result } = renderHook(() => useDiscussionColumn(fileColumn("a.md")));
    expect(result.current.hidden).toBe(false);

    act(() => setColumnHidden(fileColumn("a.md"), true));
    expect(result.current.hidden).toBe(true);

    // A change on another column gives the same state object back.
    const before = result.current;
    act(() => setColumnHidden(fileColumn("b.md"), true));
    expect(result.current).toBe(before);
  });
});

describe("persisting a column (GSS-FR-17)", () => {
  it("CMT-FR-30, GSS-FR-17: writes nothing before a project has been restored", async () => {
    setColumnHidden(fileColumn("a.md"), true);
    setColumnRatio(draftColumn("d1"), "chat70");
    // The state changes on screen all the same.
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(true);
    // Writes are serialised, so a write queued after these runs after any
    // write they started. Only that one write reaches the record.
    const record = await patchLayoutPreferences("/p", { verticalPanelFraction: 0.5 });
    expect(saveCalls()).toHaveLength(1);
    expect(record).toEqual({ verticalPanelFraction: 0.5 });
  });

  it("CMT-FR-30, DDS-FR-XQMF, GSS-FR-17: a file's hidden state goes into fileDiscussionHidden, and every other entry stays", async () => {
    stored = {
      verticalPanelFraction: 0.4,
      draftDiscussionRatios: { other: "doc70" },
      draftDiscussionHidden: { other: true },
      fileDiscussionHidden: { "b.md": true },
    };
    await restoreDiscussionColumns("/p");

    setColumnHidden(fileColumn("specs/a.md"), true);

    expect(await lastSaved(1)).toEqual({
      verticalPanelFraction: 0.4,
      draftDiscussionRatios: { other: "doc70" },
      draftDiscussionHidden: { other: true },
      fileDiscussionHidden: { "b.md": true, "specs/a.md": true },
    });
  });

  it("CMT-FR-30, DDS-FR-PNXR, GSS-FR-17: a file's ratio goes into fileDiscussionRatios, by name or as a fraction", async () => {
    stored = { fileDiscussionRatios: { "b.md": "doc70" } };
    await restoreDiscussionColumns("/p");

    setColumnRatio(fileColumn("a.md"), "chat70");
    expect(await lastSaved(1)).toEqual({
      fileDiscussionRatios: { "b.md": "doc70", "a.md": "chat70" },
    });

    // A dragged split is kept as the document column's fraction.
    setColumnRatio(fileColumn("a.md"), 0.6);
    expect(await lastSaved(2)).toEqual({
      fileDiscussionRatios: { "b.md": "doc70", "a.md": "0.6" },
    });
  });

  it("DDS-FR-PNXR, DDS-FR-XQMF, GSS-FR-17: a draft's column goes into the draftDiscussion fields, never into the file ones", async () => {
    stored = { fileDiscussionRatios: { d1: "doc70" } };
    await restoreDiscussionColumns("/p");

    setColumnRatio(draftColumn("d1"), "chat70");
    setColumnHidden(draftColumn("d1"), true);

    expect(await lastSaved(2)).toEqual({
      fileDiscussionRatios: { d1: "doc70" },
      draftDiscussionRatios: { d1: "chat70" },
      draftDiscussionHidden: { d1: true },
    });
  });

  it("DDS-FR-XQMF, GSS-FR-17: a write the store refuses leaves the state on screen and raises nothing", async () => {
    await restoreDiscussionColumns("/p");
    saveFails = true;

    setColumnHidden(fileColumn("a.md"), true);

    // The refused write ran: an unhandled rejection would fail the run here.
    await vi.waitFor(() => expect(saveCalls()).toHaveLength(1));
    expect(saved).toHaveLength(0);
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(true);
  });

  it("CMT-FR-36, GSS-FR-17: a write that changes nothing stores nothing", async () => {
    await restoreDiscussionColumns("/p");

    setColumnHidden(fileColumn("a.md"), false);
    setColumnRatio(fileColumn("a.md"), "even");
    setColumnHidden(fileColumn("a.md"), true);

    // The writes are stored in order, so a stored no-op would land before the
    // real write and show here.
    await vi.waitFor(() =>
      expect(saved.some((r) => r.fileDiscussionHidden?.["a.md"] === true)).toBe(true),
    );
    expect(saved.filter((r) => r.fileDiscussionHidden?.["a.md"] === false)).toHaveLength(0);
    expect(saved.some((r) => r.fileDiscussionRatios !== undefined)).toBe(false);
    expect(saveCalls()).toHaveLength(1);
  });

  it("GSS-FR-17: two quick writes to two columns of one kind both reach the record", async () => {
    // Each write folds its entry into the record as it stands when the write
    // runs. A map built from an earlier copy would drop the first entry.
    await restoreDiscussionColumns("/p");

    setColumnHidden(fileColumn("a.md"), true);
    setColumnHidden(fileColumn("b.md"), true);

    expect(await lastSaved(2)).toEqual({
      fileDiscussionHidden: { "a.md": true, "b.md": true },
    });
  });
});

describe("restoring the columns of a project (CMT-FR-30, GSS-FR-17)", () => {
  it("DDS-FR-PNXR, DDS-FR-XQMF, CMT-FR-30: restores the drafts' and the files' columns, each under its own kind", async () => {
    stored = {
      draftDiscussionRatios: { d1: "doc70", d2: "0.6" },
      draftDiscussionHidden: { d1: true },
      fileDiscussionRatios: { "specs/a.md": "chat70" },
      fileDiscussionHidden: { "specs/a.md": true, "src/main.rs": false },
    };

    await restoreDiscussionColumns("/p");

    expect(discussionColumnState(draftColumn("d1"))).toEqual({
      ...DEFAULT,
      ratio: "doc70",
      hidden: true,
    });
    expect(discussionColumnState(draftColumn("d2")).ratio).toBe(0.6);
    expect(discussionColumnState(fileColumn("specs/a.md"))).toEqual({
      ...DEFAULT,
      ratio: "chat70",
      hidden: true,
    });
    expect(discussionColumnState(fileColumn("src/main.rs"))).toEqual(DEFAULT);
    // A draft entry does not name a file, and a file entry does not name a draft.
    expect(discussionColumnState(fileColumn("d1"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("specs/a.md"))).toEqual(DEFAULT);
    // Restoring reads the record and writes nothing back.
    expect(saveCalls()).toHaveLength(0);
  });

  it("DDS-FR-PNXR, DDS-FR-XQMF: ignores a stored value this build does not know, and clamps a fraction", async () => {
    stored = {
      draftDiscussionRatios: { d1: "doc90", d2: "", d3: "5", d5: "0", d6: "-1", d7: "abc" },
      draftDiscussionHidden: { d4: "yes" as unknown as boolean },
      fileDiscussionRatios: { "a.md": "wide" },
      fileDiscussionHidden: { "b.md": 1 as unknown as boolean },
    };

    await restoreDiscussionColumns("/p");

    expect(discussionColumnState(draftColumn("d1"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("d2"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("d3")).ratio).toBe(MAX_FRACTION);
    expect(discussionColumnState(draftColumn("d4"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("d5")).ratio).toBe(MIN_FRACTION);
    expect(discussionColumnState(draftColumn("d6")).ratio).toBe(MIN_FRACTION);
    expect(discussionColumnState(draftColumn("d7"))).toEqual(DEFAULT);
    expect(discussionColumnState(fileColumn("a.md"))).toEqual(DEFAULT);
    expect(discussionColumnState(fileColumn("b.md"))).toEqual(DEFAULT);
  });

  it("CMT-FR-30: a record that cannot be read leaves every column at its default", async () => {
    loadFails = true;

    await expect(restoreDiscussionColumns("/p")).resolves.toBeUndefined();

    expect(discussionColumnState(fileColumn("a.md"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("d1"))).toEqual(DEFAULT);
  });

  it("CMT-FR-30: a column the author set before the record arrived keeps the restored entries of other columns beside it", async () => {
    setColumnHidden(fileColumn("a.md"), true);
    stored = { fileDiscussionRatios: { "b.md": "doc70" } };

    await restoreDiscussionColumns("/p");

    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(true);
    expect(discussionColumnState(fileColumn("b.md")).ratio).toBe("doc70");
  });

  it("CMT-FR-30: a column the author set before the record arrived keeps the author's value, not the stored one", async () => {
    stored = { fileDiscussionHidden: { "a.md": true }, fileDiscussionRatios: { "a.md": "doc70" } };
    const restoring = restoreDiscussionColumns("/p");
    setColumnRatio(fileColumn("a.md"), "chat70");

    await restoring;

    expect(discussionColumnState(fileColumn("a.md")).ratio).toBe("chat70");
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(false);
  });

  it("DCR-FR-33, CMT-FR-30: a record that arrives after the project closed restores nothing", async () => {
    stored = { fileDiscussionHidden: { "a.md": true } };
    const restoring = restoreDiscussionColumns("/p");
    resetDiscussionColumns();

    await restoring;

    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(false);
  });

  it("CMT-FR-30, GSS-FR-17: a record read for a project that is no longer open restores nothing", async () => {
    stored = { fileDiscussionHidden: { "a.md": true } };
    const first = restoreDiscussionColumns("/first");
    stored = null;
    const second = restoreDiscussionColumns("/second");

    await Promise.all([first, second]);

    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(false);
  });
});

describe("resetting the columns (DCR-FR-33)", () => {
  it("DDS-FR-PNXR, DDS-FR-XQMF, CMT-FR-30: drops every column's state and stops writing for the closed project", async () => {
    await restoreDiscussionColumns("/p");
    setColumnHidden(fileColumn("a.md"), true);
    setColumnRatio(draftColumn("d1"), "chat70");
    expandColumnHistory(fileColumn("a.md"));
    await lastSaved(2);

    resetDiscussionColumns();

    expect(discussionColumnState(fileColumn("a.md"))).toEqual(DEFAULT);
    expect(discussionColumnState(draftColumn("d1"))).toEqual(DEFAULT);

    // No project is open, so a later change is not written to any slot. A
    // write queued after it runs after any write it started.
    setColumnHidden(fileColumn("c.md"), true);
    const record = await patchLayoutPreferences("/p", { verticalPanelFraction: 0.5 });
    expect(saveCalls()).toHaveLength(3);
    expect(record.fileDiscussionHidden).toEqual({ "a.md": true });
  });

  it("CMT-FR-30: a component that reads a column sees the reset", () => {
    setColumnHidden(fileColumn("a.md"), true);
    const { result } = renderHook(() => useDiscussionColumn(fileColumn("a.md")));
    expect(result.current.hidden).toBe(true);

    act(() => resetDiscussionColumns());
    expect(result.current).toEqual(DEFAULT);
  });
});
