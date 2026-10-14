/**
 * The drafts revision and the backend channels that bump it
 * (`DRP-drafts-panel.md` DRP-FR-05).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

const handlers = new Map<string, (event: { payload: unknown }) => void>();
const unlisten = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return unlisten;
  },
}));

import { DRAFT_PUBLICATION_CHANGED, DRAFTS_CHANGED } from "../../events";
import { useDraftsRevision } from "./useDraftsRevision";

beforeEach(() => {
  handlers.clear();
  unlisten.mockReset();
});
afterEach(cleanup);

describe("useDraftsRevision", () => {
  it("DRP-FR-05: a change of a publication store re-lists the drafts", async () => {
    const { result } = renderHook(() => useDraftsRevision());
    await waitFor(() => expect(handlers.has(DRAFT_PUBLICATION_CHANGED)).toBe(true));
    const before = result.current.draftsRevision;

    act(() => handlers.get(DRAFT_PUBLICATION_CHANGED)!({ payload: { draftId: "d1" } }));
    expect(result.current.draftsRevision).toBe(before + 1);
  });

  it("DRP-FR-05: a drafts change still re-lists, and both subscriptions end on unmount", async () => {
    const { result, unmount } = renderHook(() => useDraftsRevision());
    await waitFor(() => expect(handlers.size).toBe(2));
    const before = result.current.draftsRevision;

    act(() => handlers.get(DRAFTS_CHANGED)!({ payload: null }));
    expect(result.current.draftsRevision).toBe(before + 1);

    unmount();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });
});
