import { beforeEach, describe, expect, it, vi } from "vitest";

const coreInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => coreInvoke(...a) }));

import { resetInvokeOrderForTest, setWindowProjectId } from "./invoke";
import { activateProjectWindow, setProjectWindowAttention } from "./projectWindows";

const flush = () => new Promise<void>((r) => setTimeout(r, 0));

describe("project window call order (CEX-FR-QZPN)", () => {
  beforeEach(() => {
    coreInvoke.mockReset();
    setWindowProjectId(null);
    resetInvokeOrderForTest();
  });

  it("CEX-FR-QZPN, CEX-FR-FUUP: an activation sent without waiting starts only after the one before it settles", async () => {
    const releases: Array<() => void> = [];
    coreInvoke.mockImplementation(
      () => new Promise<void>((resolve) => releases.push(() => resolve())),
    );
    void activateProjectWindow("/a");
    void activateProjectWindow("/b");
    void setProjectWindowAttention("/b", true);
    await flush();
    expect(coreInvoke.mock.calls.map((c) => c[1])).toEqual([{ projectId: "/a" }]);
    releases[0]();
    await flush();
    expect(coreInvoke).toHaveBeenCalledTimes(2);
    expect(coreInvoke.mock.calls[1][1]).toEqual({ projectId: "/b" });
    releases[1]();
    await flush();
    expect(coreInvoke.mock.calls[2]).toEqual([
      "set_project_window_attention",
      { projectId: "/b", needsAttention: true },
    ]);
  });

  it("CEX-FR-QZPN: a call that fails does not stop the calls after it", async () => {
    coreInvoke.mockRejectedValueOnce(new Error("project_not_open"));
    coreInvoke.mockResolvedValue(undefined);
    const first = activateProjectWindow("/gone");
    const second = activateProjectWindow("/a");
    await expect(first).rejects.toThrow("project_not_open");
    await expect(second).resolves.toBeUndefined();
    expect(coreInvoke).toHaveBeenCalledTimes(2);
  });
});
