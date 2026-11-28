// The pure rules of the Codex tab: the offered efforts of
// `specifications/core/AIC-agentic-integrations.md` AIC-FR-SFMS and AIC-FR-TPKB
// as `specifications/ui/AII-ai-integrations.md` AII-FR-24 and AII-FR-BMGM render
// them, and the access token rules of AII-FR-NXAV, AII-FR-RNSO and AII-FR-LOYM.
import { describe, expect, it } from "vitest";

import {
  CODEX_ACCESS_TOKEN_MAX_BYTES,
  isValidCodexAccessToken,
  type AgenticIntegration,
} from "../types";
import { agenticIntegration, cliVerified, withLadder } from "../test/aiIntegrationsFixtures";
import { effectiveModel, offeredEfforts, shownEffort } from "./agenticEfforts";
import {
  accessTokenValidation,
  agenticStatus,
  canVerifyAgentic,
  cliConfigFor,
  draftForIntegration,
  emptyDraft,
} from "./agenticDraft";

const LEVELS = [
  { id: "low", label: "Low" },
  { id: "medium", label: "Medium" },
  { id: "high", label: "High" },
  { id: "xhigh", label: "Extra high" },
  { id: "max", label: "Max" },
  { id: "ultra", label: "Ultra" },
];

function codex(over: Partial<AgenticIntegration> = {}): AgenticIntegration {
  return agenticIntegration("codex", {
    codexAuthMode: "login_folder",
    reasoningEfforts: LEVELS,
    models: [
      withLadder("sol", ["low", "medium", "high", "xhigh", "max", "ultra"]),
      withLadder("luna", ["low", "medium", "high", "xhigh", "max"]),
      withLadder("old", ["low", "medium", "high", "xhigh"]),
    ],
    ...over,
  });
}

const ids = (levels: { id: string }[]) => levels.map((l) => l.id);

describe("the offered efforts (AIC-FR-SFMS)", () => {
  it("AII-FR-24, AII-FR-BMGM: a model with a ladder offers that ladder, labelled from the vendor list", () => {
    const record = codex();
    expect(offeredEfforts(record, "luna")).toEqual(LEVELS.slice(0, 5));
    expect(ids(offeredEfforts(record, "sol"))).toContain("ultra");
  });

  it("AII-FR-24: with no effective model, the levels every ladder shares", () => {
    expect(ids(offeredEfforts(codex(), null))).toEqual(["low", "medium", "high", "xhigh"]);
  });

  it("AII-FR-24: a model id the record does not list is treated as no model", () => {
    expect(ids(offeredEfforts(codex(), "gone"))).toEqual(["low", "medium", "high", "xhigh"]);
  });

  it("AII-FR-24, AII-FR-BMGM: a listed model with no ladder, beside models with one, offers the vendor list", () => {
    // The same fixture as `agentic/tests/effort_ladders.rs`, so both sides agree.
    const record = codex({
      models: [
        withLadder("sol", ["low", "medium", "high", "xhigh", "max", "ultra"]),
        withLadder("luna", ["low", "medium", "high", "xhigh", "max"]),
        { id: "bare", label: "bare" },
      ],
    });
    expect(offeredEfforts(record, "bare")).toEqual(LEVELS);
    expect(ids(offeredEfforts(record, null))).toEqual(["low", "medium", "high", "xhigh", "max"]);
  });

  it("AII-FR-24: with no ladder at all, the vendor list unchanged (Claude Code)", () => {
    const claude = cliVerified("claude_code", "/bin/claude", { selectedModel: "opus" });
    expect(offeredEfforts(claude, "opus")).toEqual(claude.reasoningEfforts);
    expect(offeredEfforts(claude, null)).toEqual(claude.reasoningEfforts);
  });

  it("AII-FR-24: a level the vendor list does not name falls back to its id", () => {
    const record = codex({ models: [withLadder("new", ["low", "turbo"])] });
    expect(offeredEfforts(record, "new")).toEqual([
      { id: "low", label: "Low" },
      { id: "turbo", label: "turbo" },
    ]);
  });

  it("AII-FR-BMGM: the effective model of a row is its own model, else the default", () => {
    const record = codex({ selectedModel: "sol", modelOverrides: { review: "old" } });
    expect(effectiveModel(record, null)).toBe("sol");
    expect(effectiveModel(record, "work")).toBe("sol");
    expect(effectiveModel(record, "review")).toBe("old");
    expect(effectiveModel(codex(), "work")).toBeNull();
  });

  it("AII-FR-BMGM: a stored level the list does not offer shows as the default entry", () => {
    const offered = offeredEfforts(codex(), "old");
    expect(shownEffort("ultra", offered)).toBe("");
    expect(shownEffort("high", offered)).toBe("high");
    expect(shownEffort(null, offered)).toBe("");
    expect(shownEffort(undefined, offered)).toBe("");
  });
});

describe("the access token rules (AII-FR-NXAV)", () => {
  it("AII-FR-NXAV: trimmed, non-empty, no whitespace or control character, at most 16384 bytes", () => {
    expect(isValidCodexAccessToken("  tok-123  ")).toBe(true);
    expect(isValidCodexAccessToken("   ")).toBe(false);
    expect(isValidCodexAccessToken("a b")).toBe(false);
    expect(isValidCodexAccessToken("a\tb")).toBe(false);
    expect(isValidCodexAccessToken("a\u0007b")).toBe(false);
    expect(isValidCodexAccessToken("a\u0085b")).toBe(false);
    expect(isValidCodexAccessToken("a b")).toBe(false);
    expect(isValidCodexAccessToken("x".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES))).toBe(true);
    expect(isValidCodexAccessToken("x".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES + 1))).toBe(false);
    // The limit counts bytes, not characters: "é" is two bytes in UTF-8.
    expect(isValidCodexAccessToken("é".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES / 2))).toBe(true);
    expect(isValidCodexAccessToken("é".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES / 2 + 1))).toBe(false);
    // The same set the backend judges by (AIC-FR-BXFL): U+FEFF is trimmed like a
    // space and refused inside a token; U+0085 is a control character, never trimmed.
    expect(isValidCodexAccessToken("\uFEFF\u00A0tok-123\u2028")).toBe(true);
    expect(isValidCodexAccessToken("a\uFEFFb")).toBe(false);
    expect(isValidCodexAccessToken("\u0085tok-123")).toBe(false);
    expect(isValidCodexAccessToken("  " + "b".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES))).toBe(true);
  });

  it("AII-FR-NXAV: the inline message names the rule, never the value, and says nothing for an empty field", () => {
    expect(accessTokenValidation("")).toBe("");
    expect(accessTokenValidation("  ")).toBe("");
    expect(accessTokenValidation("good-token")).toBe("");
    const spaced = accessTokenValidation("secret part");
    expect(spaced).toMatch(/no spaces/);
    expect(spaced).not.toContain("secret");
    expect(accessTokenValidation("y".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES + 1))).toMatch(
      /at most 16384 bytes/,
    );
  });

  it("AII-FR-NXAV: Verify needs a valid typed token or a stored one; Login folder needs neither", () => {
    const login = { ...emptyDraft(), path: "/c" };
    const token = { ...login, codexAuthMode: "access_token" as const };
    expect(canVerifyAgentic(codex(), login)).toBe(true);
    expect(canVerifyAgentic(codex(), token)).toBe(false);
    expect(canVerifyAgentic(codex({ keyState: "set" }), token)).toBe(true);
    expect(canVerifyAgentic(codex(), { ...token, accessToken: "tok" })).toBe(true);
    expect(canVerifyAgentic(codex({ keyState: "set" }), { ...token, accessToken: "a b" })).toBe(
      false,
    );
  });

  it("AII-FR-RNSO, AII-FR-NXAV, AII-FR-LOYM: the payload carries the open sub-tab's fields and no other's", () => {
    const draft = { ...emptyDraft(), path: "/c", accessToken: " tok ", oauthToken: "x" };
    expect(cliConfigFor(codex(), draft)).toEqual({ path: "/c", authMode: "login_folder" });
    expect(cliConfigFor(codex(), { ...draft, codexAuthMode: "access_token" })).toEqual({
      path: "/c",
      authMode: "access_token",
      accessToken: "tok",
    });
    expect(
      cliConfigFor(codex(), { ...draft, codexAuthMode: "access_token", accessToken: "" }),
    ).toEqual({ path: "/c", authMode: "access_token" });
  });

  it("AII-FR-LOYM, AII-FR-22: the draft opens on the record's mode, and a change of mode or a typed token is an edit", () => {
    const stored = cliVerified("codex", "/c", {
      codexAuthMode: "access_token",
      keyState: "set",
      maskedHint: "Q7xk",
    });
    const draft = draftForIntegration(stored);
    expect(draft.codexAuthMode).toBe("access_token");
    expect(draft.accessToken).toBe("");
    expect(agenticStatus(stored, draft, "idle", "", false).tone).toBe("ok");
    expect(
      agenticStatus(stored, { ...draft, codexAuthMode: "login_folder" }, "idle", "", false).text,
    ).toMatch(/Not verified/);
    expect(agenticStatus(stored, { ...draft, accessToken: "t" }, "idle", "", false).text).toMatch(
      /Not verified/,
    );
    // AIC-FR-UFYH: a record with no stored mode reads as login_folder.
    expect(draftForIntegration(cliVerified("codex", "/c")).codexAuthMode).toBe("login_folder");
  });
});
