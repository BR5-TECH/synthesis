import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AgenticAiIntegrations } from "./AiIntegrations";
import { agenticStatus, cliConfigFor, claudeDraftEdited, draftForIntegration } from "./agenticDraft";
import { resetLogBufferForTest } from "../logging";
import {
  AGENTIC_ALL,
  agenticIntegration,
  agenticLevel,
  backendFor,
  cliVerified,
} from "../test/aiIntegrationsFixtures";

// The section reaches the backend only through `invoke`; mock the bridge so
// jsdom never needs the Tauri runtime.
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
});

afterEach(cleanup);

const GATEWAY_TOKEN = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL = "https://llm-gateway.example.com/bedrock";

/** A Claude Code record that last verified in gateway mode. */
function gatewayVerified(over: Record<string, unknown> = {}) {
  return cliVerified("claude_code", "/usr/bin/claude", {
    authMode: "custom_gateway",
    gatewayApi: "anthropic",
    gatewayBaseUrl: GATEWAY_URL,
    gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
    gatewayKeyState: "set",
    gatewayMaskedHint: "k2Qz",
    envVars: [],
    modelsOrigin: "probed",
    models: [{ id: "claude-a", label: "Claude A" }],
    ...over,
  });
}

const bedrockVerified = () => gatewayVerified({ gatewayApi: "bedrock", modelsOrigin: "catalog" });

function withClaude(claude: ReturnType<typeof cliVerified>) {
  return [claude, ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v))];
}

const apiButton = (api: "anthropic" | "bedrock") => agenticLevel().getByTestId(`agentic-gateway-api-${api}`);
const gatewayUrl = () => agenticLevel().getByTestId("agentic-gateway-url");
const gatewayToken = () => agenticLevel().getByTestId("agentic-gateway-token");
const verifyButton = () => agenticLevel().getByRole("button", { name: /^Verify/ });
const status = () => agenticLevel().getByTestId("agentic-status");

/** The configs `verify_agentic_integration` was invoked with, in order. */
const verifyConfigs = () =>
  invokeMock.mock.calls
    .filter(([cmd]) => cmd === "verify_agentic_integration")
    .map(([, args]) => (args as { config: Record<string, unknown> }).config);

async function openLevel() {
  render(<AgenticAiIntegrations />);
  const user = userEvent.setup();
  await agenticLevel().findByRole("tab", { name: /Claude Code/ });
  await agenticLevel().findByTestId("agentic-status");
  return user;
}

describe("the gateway API choice (AII-FR-PHFX, AII-FR-DKDC, AII-FR-EJMG)", () => {
  it("AII-FR-PHFX: the API row comes first, offers Anthropic then Bedrock, and a fresh tab presses Anthropic", async () => {
    backend();
    const user = await openLevel();
    await user.click(agenticLevel().getByRole("tab", { name: /Custom Gateway/ }));

    const group = agenticLevel().getByRole("group", { name: "API" });
    const buttons = Array.from(group.querySelectorAll("button")).map((b) => b.textContent);
    expect(buttons).toEqual(["Anthropic", "Bedrock"]);
    expect(apiButton("anthropic")).toHaveAttribute("aria-pressed", "true");
    expect(apiButton("bedrock")).toHaveAttribute("aria-pressed", "false");
    expect(group.compareDocumentPosition(gatewayUrl()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(gatewayUrl().compareDocumentPosition(gatewayToken()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("AII-FR-PHFX: a stored Bedrock gateway opens with Bedrock pressed and the Bedrock status line", async () => {
    backend({ agentic: withClaude(bedrockVerified()) });
    await openLevel();
    expect(apiButton("bedrock")).toHaveAttribute("aria-pressed", "true");
    expect(apiButton("anthropic")).toHaveAttribute("aria-pressed", "false");
    expect(status()).toHaveTextContent(/^Bedrock gateway · not checked · .* · verified · 2\.1\.4$/);
  });

  it("AII-FR-PHFX, AII-FR-22: choosing another API is an edit that invokes nothing, and choosing back undoes it", async () => {
    backend({ agentic: withClaude(gatewayVerified()) });
    const user = await openLevel();
    const before = invokeMock.mock.calls.filter(([cmd]) => cmd !== "append_log_records").length;
    expect(status()).toHaveTextContent(/gateway answered/);

    await user.click(apiButton("bedrock"));
    expect(apiButton("bedrock")).toHaveAttribute("aria-pressed", "true");
    expect(status()).toHaveTextContent(/Not verified/);

    await user.click(apiButton("anthropic"));
    expect(status()).toHaveTextContent(/gateway answered/);
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd !== "append_log_records")).toHaveLength(before);
  });

  it("AII-FR-DKDC: Verify sends the chosen API, shows the Bedrock line, and clears the typed token", async () => {
    backend(
      { agentic: withClaude(gatewayVerified()) },
      { verify_agentic_integration: () => bedrockVerified() },
    );
    const user = await openLevel();
    await user.click(apiButton("bedrock"));
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    await user.click(verifyButton());

    await waitFor(() => expect(status()).toHaveTextContent(/Bedrock gateway · not checked/));
    expect(verifyConfigs()).toEqual([
      {
        path: "/usr/bin/claude",
        authMode: "custom_gateway",
        gatewayApi: "bedrock",
        gatewayBaseUrl: GATEWAY_URL,
        gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
        gatewayToken: GATEWAY_TOKEN,
      },
    ]);
    expect(gatewayToken()).toHaveValue("");
  });

  it("AII-FR-EJMG: the Bedrock hint names the optional region and model entries only while Bedrock is chosen", async () => {
    backend({ agentic: withClaude(gatewayVerified()) });
    const user = await openLevel();
    const note = () => agenticLevel().getByTestId("agentic-env-note");
    expect(note().textContent).not.toMatch(/AWS_REGION/);

    await user.click(apiButton("bedrock"));
    for (const name of [
      "AWS_REGION",
      "ANTHROPIC_DEFAULT_OPUS_MODEL",
      "ANTHROPIC_DEFAULT_SONNET_MODEL",
      "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    ]) {
      expect(note()).toHaveTextContent(name);
    }

    // A wrong entry is named in place of the hint.
    fireEvent.change(agenticLevel().getByTestId("agentic-env-vars"), { target: { value: "no equals sign" } });
    expect(note()).toHaveTextContent("Line 1 is not written as NAME=value.");
    expect(note().textContent).not.toMatch(/AWS_REGION/);
    fireEvent.change(agenticLevel().getByTestId("agentic-env-vars"), { target: { value: "" } });

    await user.click(agenticLevel().getByRole("tab", { name: /^Subscription$/ }));
    expect(note().textContent).not.toMatch(/AWS_REGION/);
    expect(screen.queryByTestId("agentic-gateway-api-bedrock")).not.toBeInTheDocument();
  });
});

describe("the gateway API rules (AII-FR-DKDC, AII-FR-PHFX)", () => {
  it("AII-FR-DKDC: a gateway config carries the API, and a subscription config carries none", () => {
    const record = bedrockVerified();
    const gateway = cliConfigFor(record, draftForIntegration(record));
    expect(gateway.gatewayApi).toBe("bedrock");
    const subscription = cliVerified("claude_code", "/usr/bin/claude");
    expect("gatewayApi" in cliConfigFor(subscription, draftForIntegration(subscription))).toBe(false);
  });

  it("AII-FR-PHFX: a record with no stored API reads as Anthropic, and a different choice is an edit", () => {
    const legacy = gatewayVerified({ gatewayApi: undefined });
    const draft = draftForIntegration(legacy);
    expect(draft.gatewayApi).toBe("anthropic");
    expect(claudeDraftEdited(legacy, draft)).toBe(false);
    expect(claudeDraftEdited(legacy, { ...draft, gatewayApi: "bedrock" })).toBe(true);
  });

  it("AII-FR-DKDC: the Bedrock line wins over a skipped-check flag, and speaks only for a gateway record", () => {
    const both = gatewayVerified({ gatewayApi: "bedrock", gatewayCheckSkipped: true });
    expect(agenticStatus(both, draftForIntegration(both), "idle", "", false).text).toMatch(/^Bedrock gateway/);
    const subscription = cliVerified("claude_code", "/usr/bin/claude", { gatewayApi: "bedrock" });
    expect(agenticStatus(subscription, draftForIntegration(subscription), "idle", "", false).text).not.toMatch(
      /gateway/,
    );
  });
});
