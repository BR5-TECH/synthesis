import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, renderHook } from "@testing-library/react";

import { useTabScrollPruning } from "./tabScrollPruning";
import {
  holdTabScroll,
  resetTabScrollForTest,
  tabScrollOf,
} from "../../state/tabScroll";
import type { Tab } from "../../types";

const tab = (id: string): Tab => ({ id, label: id });

beforeEach(() => {
  resetTabScrollForTest();
});

afterEach(cleanup);

describe("useTabScrollPruning", () => {
  it("EDT-FR-UERT, NAW-FR-IHDU, DFV-FR-YMJA: a closed tab forgets its scroll position and the open tabs keep theirs", () => {
    holdTabScroll("a.md", "a.md\u0000wysiwyg", 300);
    holdTabScroll("draft:d1", "doc", 120);
    const { rerender } = renderHook(({ tabs }) => useTabScrollPruning(tabs), {
      initialProps: { tabs: [tab("a.md"), tab("draft:d1")] },
    });
    expect(tabScrollOf("a.md", "a.md\u0000wysiwyg")).toBe(300);

    rerender({ tabs: [tab("draft:d1")] });
    expect(tabScrollOf("a.md", "a.md\u0000wysiwyg")).toBe(0);
    expect(tabScrollOf("draft:d1", "doc")).toBe(120);

    // A new tab with the same id starts at the top.
    rerender({ tabs: [tab("a.md"), tab("draft:d1")] });
    expect(tabScrollOf("a.md", "a.md\u0000wysiwyg")).toBe(0);
  });
});
