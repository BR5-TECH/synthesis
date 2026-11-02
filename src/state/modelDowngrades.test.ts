import { beforeEach, describe, expect, it, vi } from "vitest";

import { flushLogs, resetLogBufferForTest } from "../logging";
import type { ModelDowngrade, ModelDowngradeSuppression } from "../types";
import {
  downgradeKey,
  downgradeTarget,
  modelDowngradeRaise,
  isMuted,
  SUPPRESS_FAILED_KEY,
} from "./modelDowngrades";
import { mintAddress } from "./notificationAddress";
import {
  configureNotifications,
  notifyArrived,
  raiseNotification,
  resetNotifications,
  setNotificationPermissionGranted,
  setNotificationsEnabled,
  type WindowSnapshot,
} from "./notifications";
import { indicatedAddresses, resetTabIndications } from "./tabIndications";
import { showToast, visibleToasts } from "./toasts";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** What the shell does with one event: decide, then raise. */
async function handleModelDowngrade(
  payload: ModelDowngrade,
  root: { projectKey: string | null; worktree: string | null },
  now: () => number,
) {
  const raise = await modelDowngradeRaise(payload, () => root, now);
  if (raise !== null) await raiseNotification(raise);
}

const PROJECT = "/dev/acme";
const WORKTREE = "/dev/acme";
const ROOT = { projectKey: PROJECT, worktree: WORKTREE };
const NOW = Date.parse("2026-11-02T12:00:00Z");

const event = (over: Partial<ModelDowngrade> = {}): ModelDowngrade => ({
  source: "agent",
  requestedModel: "claude-opus-5-5",
  servedModel: "claude-opus-4-8",
  provider: "anthropic",
  projectKey: PROJECT,
  worktree: WORKTREE,
  origin: { discussionId: "d1", target: { kind: "draft", draftId: "dr1" } },
  runId: null,
  ...over,
});

const draftAddress = mintAddress(PROJECT, WORKTREE, { kind: "draft", draftId: "dr1" });

const base: WindowSnapshot = {
  focused: true,
  mainFocused: true,
  projectKey: PROJECT,
  worktree: WORKTREE,
  activeTarget: null,
  visiblePanelSurface: null,
  visibleBottomSurface: null,
  visibleRun: null,
  settingsWindowOpen: false,
};

let rules: ModelDowngradeSuppression[] | Error = [];

function useWindow(snapshot: WindowSnapshot, resolveTab = () => null as { active: boolean } | null) {
  configureNotifications({ snapshot: () => snapshot, resolveTab });
}

const posts = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "post_notification")
    .map((c) => (c[1] as { request: Record<string, unknown> }).request);

/** The fields of every `model downgrade` record written so far. */
function downgradeRecords(): Array<{ level: string; domains: string[]; fields: Record<string, unknown> }> {
  flushLogs();
  return invokeMock.mock.calls
    .filter((c) => c[0] === "append_log_records")
    .flatMap((c) => (c[1] as { records: Array<Record<string, unknown>> }).records)
    .filter((r) => r.message === "model downgrade") as never;
}

beforeEach(() => {
  resetLogBufferForTest();
  resetNotifications();
  resetTabIndications();
  setNotificationsEnabled(true);
  setNotificationPermissionGranted(true);
  rules = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "append_log_records":
        return undefined;
      case "load_model_downgrade_suppressions":
        if (rules instanceof Error) throw rules;
        return rules;
      case "post_notification":
        return { id: "n-1" };
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
  useWindow(base);
});

describe("the model downgrade raise", () => {
  it("NTF-FR-AGTB, NTF-FR-OPCD: a downgrade shows one Warn toast that names both models", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    const [toast] = visibleToasts();
    expect(toast).toMatchObject({
      key: "model-downgrade:claude-opus-5-5→claude-opus-4-8",
      level: "Warn",
      title: "Model changed",
      body: "Requested claude-opus-5-5, served claude-opus-4-8.",
      address: draftAddress,
      dismissLabel: "Acknowledge",
    });
    expect(toast.menu?.items.map((i) => i.label)).toEqual(["1 hour", "1 day", "1 week", "Forever"]);
    expect(posts()).toHaveLength(0);
  });

  it("NTF-FR-FPBH, NTF-FR-08: the toast shows while the author looks at the draft of the conversation", async () => {
    useWindow({ ...base, activeTarget: { kind: "draft", draftId: "dr1" } });
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(1);
  });

  it("NTF-FR-FPBH, NTF-FR-ZSIK: reaching the target does not remove the toast, as it removes an ordinary one", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    showToast({ key: "other", level: "Warn", title: "t", body: "", address: draftAddress });
    notifyArrived(draftAddress);
    expect(visibleToasts().map((t) => t.key)).toEqual([
      downgradeKey("claude-opus-5-5", "claude-opus-4-8"),
    ]);
  });

  it("NTF-FR-HZNF, NTF-FR-AGTB: a second downgrade of the same pair replaces the toast in place", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    await handleModelDowngrade(
      event({ origin: { discussionId: "d2", target: { kind: "artifact", artifactId: "specs/a.md" } } }),
      ROOT,
      () => NOW,
    );
    const shown = visibleToasts();
    expect(shown).toHaveLength(1);
    expect(shown[0].revision).toBe(1);
    expect(shown[0].address).toBe(mintAddress(PROJECT, WORKTREE, { kind: "file", path: "specs/a.md" }));
  });

  it("NTF-FR-AGTB: two pairs show two toasts", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    await handleModelDowngrade(event({ servedModel: "claude-sonnet-5-5" }), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(2);
  });

  it("NTF-FR-WMBD, NTF-FR-UGJO: with no window focused the raise posts one OS notification with no action and shows no toast", async () => {
    useWindow({ ...base, focused: false, mainFocused: false });
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(0);
    const [post] = posts();
    expect(post).toEqual({
      key: "model-downgrade:claude-opus-5-5→claude-opus-4-8",
      title: "Model changed",
      subtitle: "acme",
      body: "Requested claude-opus-5-5, served claude-opus-4-8.",
      payload: draftAddress,
    });
  });

  it("NTF-FR-PCVX, GLS-FR-25, NTF-FR-DYHQ: with the switch off and the window unfocused nothing posts, and the record is still written", async () => {
    setNotificationsEnabled(false);
    useWindow({ ...base, focused: false, mainFocused: false });
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(posts()).toHaveLength(0);
    expect(downgradeRecords()).toHaveLength(1);
  });

  it("NTF-FR-CXWO: the address is the draft, the artifact file, the Notes panel, or the run", () => {
    expect(downgradeTarget(event())).toEqual({ kind: "draft", draftId: "dr1" });
    expect(
      downgradeTarget(event({ origin: { discussionId: "d", target: { kind: "artifact", artifactId: "a/b.md" } } })),
    ).toEqual({ kind: "file", path: "a/b.md" });
    expect(
      downgradeTarget(event({ origin: { discussionId: "d", target: { kind: "note", noteId: "n1" } } })),
    ).toEqual({ kind: "panel", surface: "notes" });
    expect(downgradeTarget(event({ source: "graduation", origin: null, runId: "r1" }))).toEqual({
      kind: "run",
      runId: "r1",
    });
  });

  it("GRU-FR-HMPM: a graduation downgrade raises a Warn toast addressed to its run", async () => {
    await handleModelDowngrade(event({ source: "graduation", origin: null, runId: "r1" }), ROOT, () => NOW);
    expect(visibleToasts()[0]).toMatchObject({
      level: "Warn",
      address: mintAddress(PROJECT, WORKTREE, { kind: "run", runId: "r1" }),
    });
  });

  it("NTF-FR-CXWO, NTF-FR-DYHQ: an event while no project is open raises nothing, reads no rule, and still writes the record", async () => {
    await handleModelDowngrade(event(), { projectKey: null, worktree: null }, () => NOW);
    expect(visibleToasts()).toHaveLength(0);
    expect(downgradeRecords()).toHaveLength(1);
    expect(invokeMock.mock.calls.some((c) => c[0] === "load_model_downgrade_suppressions")).toBe(false);
  });

  it("NTF-FR-CXWO: an event of a project that is not open raises nothing", async () => {
    await handleModelDowngrade(event({ projectKey: "/dev/other" }), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NTF-FR-CXWO, NTF-FR-15: a project closed while the rules are read raises nothing", async () => {
    let root: { projectKey: string | null; worktree: string | null } = ROOT;
    let release: (value: ModelDowngradeSuppression[]) => void = () => {};
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "load_model_downgrade_suppressions"
        ? new Promise((resolve) => (release = resolve))
        : Promise.resolve(undefined),
    );
    const pending = modelDowngradeRaise(event(), () => root, () => NOW);
    await vi.waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "load_model_downgrade_suppressions")).toBe(true),
    );
    root = { projectKey: null, worktree: null };
    release([]);
    expect(await pending).toBeNull();
  });
});

describe("the suppression rules (NTF-FR-CYTU)", () => {
  const rule = (expiresAt: string | null, over: Partial<ModelDowngradeSuppression> = {}) => ({
    requestedModel: "claude-opus-5-5",
    servedModel: "claude-opus-4-8",
    expiresAt,
    ...over,
  });

  it("NTF-FR-CYTU: a pair muted by a rule in force raises no toast, no OS notification, and no tab indication", async () => {
    rules = [rule("2026-11-02T13:00:00Z")];
    useWindow(base, () => ({ active: false }));
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(0);
    expect(indicatedAddresses()).toEqual([]);

    useWindow({ ...base, focused: false, mainFocused: false });
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(posts()).toHaveLength(0);
  });

  it("NTF-FR-CYTU: a Forever rule mutes the pair", async () => {
    rules = [rule(null)];
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NTF-FR-CYTU: an expired rule mutes nothing", async () => {
    rules = [rule("2026-11-02T12:00:00Z")];
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(1);
    expect(isMuted([rule("2026-11-02T12:00:01Z")], "claude-opus-5-5", "claude-opus-4-8", NOW)).toBe(true);
  });

  it("NTF-FR-CYTU, GSS-FR-HECI: a rule mutes only its own pair, compared as exact strings", async () => {
    rules = [rule(null, { servedModel: "Claude-Opus-4-8" }), rule(null, { requestedModel: "claude-opus-5" })];
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(1);
  });

  it("NTF-FR-CYTU: a read that fails mutes nothing", async () => {
    rules = new Error("no_project_open");
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(visibleToasts()).toHaveLength(1);
  });
});

describe("the log record (NTF-FR-DYHQ)", () => {
  it("NTF-FR-DYHQ: each downgrade writes one WARN record in the ai domain, with suppressed false", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    expect(downgradeRecords()).toEqual([
      expect.objectContaining({
        level: "WARN",
        domains: ["ai"],
        fields: {
          requestedModel: "claude-opus-5-5",
          servedModel: "claude-opus-4-8",
          provider: "anthropic",
          source: "agent",
          suppressed: false,
        },
      }),
    ]);
  });

  it("NTF-FR-DYHQ: a muted pair still writes the record, with suppressed true", async () => {
    rules = [{ requestedModel: "claude-opus-5-5", servedModel: "claude-opus-4-8", expiresAt: null }];
    await handleModelDowngrade(event({ source: "graduation", origin: null, runId: "r1" }), ROOT, () => NOW);
    expect(downgradeRecords()).toEqual([
      expect.objectContaining({ fields: expect.objectContaining({ source: "graduation", suppressed: true }) }),
    ]);
  });
});

describe("the Suppress menu (NTF-FR-LHVU)", () => {
  it("NTF-FR-LHVU: a pick saves the rule for the pair and the duration", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    invokeMock.mockImplementation(async (cmd: string) => (cmd === "save_model_downgrade_suppression" ? {} : undefined));
    await visibleToasts()[0].menu!.onPick("week");
    expect(invokeMock).toHaveBeenCalledWith("save_model_downgrade_suppression", {
      requestedModel: "claude-opus-5-5",
      servedModel: "claude-opus-4-8",
      duration: "week",
    });
  });

  it("NTF-FR-LHVU: a failed save rejects and shows an Error toast", async () => {
    await handleModelDowngrade(event(), ROOT, () => NOW);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_model_downgrade_suppression") throw new Error("disk full");
      return undefined;
    });
    await expect(visibleToasts()[0].menu!.onPick("day")).rejects.toThrow("disk full");
    expect(visibleToasts().map((t) => [t.key, t.level])).toEqual([
      [SUPPRESS_FAILED_KEY, "Error"],
      [downgradeKey("claude-opus-5-5", "claude-opus-4-8"), "Warn"],
    ]);
    expect(visibleToasts()[0]).toMatchObject({ title: "Suppression not saved", body: "disk full", address: null });
  });
});
