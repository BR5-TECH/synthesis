/**
 * The draft creation of a Flow task (`../../../specifications/ui/FLO-flow.md`
 * FLO-FR-NRBV), through the shell's draft actions.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("../../logging", async (original) => ({
  ...(await original<typeof import("../../logging")>()),
  logWarn: vi.fn(),
}));

import { logWarn } from "../../logging";
import { DraftSessionStore } from "../../state/draftSessions";
import {
  currentMainBranchWrite,
  NOT_MAIN_GUARD,
  setBranchWriteGuardReadsForTest,
  settleMainBranchWrite,
} from "../../state/mainBranchWrite";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { BranchWriteGuard } from "../../types";
import { createDraftTabActions } from "./draftActions";

/** The actions over a strip that a test can watch. */
function actions() {
  const deps = {
    drafts: new DraftSessionStore(),
    tabsRef: { current: [] },
    setTabs: vi.fn(),
    setActiveTab: vi.fn(),
    activateTab: vi.fn(),
    bumpDrafts: vi.fn(),
    specMap: {} as SpecMapSessionStore,
  };
  return { deps, ...createDraftTabActions(deps) };
}

let createAnswer: () => Promise<unknown>;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  vi.mocked(logWarn).mockClear();
  createAnswer = () =>
    Promise.resolve({
      draft: { id: "d-9", name: "Tokenize", promptPath: "Tokenize.md", status: "active" },
      file: "Tokenize.md",
    });
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "create_draft") return createAnswer();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});

describe("createNamedDraft (FLO-FR-NRBV)", () => {
  it("FLO-FR-NRBV: a created draft is returned with its id and name, and no tab opens", async () => {
    const { deps, createNamedDraft } = actions();

    await expect(createNamedDraft("Tokenize")).resolves.toEqual({
      ok: true,
      draft: { id: "d-9", name: "Tokenize" },
    });
    expect(calls("create_draft")).toEqual([["create_draft", { name: "Tokenize", folder: null }]]);
    expect(deps.setTabs).not.toHaveBeenCalled();
    expect(deps.activateTab).not.toHaveBeenCalled();
    expect(deps.bumpDrafts).toHaveBeenCalledTimes(1);
  });

  it("FLO-FR-NRBV, MBW-FR-RMCO: a main branch write the author cancels creates nothing and returns no error", async () => {
    const main: BranchWriteGuard = { ...NOT_MAIN_GUARD, branch: "main", isMain: true, isDefault: true };
    setBranchWriteGuardReadsForTest({
      read: () => Promise.resolve(main),
      confirm: () => Promise.resolve({ ...main, confirmed: true }),
    });
    const { deps, createNamedDraft } = actions();

    const outcome = createNamedDraft("Tokenize");
    await vi.waitFor(() => expect(currentMainBranchWrite()).not.toBeNull());
    settleMainBranchWrite(false);

    await expect(outcome).resolves.toEqual({ ok: false, error: null });
    expect(calls("create_draft")).toEqual([]);
    expect(deps.setTabs).not.toHaveBeenCalled();
  });

  it("FLO-FR-NRBV: a refused creation returns the refusal's text, and logs its code alone", async () => {
    createAnswer = () =>
      Promise.reject({ code: "draft_name_taken", message: "A draft named Tokenize exists." });
    const { deps, createNamedDraft } = actions();

    await expect(createNamedDraft("Tokenize")).resolves.toEqual({
      ok: false,
      error: "A draft named Tokenize exists.",
    });
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "draft creation for a Flow item failed", {
      code: "draft_name_taken",
    });
    expect(deps.setTabs).not.toHaveBeenCalled();
  });
});
