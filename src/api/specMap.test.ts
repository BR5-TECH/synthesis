/**
 * `"load specification map"` and the index it gives the map session
 * (`../../specifications/ui/SMP-specification-map.md` SMP-FR-HWIC,
 * SMP-FR-MNZW; `../../specifications/core/OSI-openspec-spec-index.md`
 * contract surface).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { loadSpecificationMap, type OpenSpecMapIndex } from "./specMap";
import { DEMO_SPECIFICATION_INDEX } from "../state/specMap/fixture";

function answer(value: unknown) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_specification_map") return value;
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

const wire: OpenSpecMapIndex = {
  levels: [
    { plural: "groups", singular: "group" },
    { plural: "capabilities", singular: "capability" },
    { plural: "requirements", singular: "requirement" },
  ],
  roots: [
    {
      id: "group:/",
      label: "specs",
      summary: "",
      children: [
        {
          id: "capability:auth",
          label: "Authentication",
          summary: "Sign in and out.",
          issues: [{ level: "WARNING", message: "short purpose" }],
          changes: [
            {
              change: "add-logout",
              location: { kind: "stream", streamId: "s-1", streamName: "editor-work" },
            },
          ],
          children: [
            {
              code: "auth#1",
              badge: "R1",
              path: "openspec/specs/auth/spec.md",
              label: "Login",
              summary: "Users SHALL log in.",
              requirements: 1,
              scenarios: 2,
              state: "gap",
            },
          ],
        },
        { id: "capability:ui", label: "ui", summary: "", planned: true, children: [] },
      ],
    },
  ],
  dependencies: [],
  validation: "unavailable",
  unreadLocations: 2,
};

beforeEach(() => {
  invokeMock.mockReset();
});

describe('"load specification map"', () => {
  it("SMP-FR-HWIC: the source demo gives a fresh copy of the built-in demo index, so an edit to one load never reaches the next", async () => {
    answer({ source: "demo" });
    const first = await loadSpecificationMap();
    expect(invokeMock).toHaveBeenCalledWith("load_specification_map");
    expect(first.source).toBe("demo");
    expect(first.roots.length).toBe(DEMO_SPECIFICATION_INDEX.roots.length);
    first.roots[0].label = "changed";
    const second = await loadSpecificationMap();
    expect(second.roots[0].label).toBe(DEMO_SPECIFICATION_INDEX.roots[0].label);
  });

  it("SMP-FR-MNZW: the source openspec gives its index with the badge, the issues, the chips and the validation fields", async () => {
    answer({ source: "openspec", index: wire });
    const index = await loadSpecificationMap();
    expect(index.source).toBe("openspec");
    expect(index.validation).toBe("unavailable");
    expect(index.unreadLocations).toBe(2);
    expect(index.levels.map((l) => l.plural)).toEqual(["groups", "capabilities", "requirements"]);
    const auth = index.roots[0].children[0];
    expect("code" in auth).toBe(false);
    if ("code" in auth) return;
    expect(auth.issues).toEqual([{ level: "WARNING", message: "short purpose" }]);
    expect(auth.changes?.[0].location).toEqual({
      kind: "stream",
      streamId: "s-1",
      streamName: "editor-work",
    });
    expect(auth.children[0]).toMatchObject({ code: "auth#1", badge: "R1", path: "openspec/specs/auth/spec.md" });
  });

  it("SMP-FR-MNZW, SMN-FR-VNTJ: a planned capability node is marked as one and holds no planned draft", async () => {
    answer({ source: "openspec", index: wire });
    const index = await loadSpecificationMap();
    const ui = index.roots[0].children[1];
    if ("code" in ui) throw new Error("expected an index node");
    expect(ui.plannedCapability).toBe(true);
    // The session keeps the drafts of SMD-FR-OYLC in `planned`.
    expect(ui.planned).toBeUndefined();
    expect(index.roots[0].children[0]).not.toHaveProperty("plannedCapability");
  });

  it("SMP-FR-ONSD: a failed load rejects with the code of the load", async () => {
    invokeMock.mockImplementation(async () => {
      throw "cli_unavailable";
    });
    await expect(loadSpecificationMap()).rejects.toBe("cli_unavailable");
  });
});
