import { beforeEach, describe, expect, it } from "vitest";

import {
  clearToasts,
  dismissToast,
  MAX_VISIBLE_TOASTS,
  queuedToasts,
  removeToastsForAddress,
  removeToastsForKey,
  resetToasts,
  showToast,
  visibleToasts,
  type ToastInput,
} from "./toasts";

const toast = (key: string, over: Partial<ToastInput> = {}): ToastInput => ({
  key,
  level: "Info",
  title: `title ${key}`,
  body: `body ${key}`,
  address: `synthesis://p/w/run/${key}`,
  ...over,
});

const keys = () => visibleToasts().map((t) => t.key);

beforeEach(() => {
  resetToasts();
});

describe("the toast store: stacking (NTF-FR-HZNF, NTF-FR-SXTI)", () => {
  it("NTF-FR-HZNF: shows the newest toast on top", () => {
    showToast(toast("a"));
    showToast(toast("b"));
    showToast(toast("c"));
    expect(keys()).toEqual(["c", "b", "a"]);
  });

  it("NTF-FR-HZNF: shows at most three toasts", () => {
    for (const k of ["a", "b", "c", "d", "e"]) showToast(toast(k));
    expect(MAX_VISIBLE_TOASTS).toBe(3);
    expect(keys()).toEqual(["c", "b", "a"]);
    expect(queuedToasts().map((t) => t.key)).toEqual(["d", "e"]);
  });

  it("NTF-FR-SXTI: shows a queued toast, in order of arrival, when a slot frees", () => {
    for (const k of ["a", "b", "c", "d", "e"]) showToast(toast(k));
    dismissToast("b");
    expect(keys()).toEqual(["d", "c", "a"]);
    dismissToast("a");
    expect(keys()).toEqual(["e", "d", "c"]);
    expect(queuedToasts()).toEqual([]);
  });

  it("NTF-FR-HZNF: replaces a showing toast in place and counts the revision", () => {
    showToast(toast("a"));
    showToast(toast("b"));
    showToast(toast("c"));
    showToast(toast("b", { level: "Error", title: "new" }));
    expect(keys()).toEqual(["c", "b", "a"]);
    const b = visibleToasts()[1];
    expect(b).toMatchObject({ level: "Error", title: "new", revision: 1 });
    showToast(toast("b"));
    expect(visibleToasts()[1].revision).toBe(2);
  });

  it("NTF-FR-HZNF: a replacement never uses a second slot", () => {
    for (const k of ["a", "b", "c"]) showToast(toast(k));
    showToast(toast("a", { title: "again" }));
    expect(visibleToasts()).toHaveLength(3);
    expect(queuedToasts()).toEqual([]);
  });

  it("NTF-FR-SXTI, NTF-FR-20: a priority toast waits at the front of the queue", () => {
    for (const k of ["a", "b", "c", "d"]) showToast(toast(k));
    showToast(toast("p"), { priority: true });
    expect(queuedToasts().map((t) => t.key)).toEqual(["p", "d"]);
    dismissToast("a");
    expect(keys()).toEqual(["p", "c", "b"]);
  });

  it("NTF-FR-SXTI: a raise of a queued key takes the new content in its queue place", () => {
    for (const k of ["a", "b", "c", "d", "e"]) showToast(toast(k));
    showToast(toast("d", { title: "newer" }));
    expect(queuedToasts().map((t) => [t.key, t.title])).toEqual([
      ["d", "newer"],
      ["e", "title e"],
    ]);
  });
});

describe("the toast store: removal (NTF-FR-ZSIK, NTF-FR-14, NTF-FR-38, NTF-FR-15)", () => {
  it("NTF-FR-QEHM, NTF-FR-ZSIK: dismissing removes only that toast", () => {
    showToast(toast("a"));
    showToast(toast("b"));
    dismissToast("a");
    expect(keys()).toEqual(["b"]);
  });

  it("NTF-FR-14: reaching an address removes every toast that names it", () => {
    showToast(toast("a", { address: "x" }));
    showToast(toast("b", { address: "y" }));
    showToast(toast("c", { address: "x" }));
    removeToastsForAddress("x");
    expect(keys()).toEqual(["b"]);
  });

  it("NTF-FR-14: a toast without an address is not reached by any address", () => {
    showToast(toast("a", { address: null }));
    removeToastsForAddress("x");
    expect(keys()).toEqual(["a"]);
  });

  it("NTF-FR-SXTI, NTF-FR-14: drops a queued toast whose address is reached", () => {
    for (const k of ["a", "b", "c"]) showToast(toast(k));
    showToast(toast("d", { address: "x" }));
    removeToastsForAddress("x");
    dismissToast("a");
    expect(keys()).toEqual(["c", "b"]);
    expect(queuedToasts()).toEqual([]);
  });

  it("NTF-FR-SXTI, NTF-FR-38: drops a queued toast whose key is retracted", () => {
    for (const k of ["a", "b", "c", "d"]) showToast(toast(k));
    removeToastsForKey("d");
    expect(queuedToasts()).toEqual([]);
    expect(keys()).toEqual(["c", "b", "a"]);
  });

  it("NTF-FR-15: clearing removes the shown and the queued toasts", () => {
    for (const k of ["a", "b", "c", "d"]) showToast(toast(k, { address: null }));
    clearToasts();
    expect(keys()).toEqual([]);
    expect(queuedToasts()).toEqual([]);
  });
});
