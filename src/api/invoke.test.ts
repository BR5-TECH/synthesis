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
