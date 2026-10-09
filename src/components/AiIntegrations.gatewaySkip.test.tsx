import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AgenticAiIntegrations } from "./AiIntegrations";
import { agenticStatus, draftForIntegration } from "./agenticDraft";
import { aiErrorMessage, isGatewayCheckFailure } from "./aiErrorMessage";
import { resetLogBufferForTest } from "../logging";
import {
  AGENTIC_ALL,
  SAMPLE_TOKEN,
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
const GATEWAY_URL = "https://bedrock-gateway.example.com";

/** A Claude Code record that last verified in gateway mode. */
function gatewayVerified(over: Record<string, unknown> = {}) {
  return cliVerified("claude_code", "/usr/bin/claude", {
    authMode: "custom_gateway",
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

/** The record the backend returns once the author accepted without the check. */
const accepted = () =>
  gatewayVerified({ gatewayCheckSkipped: true, modelsOrigin: "catalog" });

function withClaude(claude: ReturnType<typeof cliVerified>) {
  return [claude, ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v))];
}

const gatewayToken = () => agenticLevel().getByTestId("agentic-gateway-token");
const verifyButton = () => agenticLevel().getByRole("button", { name: /^Verify/ });
const status = () => agenticLevel().getByTestId("agentic-status");
const dialog = () => agenticLevel().queryByRole("dialog");
const levelText = () => screen.getByTestId("agentic-level").textContent ?? "";

/** The configs `verify_agentic_integration` was invoked with, in order. */
const verifyConfigs = () =>
  invokeMock.mock.calls
    .filter(([cmd]) => cmd === "verify_agentic_integration")
    .map(([, args]) => (args as { config: Record<string, unknown> }).config);

/**
 * A backend whose verification fails with `firstFailure` and then answers each
 * later call with `later` (a record, or a thrown code).
 */
function failingThen(firstFailure: string, later: () => unknown = accepted) {
  let calls = 0;
  backend(
    { agentic: withClaude(gatewayVerified()) },
    {
      verify_agentic_integration: () => {
        calls += 1;
        if (calls === 1) throw firstFailure;
        return later();
      },
    },
  );
}

/** The level open on the stored gateway, with a new token typed and Verify clicked. */
async function verifyWithTypedToken() {
  render(<AgenticAiIntegrations />);
  const user = userEvent.setup();
  await agenticLevel().findByRole("tab", { name: /Claude Code/ });
  await agenticLevel().findByTestId("agentic-status");
  await user.click(gatewayToken());
  await user.paste(GATEWAY_TOKEN);
  await user.click(verifyButton());
  return user;
}

describe("the confirmation a failed gateway check opens (AII-FR-ZQTB)", () => {
  it.each([
    ["gateway_status:400", /The gateway answered with HTTP 400\./],
    ["gateway_unreachable:connection refused", /The gateway could not be reached: connection refused/],
    ["gateway_timed_out", /The gateway did not answer in time\./],
    ["gateway_not_a_model_list", /The gateway answered, but not with a model list\./],
    ["tls_untrusted:unknown_issuer:bedrock-gateway.example.com", /bedrock-gateway\.example\.com/],
  ])("AII-FR-ZQTB: %s opens the dialog and states the failure in it and in the status line", async (code, text) => {
    failingThen(code);
    await verifyWithTypedToken();
    const opened = await agenticLevel().findByRole("dialog", { name: "The gateway check failed" });
    expect(opened).toHaveAttribute("aria-modal", "true");
    expect(agenticLevel().getByTestId("gateway-check-failed-message")).toHaveTextContent(text);
    expect(opened).toHaveTextContent(/Claude Code verified\./);
    expect(opened).toHaveTextContent(/The model list is then the bundled one\./);
    expect(status()).toHaveTextContent(text);
    expect(levelText()).not.toContain(GATEWAY_TOKEN);
  });

  it("AII-FR-ZQTB: Tab and Shift+Tab stay on the dialog's two actions", async () => {
    failingThen("gateway_status:400");
    const user = await verifyWithTypedToken();
    await agenticLevel().findByRole("dialog");
    const cancel = agenticLevel().getByTestId("gateway-check-failed-cancel");
    const accept = agenticLevel().getByTestId("gateway-check-failed-accept");
    await waitFor(() => expect(cancel).toHaveFocus());
    await user.tab();
    expect(accept).toHaveFocus();
    await user.tab();
    expect(cancel).toHaveFocus();
    await user.tab({ shift: true });
    expect(accept).toHaveFocus();
  });

  it("AII-FR-ZQTB: Cancel takes focus when the dialog opens", async () => {
    failingThen("gateway_status:400");
    await verifyWithTypedToken();
    await agenticLevel().findByRole("dialog");
    await waitFor(() => expect(agenticLevel().getByTestId("gateway-check-failed-cancel")).toHaveFocus());
  });

  it.each([
    ["Cancel", async (user: ReturnType<typeof userEvent.setup>) => user.click(agenticLevel().getByTestId("gateway-check-failed-cancel"))],
    ["Escape", async (user: ReturnType<typeof userEvent.setup>) => user.keyboard("{Escape}")],
    ["a click outside", async () => {
      fireEvent.mouseDown(agenticLevel().getByTestId("gateway-check-failed-scrim"));
    }],
  ])("AII-FR-ZQTB: %s closes the dialog, invokes nothing, and keeps the failure", async (_route, dismiss) => {
    failingThen("gateway_status:400");
    const user = await verifyWithTypedToken();
    await agenticLevel().findByRole("dialog");
    // Focus on Cancel shows that the dialog's listeners are in place.
    await waitFor(() => expect(agenticLevel().getByTestId("gateway-check-failed-cancel")).toHaveFocus());
    const before = invokeMock.mock.calls.filter(([cmd]) => cmd !== "append_log_records").length;

    await dismiss(user);

    await waitFor(() => expect(dialog()).not.toBeInTheDocument());
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd !== "append_log_records")).toHaveLength(before);
    expect(gatewayToken()).toHaveFocus();
    expect(status()).toHaveTextContent(/The gateway answered with HTTP 400\./);
  });

  it("AII-FR-ZQTB, AIC-FR-KWMV: Accept anyway sends the same payload with the check skipped, and the status line says so", async () => {
    failingThen("gateway_status:400");
    const user = await verifyWithTypedToken();
    await agenticLevel().findByRole("dialog");

    await user.click(agenticLevel().getByTestId("gateway-check-failed-accept"));

    await waitFor(() =>
      expect(status()).toHaveTextContent(/gateway not checked · accepted by you · .*verified · 2\.1\.4/),
    );
    expect(dialog()).not.toBeInTheDocument();
    const [first, second] = verifyConfigs();
    expect(first).toEqual({
      path: "/usr/bin/claude",
      authMode: "custom_gateway",
      gatewayApi: "anthropic",
      gatewayBaseUrl: GATEWAY_URL,
      gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
      gatewayToken: GATEWAY_TOKEN,
    });
    expect(second).toEqual({ ...first, skipGatewayCheck: true });
    expect(verifyConfigs()).toHaveLength(2);
  });

  it("AII-FR-53: the typed token goes from the field once the accepted submission resolves", async () => {
    failingThen("gateway_status:400");
    const user = await verifyWithTypedToken();
    await agenticLevel().findByRole("dialog");
    expect(gatewayToken()).toHaveValue(GATEWAY_TOKEN);

    await user.click(agenticLevel().getByTestId("gateway-check-failed-accept"));

    await waitFor(() => expect(gatewayToken()).toHaveValue(""));
    expect(gatewayToken()).toHaveAttribute("placeholder", "•••• k2Qz");
    expect(levelText()).not.toContain(GATEWAY_TOKEN);
  });

  it.each([
    ["gateway_status:502", /The gateway answered with HTTP 502\./],
    ["not_the_expected_cli", /That program is not this vendor's CLI\./],
  ])("AII-FR-ZQTB: a failed Accept anyway (%s) states its failure and opens no second dialog", async (code, text) => {
    failingThen("gateway_status:400", () => {
      throw code;
    });
    const user = await verifyWithTypedToken();
    await agenticLevel().findByRole("dialog");

    await user.click(agenticLevel().getByTestId("gateway-check-failed-accept"));

    await waitFor(() => expect(status()).toHaveTextContent(text));
    expect(dialog()).not.toBeInTheDocument();
    expect(verifyConfigs()).toHaveLength(2);
  });

  it("AII-FR-ZQTB, AII-FR-DKDC: Accept anyway with the stored token sends no token, as the first payload did", async () => {
    failingThen("gateway_status:400");
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await agenticLevel().findByRole("tab", { name: /Claude Code/ });
    await agenticLevel().findByTestId("agentic-status");
    fireEvent.change(agenticLevel().getByTestId("agentic-env-vars"), { target: { value: "HTTPS_PROXY=http://proxy:3128" } });
    await user.click(verifyButton());
    await agenticLevel().findByRole("dialog");
    await user.click(agenticLevel().getByTestId("gateway-check-failed-accept"));
    await waitFor(() => expect(status()).toHaveTextContent(/gateway not checked/));

    const [first, second] = verifyConfigs();
    expect(first).toEqual({
      path: "/usr/bin/claude",
      authMode: "custom_gateway",
      gatewayApi: "anthropic",
      gatewayBaseUrl: GATEWAY_URL,
      gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
      envVars: ["HTTPS_PROXY=http://proxy:3128"],
    });
    expect(second).toEqual({ ...first, skipGatewayCheck: true });
  });

  it.each(["timed_out", "not_found", "not_the_expected_cli", "token_malformed", "env_var_reserved:PATH"])(
    "AII-FR-ZQTB: a binary or field failure (%s) opens no dialog",
    async (code) => {
      failingThen(code);
      await verifyWithTypedToken();
      await waitFor(() => expect(status()).toHaveTextContent(aiErrorMessage(code)));
      expect(dialog()).not.toBeInTheDocument();
    },
  );

  it("AII-FR-ZQTB: a Subscription verification opens no dialog, whatever it fails with", async () => {
    backend(
      { agentic: withClaude(cliVerified("claude_code", "/usr/bin/claude")) },
      {
        verify_agentic_integration: () => {
          throw "gateway_status:400";
        },
      },
    );
    render(<AgenticAiIntegrations />);
    const user = userEvent.setup();
    await agenticLevel().findByRole("tab", { name: /Claude Code/ });
    await agenticLevel().findByTestId("agentic-status");
    await user.click(agenticLevel().getByTestId("agentic-oauth-token"));
    await user.paste(SAMPLE_TOKEN);
    await user.click(verifyButton());
    await waitFor(() => expect(status()).toHaveTextContent(/HTTP 400/));
    expect(dialog()).not.toBeInTheDocument();
  });
});

describe("the gateway check failure rules (AII-FR-ZQTB, AII-FR-DKDC)", () => {
  it("AII-FR-ZQTB: only the gateway check codes count as a gateway check failure", () => {
    for (const code of [
      "gateway_status:400",
      "gateway_unreachable:dns",
      "gateway_timed_out",
      "gateway_not_a_model_list",
      "tls_untrusted:expired:host",
      new Error("gateway_status:502"),
    ]) {
      expect(isGatewayCheckFailure(code), String(code)).toBe(true);
    }
    for (const code of ["timed_out", "not_found", "token_missing", "gateway", "", null, undefined, 42]) {
      expect(isGatewayCheckFailure(code), String(code)).toBe(false);
    }
  });

  it("AII-FR-DKDC: a skipped check says so in place of the model count, and an unskipped one counts models", () => {
    const skipped = accepted();
    expect(agenticStatus(skipped, draftForIntegration(skipped), "idle", "", false).text).toMatch(
      /^gateway not checked · accepted by you · .* · verified · 2\.1\.4$/,
    );
    // The flag speaks only for a gateway record.
    const subscription = cliVerified("claude_code", "/usr/bin/claude", { gatewayCheckSkipped: true });
    expect(agenticStatus(subscription, draftForIntegration(subscription), "idle", "", false).text).not.toMatch(
      /gateway/,
    );
    const checked = gatewayVerified({ gatewayCheckSkipped: false });
    expect(agenticStatus(checked, draftForIntegration(checked), "idle", "", false).text).toMatch(
      /^gateway answered · 1 model · /,
    );
  });
});
