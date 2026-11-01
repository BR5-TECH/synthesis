/**
 * A file written on the author's behalf while its session holds unsaved edits
 * (`PCR-prompt-change-review.md` PCR-FR-LAGZ, `DPC-decide-prompt-changes-tool.md`
 * DPC-FR-TVKX, `EXC-editor-external-change.md` EXC-FR-QCNO).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { EditSessionStore } from "./editSessions";
import { deferred } from "../test/voiceVadKit";

const ID = "prompts/review.md";

let read: ReturnType<typeof deferred<{ body: string; checksum: string }>>;

beforeEach(() => {
  invokeMock.mockReset();
  read = deferred<{ body: string; checksum: string }>();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") return read.promise;
    if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
    throw new Error(`unexpected invoke ${cmd}`);
  });
});

function dirtyStore(): EditSessionStore {
  const store = new EditSessionStore();
  store.ensure(ID);
  store.update(ID, { buffer: "typed", baseline: "ck-1", dirty: true, loaded: true });
  return store;
}

const saves = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "save_artifact_contents");

describe("a file written elsewhere over unsaved edits", () => {
  it("PCR-FR-LAGZ, EXC-FR-QCNO: the modal rises and the buffer stays unwritten", async () => {
    const store = dirtyStore();
    const done = store.raiseWrittenElsewhere(ID);
    read.resolve({ body: "accepted", checksum: "ck-2" });
    await done;
    expect(store.get(ID)).toMatchObject({ conflict: true, dirty: true, buffer: "typed" });
    expect(await store.flush(ID)).toEqual({ ok: false, blocked: "conflict", artifactId: ID });
    expect(saves()).toHaveLength(0);
  });

  it("PCR-FR-LAGZ: a read that fails still holds the buffer behind the modal", async () => {
    const store = dirtyStore();
    const done = store.raiseWrittenElsewhere(ID);
    read.reject("artifact_not_found");
    await done;
    expect(store.get(ID)?.conflict).toBe(true);
    expect(await store.flush(ID)).toMatchObject({ ok: false, blocked: "conflict" });
    expect(saves()).toHaveLength(0);
  });

  it("PCR-FR-AXPW: a quiesce of an acceptance that comes during the read stays", async () => {
    const store = dirtyStore();
    const done = store.raiseWrittenElsewhere(ID);
    // A chip acceptance starts while the read is out.
    await store.quiesce([ID]);
    read.resolve({ body: "accepted", checksum: "ck-2" });
    await done;
    expect(store.get(ID)?.quiesced).toBe(true);
  });

  it("PCR-FR-LAGZ: a session that is quiesced already is left to its owner", async () => {
    const store = dirtyStore();
    await store.quiesce([ID]);
    await store.raiseWrittenElsewhere(ID);
    expect(invokeMock).not.toHaveBeenCalledWith("load_artifact_contents_by_id", expect.anything());
    expect(store.get(ID)).toMatchObject({ quiesced: true, conflict: false });
  });
});
