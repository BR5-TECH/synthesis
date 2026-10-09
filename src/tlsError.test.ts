import { describe, expect, it } from "vitest";

import {
  parseTlsError,
  tlsErrorMessage,
  tlsFailureMessage,
  type TlsCause,
} from "./tlsError";
import { aiErrorMessage } from "./components/AiIntegrations";
import { tokenErrorMessage } from "./components/GithubTokens";
import { rejectionMessage } from "./components/Git/errors";
import { relayVerifyMessage } from "./components/RemoteConnectivitySettings";
import { githubPollingErrorMessage } from "./state/githubPolling";
import { turnFailureMessage, failedTurnDetail } from "./components/CommentRail/messages";
import type { AgentTurn } from "./types";

describe("the typed TLS error", () => {
  it("AAP-FR-PKWE: reads the cause and the host from the wire text", () => {
    expect(parseTlsError("tls_untrusted:unknown_issuer:api.openai.com")).toEqual({
      cause: "unknown_issuer",
      host: "api.openai.com",
    });
    expect(parseTlsError(new Error("tls_untrusted:expired:h.test"))).toEqual({
      cause: "expired",
      host: "h.test",
    });
    expect(parseTlsError("Error: tls_untrusted:other:h.test")).toEqual({
      cause: "other",
      host: "h.test",
    });
    expect(parseTlsError("tls_untrusted:other:::1")).toEqual({
      cause: "other",
      host: "::1",
    });
  });

  it.each([
    "unreachable",
    "tls_untrusted",
    "tls_untrusted:expired",
    "tls_untrusted:expired:",
    "tls_untrusted:bogus:h.test",
    "",
    undefined,
  ])("AAP-FR-HZTB: %j is no TLS error", (raw) => {
    expect(parseTlsError(raw)).toBeNull();
    expect(tlsErrorMessage(raw)).toBeNull();
  });

  it.each<[TlsCause, RegExp]>([
    ["unknown_issuer", /issuer.*unknown/],
    ["expired", /expired/],
    ["hostname_mismatch", /host name/],
    ["other", /check failed/],
  ])("AAP-FR-HZTB: the cause %s is worded and the host is named", (cause, words) => {
    const text = tlsFailureMessage({ host: "api.example.com", cause });
    expect(text).toContain("api.example.com");
    expect(text).toMatch(words);
  });

  it("AAP-FR-LRTC: every surface that shows a backend error words the same failure the same way", () => {
    const wire = "tls_untrusted:unknown_issuer:api.github.com";
    const expected = tlsFailureMessage({ host: "api.github.com", cause: "unknown_issuer" });
    expect(aiErrorMessage(wire)).toBe(expected);
    expect(tokenErrorMessage(wire)).toBe(expected);
    expect(rejectionMessage(wire)).toBe(expected);
    expect(relayVerifyMessage(wire)).toBe(expected);
    expect(githubPollingErrorMessage(wire)).toBe(expected);
    expect(githubPollingErrorMessage(new Error(wire))).toBe(expected);
  });
});

describe("a conversation turn that failed on a refused certificate", () => {
  const turn = (over: Partial<AgentTurn>): AgentTurn =>
    ({
      id: "turn-1",
      agentId: "a",
      nickname: "arch",
      origin: { kind: "artifact" },
      triggerCommentId: "c",
      state: "failed",
      failure: "tls_untrusted",
      tlsFailure: { host: "gateway.corp", cause: "expired" },
      retryPermitted: true,
      startedAt: "t",
      endedAt: "t",
      activeToolCalls: [],
      imagesOmitted: false,
      ...over,
    }) as unknown as AgentTurn;

  it("CTA-FR-EXVN: names the host and the cause", () => {
    const text = failedTurnDetail(turn({}));
    expect(text).toContain("gateway.corp");
    expect(text).toMatch(/expired/);
  });

  it("CTA-FR-EXVN: every other failure names nothing", () => {
    expect(failedTurnDetail(turn({ failure: "unreachable", tlsFailure: null }))).toBeNull();
    expect(failedTurnDetail(turn({ tlsFailure: null }))).toBeNull();
  });

  it("CTA-FR-IGNT: the bare failure code still has words", () => {
    expect(turnFailureMessage("tls_untrusted")).toMatch(/certificate/);
  });
});
