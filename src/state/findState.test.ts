import { describe, expect, it, vi } from "vitest";
import {
  createFindState,
  DEFAULT_FIND_MODE,
  findStateIsPristine,
} from "./findState";
import { EditSessionStore } from "./editSessions";
import { recordEdit } from "./editHistory";

describe("createFindState (EFR-FR-DBOW, EFR-FR-GBJT)", () => {
  it("starts closed, empty, and in case-insensitive literal", () => {
    // EFR-FR-DBOW: an artifact whose panel has never been opened starts in
    // case-insensitive literal — NOT in whatever the universal search bar is
    // set to, which is a separate user-global preference (SCH-FR-13).
    expect(createFindState()).toEqual({
      form: null,
      query: "",
      replacement: "",
      mode: "literal_insensitive",
    });
    expect(DEFAULT_FIND_MODE).toBe("literal_insensitive");
  });

  it("hands out an independent object per artifact", () => {
    // A shared object would leak one artifact's query into every other one,
    // which EFR-FR-GMCE forbids.
    const a = createFindState();
    const b = createFindState();
    a.query = "needle";
    expect(b.query).toBe("");
  });
});

describe("findStateIsPristine (EDT-FR-28)", () => {
  it("is pristine only when nothing the user authored remains", () => {
    expect(findStateIsPristine(createFindState())).toBe(true);
  });

  it("is not pristine while the panel is open in either form", () => {
    expect(findStateIsPristine({ ...createFindState(), form: "find" })).toBe(false);
    expect(findStateIsPristine({ ...createFindState(), form: "replace" })).toBe(
      false,
    );
  });

  it("is not pristine while a query, replacement, or non-default mode remains", () => {
    // A panel closed with the query still in it is state the user authored:
    // EFR-FR-GIPZ, EFR-FR-GBJT, EFR-FR-GNBZ requires it back when the artifact is reopened.
    expect(findStateIsPristine({ ...createFindState(), query: "x" })).toBe(false);
    expect(findStateIsPristine({ ...createFindState(), replacement: "y" })).toBe(
      false,
    );
    expect(findStateIsPristine({ ...createFindState(), mode: "regex" })).toBe(
      false,
    );
  });
});

describe("EditSessionStore find state (EDT-FR-28, EFR-FR-GMCE, EFR-FR-HLGY)", () => {
  it("gives every artifact its own find state (EFR-FR-GMCE)", () => {
    const store = new EditSessionStore();
    store.setFind("a.md", { form: "find", query: "alpha" });
    store.setFind("b.md", { form: "find", query: "beta" });
    expect(store.get("a.md")?.find.query).toBe("alpha");
    expect(store.get("b.md")?.find.query).toBe("beta");
  });

  it("notifies subscribers, because the panel occupies the toolbar band", () => {
    // EFR-FR-ABVQ: opening the panel replaces the formatting toolbar, so unlike a
    // buffer mutation this has to reach the renderer.
    const store = new EditSessionStore();
    const listener = vi.fn();
    store.subscribe(listener);
    store.setFind("a.md", { form: "find" });
    expect(listener).toHaveBeenCalled();
  });

  it("patches rather than replaces, so collapsing to Find keeps the replacement", () => {
    // EFR-FR-BBKS: ⌘F collapses Find & Replace to Find and retains the
    // replacement text so expanding again restores it (EFR-FR-BBKS).
    const store = new EditSessionStore();
    store.setFind("a.md", { form: "replace", query: "q", replacement: "r" });
    store.setFind("a.md", { form: "find" });
    expect(store.get("a.md")?.find).toEqual({
      form: "find",
      query: "q",
      replacement: "r",
      mode: "literal_insensitive",
    });
  });

  it("retains the record across a tab close when only the panel was used (EFR-FR-GIPZ, EFR-FR-GBJT, EFR-FR-GNBZ)", () => {
    // EDT-FR-28: the retained edit state comes into being with the first edit
    // OR the first opening of the find panel. Without that second condition an
    // artifact the user searched but never edited would lose its query on close.
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.setFind("a.md", { form: "find", query: "needle" });
    store.closeTab("a.md");
    expect(store.get("a.md")?.find).toMatchObject({
      form: "find",
      query: "needle",
    });
  });

  it("still drops a record whose panel was opened and fully cleared", () => {
    // Nothing the user authored is left, so there is nothing to bring back —
    // the artifact reopens as a plain first load.
    const store = new EditSessionStore();
    store.openTab("a.md");
    store.setFind("a.md", { form: "find", query: "needle" });
    store.setFind("a.md", { form: null, query: "" });
    store.closeTab("a.md");
    expect(store.get("a.md")).toBeUndefined();
  });

  it("keeps a record that has edits even with a pristine find state", () => {
    const store = new EditSessionStore();
    store.openTab("a.md");
    const s = store.ensure("a.md");
    recordEdit(s.history, "edited", "text", "source");
    store.closeTab("a.md");
    expect(store.get("a.md")).toBeDefined();
  });

  it("leaves the find state untouched when a load is adopted (EFR-FR-HLGY)", () => {
    // EFR-FR-HVVO: "Load from filesystem" replaces the content, not the search
    // over it — the panel stays open with its query, replacement and mode, and
    // only its matches are recomputed.
    const store = new EditSessionStore();
    store.setFind("a.md", {
      form: "replace",
      query: "needle",
      replacement: "thread",
      mode: "regex",
    });
    store.adoptLoad("a.md", "fresh content from disk", "sha-1");
    expect(store.get("a.md")?.find).toEqual({
      form: "replace",
      query: "needle",
      replacement: "thread",
      mode: "regex",
    });
  });
});
