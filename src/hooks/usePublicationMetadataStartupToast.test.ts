/**
 * The toast of a failed startup refresh of the publication metadata
 * (`../../specifications/ui/NTF-notifications.md` NTF-FR-POEW, NTF-FR-24,
 * served by `../../specifications/core/GHP-github-publication.md` GHP-FR-ISJI
 * and GHP-FR-KPXF).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

type Failure = { errorCode: string; error: string };

/** The failure that the mocked backend keeps until it is taken. */
let kept: Failure | null = null;
/** Whether the mocked backend refuses the take. */
let refuseTake = false;
const invokeMock = vi.fn((cmd: string, _args?: unknown): Promise<unknown> => {
  if (cmd === "append_log_records") return Promise.resolve();
  if (cmd === "take_publication_metadata_startup_failure") {
    if (refuseTake) return Promise.reject(new Error("no project"));
    const failure = kept;
    kept = null;
    return Promise.resolve(failure);
  }
  return Promise.reject(new Error(`unexpected command ${cmd}`));
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

import {
  METADATA_STARTUP_FAILED_KEY,
  METADATA_STARTUP_FAILED_TITLE,
  usePublicationMetadataStartupToast,
} from "./usePublicationMetadataStartupToast";
import { PUBLICATION_METADATA_STARTUP_FAILED } from "../events";
import { flushLogs, resetLogBufferForTest } from "../logging";
import { resetToasts, visibleToasts } from "../state/toasts";

const TAKE = "take_publication_metadata_startup_failure";
const ROOT = "/p/acme\u0000/p/acme";

/** The backend keeps the failure and emits the event, as GHP-FR-ISJI states. */
function raiseFailed(errorCode: string, error: string) {
  kept = { errorCode, error };
  return act(async () => {
    for (const cb of listeners.get(PUBLICATION_METADATA_STARTUP_FAILED) ?? []) {
      cb({ payload: { errorCode, error } });
    }
  });
}

function takes() {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === TAKE).length;
}

async function mountListening() {
  const hook = renderHook(() => usePublicationMetadataStartupToast(ROOT));
  await waitFor(() => expect(listeners.get(PUBLICATION_METADATA_STARTUP_FAILED)?.size).toBe(1));
  await waitFor(() => expect(takes()).toBe(1));
  return hook;
}

beforeEach(() => {
  kept = null;
  refuseTake = false;
  listeners.clear();
  invokeMock.mockClear();
  resetLogBufferForTest();
  resetToasts();
});

afterEach(cleanup);

describe("usePublicationMetadataStartupToast", () => {
  it("NTF-FR-POEW, NTF-FR-24: a failed startup refresh shows one Error toast with the displayable error and no address", async () => {
    await mountListening();

    await raiseFailed("token_unavailable", "The project has no GitHub token.");

    await waitFor(() =>
      expect(visibleToasts()).toEqual([
        expect.objectContaining({
          key: METADATA_STARTUP_FAILED_KEY,
          level: "Error",
          title: "GitHub publication data not loaded",
          body: "The project has no GitHub token.",
          address: null,
        }),
      ]),
    );
    expect(METADATA_STARTUP_FAILED_TITLE).toBe("GitHub publication data not loaded");
    expect(METADATA_STARTUP_FAILED_KEY).toBe("publication-metadata-refresh-failed");
  });

  it("NTF-FR-POEW, GHP-FR-KPXF: a failure raised before the listener exists shows one toast when the window starts to listen", async () => {
    kept = { errorCode: "no_remote_configured", error: "The project has no Git remote." };

    renderHook(() => usePublicationMetadataStartupToast(ROOT));

    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toEqual(
      expect.objectContaining({
        key: METADATA_STARTUP_FAILED_KEY,
        level: "Error",
        body: "The project has no Git remote.",
      }),
    );
    expect(kept).toBeNull();
  });

  it("NTF-FR-POEW: with no kept failure the window shows nothing when it starts to listen", async () => {
    await mountListening();
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NTF-FR-POEW: a second failure replaces the toast rather than stacking one more", async () => {
    await mountListening();

    await raiseFailed("github_unreachable", "GitHub could not be reached.");
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    await raiseFailed("milestones_unreadable", "The milestones could not be read.");

    await waitFor(() =>
      expect(visibleToasts()[0].body).toBe("The milestones could not be read."),
    );
    expect(visibleToasts()).toHaveLength(1);
  });

  it("NTF-FR-POEW: a take that the backend refuses shows nothing", async () => {
    refuseTake = true;
    kept = { errorCode: "github_unreachable", error: "GitHub could not be reached." };
    renderHook(() => usePublicationMetadataStartupToast(ROOT));
    await waitFor(() => {
      flushLogs();
      expect(
        invokeMock.mock.calls.some(
          ([cmd, args]) =>
            cmd === "append_log_records" &&
            JSON.stringify(args).includes("publication metadata startup failure not read"),
        ),
      ).toBe(true);
    });
    expect(takes()).toBe(1);
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NTF-FR-POEW: the subscription ends with the window", async () => {
    const { unmount } = await mountListening();
    unmount();
    await waitFor(() => expect(listeners.get(PUBLICATION_METADATA_STARTUP_FAILED)?.size).toBe(0));
    await raiseFailed("github_unreachable", "GitHub could not be reached.");
    expect(visibleToasts()).toHaveLength(0);
    expect(takes()).toBe(1);
  });

  it("NTF-FR-POEW, NTF-FR-15: with no open project the window neither listens nor takes, so the backend keeps the failure", async () => {
    kept = { errorCode: "token_unavailable", error: "The project has no GitHub token." };
    const { rerender } = renderHook(
      ({ root }: { root: string | null }) => usePublicationMetadataStartupToast(root),
      { initialProps: { root: null as string | null } },
    );
    await act(async () => {});
    expect(listeners.get(PUBLICATION_METADATA_STARTUP_FAILED)?.size ?? 0).toBe(0);
    expect(takes()).toBe(0);
    expect(kept).not.toBeNull();

    rerender({ root: ROOT });

    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0].body).toBe("The project has no GitHub token.");
  });
});
