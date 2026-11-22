// The pure rules of the Terminals panel (`TRM-terminals.md`).
import { describe, expect, it } from "vitest";
import { session } from "../components/Terminals/terminalFixtures";
import { closeSet, groupTerminals, kindName, placeName, runningCount, sortRows, stateName } from "./terminalGroups";

const wt = (path: string) => ({ kind: "worktree" as const, path });
const stream = (streamId: string) => ({ kind: "work_stream" as const, streamId });

describe("terminal groups", () => {
  it("TRM-FR-ZQPC: pinned rows come first, each part in creation order", () => {
    const a = session({ id: "a" });
    const b = session({ id: "b", pinned: true });
    const c = session({ id: "c" });
    const d = session({ id: "d", pinned: true });
    expect(sortRows([a, b, c, d]).map((s) => s.id)).toEqual(["b", "d", "a", "c"]);
  });

  it("TRM-FR-EAUY: the active worktree group is first, then groups by their earliest session", () => {
    const s1 = session({ id: "s1", target: stream("st"), targetName: "auth" });
    const o1 = session({ id: "o1", target: wt("/other"), targetName: "other" });
    const a1 = session({ id: "a1", target: wt("/repo") });
    const s2 = session({ id: "s2", target: stream("st"), targetName: "auth" });
    const groups = groupTerminals([s1, o1, a1, s2], "/repo");
    expect(groups.map((g) => g.name)).toEqual(["repo", "auth", "other"]);
    expect(groups.map((g) => g.role)).toEqual(["active", "stream", "worktree"]);
    expect(groups[1].sessions.map((s) => s.id)).toEqual(["s1", "s2"]);
  });

  it("TRM-FR-EAUY: with no active worktree known, groups keep creation order", () => {
    const o1 = session({ id: "o1", target: wt("/other"), targetName: "other" });
    const a1 = session({ id: "a1", target: wt("/repo") });
    expect(groupTerminals([o1, a1], null).map((g) => g.name)).toEqual(["other", "repo"]);
  });

  it("TRM-FR-MANC: Close terminal ends the clicked row, pinned or not", () => {
    const p = session({ id: "p", pinned: true });
    expect(closeSet([p, session({ id: "x" })], "one", "p").map((s) => s.id)).toEqual(["p"]);
  });

  it("TRM-FR-BHYF: the mass closes skip pinned rows in every group", () => {
    const list = [
      session({ id: "a", pinned: true }),
      session({ id: "b" }),
      session({ id: "c", target: stream("st") }),
      session({ id: "d", target: wt("/other"), pinned: true }),
    ];
    expect(closeSet(list, "others", "b").map((s) => s.id)).toEqual(["c"]);
    expect(closeSet(list, "all", "b").map((s) => s.id)).toEqual(["b", "c"]);
    expect(closeSet(list, "others", "a").map((s) => s.id)).toEqual(["b", "c"]);
  });

  it("TRM-FR-JJEY: the running count holds starting and running sessions only", () => {
    const list = [
      session({ state: "running" }),
      session({ state: "starting" }),
      session({ state: "exited" }),
      session({ state: "target_gone" }),
    ];
    expect(runningCount(list)).toBe(2);
  });

  it("TRM-FR-BEYN: the row names kind, place and state", () => {
    expect(kindName("agent_cli", "claude_code")).toBe("Agent CLI · Claude Code");
    expect(kindName("microsandbox_shell")).toBe("Microsandbox shell");
    expect(placeName("docker", "syn-1")).toBe("docker · syn-1");
    expect(placeName("microsandbox", "vm-a")).toBe("vm · vm-a");
    expect(placeName("host")).toBe("host");
    expect(stateName(session({ state: "exited", exitCode: 2 }))).toBe("exited 2");
    expect(stateName(session({ state: "target_gone" }))).toBe("target gone");
    expect(stateName(session({ state: "starting" }))).toBe("starting");
  });
});
