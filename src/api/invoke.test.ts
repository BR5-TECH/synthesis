import { beforeEach, describe, expect, it, vi } from "vitest";

const coreInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => coreInvoke(...a) }));

import { invoke, setPayloadProjectIdEnabled, setWindowProjectId } from "./invoke";

describe("invoke", () => {
  beforeEach(() => {
    coreInvoke.mockReset();
    coreInvoke.mockResolvedValue(null);
    setWindowProjectId(null);
    setPayloadProjectIdEnabled(true);
  });

  it("adds nothing while the payload field is off", async () => {
    setPayloadProjectIdEnabled(false);
    setWindowProjectId("/p/one");
    await invoke("a_command", { x: 1 });
    expect(coreInvoke).toHaveBeenCalledWith("a_command", { x: 1 });
  });

  it("sends the payload unchanged while the window holds no project", async () => {
    await invoke("a_command", { x: 1 });
    expect(coreInvoke).toHaveBeenCalledWith("a_command", { x: 1 });
  });

  it("sends a call without a payload with no payload", async () => {
    await invoke("a_command");
    expect(coreInvoke).toHaveBeenCalledWith("a_command");
  });

  it("adds the project id to the payload", async () => {
    setWindowProjectId("/p/one");
    await invoke("a_command", { x: 1 });
    expect(coreInvoke).toHaveBeenCalledWith("a_command", { x: 1, projectId: "/p/one" }, undefined);
  });

  it("sends the project id when the call has no payload", async () => {
    setWindowProjectId("/p/one");
    await invoke("a_command");
    expect(coreInvoke).toHaveBeenCalledWith("a_command", { projectId: "/p/one" }, undefined);
  });

  it("keeps a project id that the call names itself", async () => {
    setWindowProjectId("/p/one");
    await invoke("a_command", { projectId: "/p/two" });
    expect(coreInvoke).toHaveBeenCalledWith("a_command", { projectId: "/p/two" }, undefined);
  });

  it("leaves a raw payload alone", async () => {
    setWindowProjectId("/p/one");
    const raw = new Uint8Array([1, 2]);
    await invoke("a_command", raw);
    expect(coreInvoke).toHaveBeenCalledWith("a_command", raw);
  });
});

describe("the order of state writes (CEX-FR-QZPN)", () => {
  beforeEach(async () => {
    const { resetInvokeOrderForTest } = await import("./invoke");
    resetInvokeOrderForTest();
    coreInvoke.mockReset();
    setPayloadProjectIdEnabled(false);
  });

  const flush = () => new Promise<void>((r) => setTimeout(r, 0));

  it("CEX-FR-QZPN: the calls of one state-writing command go out one at a time, in order", async () => {
    const releases: Array<() => void> = [];
    coreInvoke.mockImplementation(
      () => new Promise<void>((resolve) => releases.push(() => resolve())),
    );
    void invoke("save_changes_panel_state", { state: { mode: "branch" } });
    void invoke("save_changes_panel_state", { state: { mode: "uncommitted" } });
    expect(coreInvoke).toHaveBeenCalledTimes(1);
    releases[0]();
    await flush();
    expect(coreInvoke).toHaveBeenCalledTimes(2);
    expect(coreInvoke.mock.calls[1]).toEqual([
      "save_changes_panel_state",
      { state: { mode: "uncommitted" } },
    ]);
  });

  it("CEX-FR-QZPN: other commands, and reads, are not held behind a write", () => {
    coreInvoke.mockImplementation(() => new Promise<void>(() => {}));
    void invoke("save_layout", { layout: 1 });
    void invoke("save_app_preferences", { prefs: 1 });
    void invoke("set_terminal_label", { label: "a" });
    void invoke("set_terminal_label", { label: "b" });
    void invoke("list_project_windows");
    void invoke("list_project_windows");
    expect(coreInvoke).toHaveBeenCalledTimes(6);
  });

  it("CEX-FR-QZPN: a failed call does not stop the call after it", async () => {
    coreInvoke.mockRejectedValueOnce(new Error("refused"));
    coreInvoke.mockResolvedValue("ok");
    const first = invoke("save_layout", { n: 1 });
    const second = invoke("save_layout", { n: 2 });
    await expect(first).rejects.toThrow("refused");
    await expect(second).resolves.toBe("ok");
  });

  it("CEX-FR-QZPN: the order key names each save command and the shared project window list", async () => {
    const { orderKeyOf } = await import("./invoke");
    expect(orderKeyOf("save_x")).toBe("save_x");
    expect(orderKeyOf("set_x")).toBeNull();
    expect(orderKeyOf("record_x")).toBeNull();
    expect(orderKeyOf("list_x")).toBeNull();
    expect(orderKeyOf("get_set_x")).toBeNull();
    expect(orderKeyOf("activate_project_window")).toBe(orderKeyOf("set_project_window_attention"));
  });
});
