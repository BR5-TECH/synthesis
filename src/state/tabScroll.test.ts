import { beforeEach, describe, expect, it } from "vitest";
import {
  documentSurface,
  holdTabScroll,
  rekeyDocumentScroll,
  pruneTabScroll,
  resetTabScrollForTest,
  tabScrollOf,
} from "./tabScroll";

describe("tab scroll store", () => {
  beforeEach(() => {
    resetTabScrollForTest();
  });

  it("EDT-FR-UERT: a surface with no held offset reads the top", () => {
    expect(tabScrollOf("tab-a", "doc")).toBe(0);
  });

  it("EDT-FR-UERT: an offset reads back for its tab and surface only", () => {
    holdTabScroll("tab-a", "doc", 420);
    holdTabScroll("tab-a", "other", 30);
    holdTabScroll("tab-b", "doc", 99);

    expect(tabScrollOf("tab-a", "doc")).toBe(420);
    expect(tabScrollOf("tab-a", "other")).toBe(30);
    expect(tabScrollOf("tab-b", "doc")).toBe(99);
  });

  it("EDT-FR-UERT: a later offset replaces the earlier one", () => {
    holdTabScroll("tab-a", "doc", 420);
    holdTabScroll("tab-a", "doc", 0);
    expect(tabScrollOf("tab-a", "doc")).toBe(0);
  });

  it("EDT-FR-UERT, NAW-FR-IHDU, DFV-FR-YMJA: pruning forgets every tab that left the strip and keeps the others", () => {
    holdTabScroll("tab-a", "doc", 420);
    holdTabScroll("tab-b", "doc", 99);
    holdTabScroll("tab-c", "doc", 7);

    pruneTabScroll(new Set(["tab-b"]));

    expect(tabScrollOf("tab-a", "doc")).toBe(0);
    expect(tabScrollOf("tab-b", "doc")).toBe(99);
    expect(tabScrollOf("tab-c", "doc")).toBe(0);
  });

  it("NAW-FR-IHDU: a renamed document keeps its offsets in every mode and every tab, and other surfaces stay", () => {
    holdTabScroll("tab-a", documentSurface("d1/old.md", "wysiwyg"), 640);
    holdTabScroll("tab-a", documentSurface("d1/old.md", "text"), 80);
    holdTabScroll("tab-b", documentSurface("d1/old.md", "wysiwyg"), 12);
    holdTabScroll("tab-a", documentSurface("d1/other.md", "wysiwyg"), 33);
    holdTabScroll("tab-a", "comparison", 5);

    rekeyDocumentScroll((id) => (id === "d1/old.md" ? "d1/new.md" : null));

    expect(tabScrollOf("tab-a", documentSurface("d1/new.md", "wysiwyg"))).toBe(640);
    expect(tabScrollOf("tab-a", documentSurface("d1/new.md", "text"))).toBe(80);
    expect(tabScrollOf("tab-b", documentSurface("d1/new.md", "wysiwyg"))).toBe(12);
    expect(tabScrollOf("tab-a", documentSurface("d1/old.md", "wysiwyg"))).toBe(0);
    expect(tabScrollOf("tab-a", documentSurface("d1/other.md", "wysiwyg"))).toBe(33);
    expect(tabScrollOf("tab-a", "comparison")).toBe(5);
  });
});
