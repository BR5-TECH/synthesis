import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AgenticAiIntegrations } from "./AiIntegrations";
import { agenticStatus, emptyDraft, cliConfigFor, claudeDraftEdited, draftForIntegration } from "./agenticDraft";
import {
  canVerifyGateway,
  gatewayTokenValidation,
  gatewayTokenVarValidation,
  parseEnvText,
} from "./claudeGateway";
import { aiErrorMessage } from "./aiErrorMessage";
import { resetLogBufferForTest } from "../logging";
import type { AgenticVendorId } from "../types";
import {
  AGENTIC_ALL,
  SAMPLE_TOKEN,
  agenticIntegration,
  agenticLevel,
  argsOfFor,
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
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
});

afterEach(cleanup);

const GATEWAY_TOKEN = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL = "https://llm-gateway.example.com/bedrock";

/**
 * A Claude Code record that last verified in gateway mode. A gateway runs no
 * binary, so it holds no path and no version, and the bundled model list
 * (AIC-FR-QHLN).
 */
function gatewayVerified(over: Record<string, unknown> = {}) {
  return cliVerified("claude_code", "/usr/bin/claude", {
    binaryPath: null,
    pathOrigin: "unset",
    version: null,
    authMode: "custom_gateway",
    gatewayBaseUrl: GATEWAY_URL,
    gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
    gatewayKeyState: "set",
    gatewayMaskedHint: "k2Qz",
    envVars: [],
    modelsOrigin: "catalog",
    ...over,
  });
}

function withClaude(claude: ReturnType<typeof cliVerified>) {
  return [claude, ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v))];
}

const tab = (name: RegExp) => agenticLevel().getByRole("tab", { name });
const gatewayUrl = () => agenticLevel().getByTestId("agentic-gateway-url");
const gatewayVar = () => agenticLevel().getByTestId("agentic-gateway-token-var");
const gatewayToken = () => agenticLevel().getByTestId("agentic-gateway-token");
const envField = () => agenticLevel().getByTestId("agentic-env-vars");
const verifyButton = () => agenticLevel().getByRole("button", { name: /^Verify/ });
const status = () => agenticLevel().getByTestId("agentic-status");
const levelText = () => screen.getByTestId("agentic-level").textContent ?? "";

/** The level mounted and its list loaded: the Claude Code tab exists. */
async function openLevel() {
  render(<AgenticAiIntegrations />);
  const user = userEvent.setup();
  await agenticLevel().findByRole("tab", { name: /Claude Code/ });
  await agenticLevel().findByTestId("agentic-status");
  return user;
}

/** Every command invoked so far, in order, apart from the log flush. */
const commandsSoFar = () =>
  invokeMock.mock.calls.map(([cmd]) => cmd as string).filter((cmd) => cmd !== "append_log_records");

describe("the Claude Code sub-tabs (AII-FR-IUUM)", () => {
  it("AII-FR-IUUM: Subscription opens for a subscription record, above the binary field", async () => {
    backend();
    const user = await openLevel();

    const subTabs = agenticLevel().getByRole("tablist", { name: "Claude Code authentication" });
    const names = within(subTabs).getAllByRole("tab").map((t) => t.textContent);
    expect(names).toEqual(["Subscription", "Custom Gateway"]);
    expect(within(subTabs).getByRole("tab", { name: "Subscription" })).toHaveAttribute("aria-selected", "true");
    expect(agenticLevel().getByTestId("agentic-oauth-token")).toBeInTheDocument();
    expect(agenticLevel().queryByTestId("agentic-gateway-url")).not.toBeInTheDocument();

    const binary = agenticLevel().getByLabelText("Binary");
    expect(subTabs.compareDocumentPosition(binary) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // Choosing a sub-tab invokes nothing and commits nothing.
    await user.click(tab(/Custom Gateway/));
    expect(gatewayUrl()).toBeInTheDocument();
    expect(agenticLevel().queryByTestId("agentic-oauth-token")).not.toBeInTheDocument();
    expect(commandsSoFar().filter((c) => c !== "list_agentic_integrations" && c !== "detect_agentic_cli_binary")).toEqual([]);

    // No other tab has the strip.
    await user.click(agenticLevel().getByRole("tab", { name: "Codex" }));
    expect(agenticLevel().queryByRole("tablist", { name: "Claude Code authentication" })).not.toBeInTheDocument();
  });

  it("AII-FR-IUUM, AII-FR-PHFX: Custom Gateway opens for a gateway record and shows what is stored", async () => {
    backend({ agentic: withClaude(gatewayVerified({ envVars: ["HTTPS_PROXY=http://proxy:3128"] })) });
    await openLevel();

    expect(tab(/Custom Gateway/)).toHaveAttribute("aria-selected", "true");
    expect(gatewayUrl()).toHaveValue(GATEWAY_URL);
    expect(gatewayVar()).toHaveValue("ANTHROPIC_AUTH_TOKEN");
    expect(gatewayToken()).toHaveValue("");
    expect(gatewayToken()).toHaveAttribute("placeholder", "•••• k2Qz");
    expect(agenticLevel().getByTestId("agentic-gateway-note")).toHaveTextContent(/Leave empty to re-verify/);
    expect(envField()).toHaveValue("HTTPS_PROXY=http://proxy:3128");
    expect(status()).toHaveTextContent("Bedrock gateway · token stored · verified");
    // AII-FR-IUUM: a gateway runs no binary, so the sub-tab renders no binary field.
    expect(agenticLevel().queryByLabelText("Binary")).not.toBeInTheDocument();
  });

  it("AII-FR-PHFX: the fields come in the order URL, variable name, token, and a fresh tab prefills the default name", async () => {
    backend();
    const user = await openLevel();
    await user.click(tab(/Custom Gateway/));

    expect(gatewayVar()).toHaveValue("ANTHROPIC_AUTH_TOKEN");
    const after = Node.DOCUMENT_POSITION_FOLLOWING;
    expect(gatewayUrl().compareDocumentPosition(gatewayVar()) & after).toBeTruthy();
    expect(gatewayVar().compareDocumentPosition(gatewayToken()) & after).toBeTruthy();
    expect(gatewayToken().compareDocumentPosition(envField()) & after).toBeTruthy();
    expect(envField().compareDocumentPosition(status()) & after).toBeTruthy();
    expect(gatewayToken()).toHaveAttribute("type", "password");
  });
});

describe("the Custom Gateway checks (AII-FR-HOKG)", () => {
  it("AII-FR-HOKG: Verify waits for a URL, a valid name, valid entries, and a token", async () => {
    backend();
    const user = await openLevel();
    await user.click(tab(/Custom Gateway/));
    const commandsBefore = commandsSoFar();

    expect(verifyButton()).toBeDisabled();
    // AII-FR-HOKG: an empty URL is named, quietly.
    expect(agenticLevel().getByTestId("agentic-gateway-url-note")).toHaveTextContent("Enter the gateway's base URL.");
    await user.click(gatewayUrl());
    await user.paste(GATEWAY_URL);
    expect(agenticLevel().queryByTestId("agentic-gateway-url-note")).not.toBeInTheDocument();
    expect(verifyButton()).toBeDisabled();
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    expect(verifyButton()).toBeEnabled();

    // A bad variable name.
    await user.clear(gatewayVar());
    await user.type(gatewayVar(), "1BAD NAME");
    expect(verifyButton()).toBeDisabled();
    expect(levelText()).toMatch(/A variable name uses letters, digits, and underscores/);
    expect(levelText()).not.toContain("1BAD NAME");
    await user.clear(gatewayVar());
    await user.type(gatewayVar(), "ANTHROPIC_API_KEY");
    expect(verifyButton()).toBeEnabled();

    // A bad environment line is named by number and never quoted.
    fireEvent.change(envField(), { target: { value: "GOOD=1\nthis line has no equals sign" } });
    expect(verifyButton()).toBeDisabled();
    const note = agenticLevel().getByTestId("agentic-env-note");
    expect(note).toHaveTextContent("Line 2 is not written as NAME=value.");
    expect(note.textContent).not.toContain("this line has");
    fireEvent.change(envField(), { target: { value: "GOOD=1\n\n   \nALSO=2" } });
    expect(verifyButton()).toBeEnabled();

    // A token with a space is refused locally, without quoting it.
    await user.clear(gatewayToken());
    await user.type(gatewayToken(), "two words");
    expect(verifyButton()).toBeDisabled();
    expect(agenticLevel().getByTestId("agentic-gateway-note")).toHaveTextContent(/no spaces or control characters/);
    expect(agenticLevel().getByTestId("agentic-gateway-note").textContent).not.toContain("two words");

    // Nothing above invoked an operation: the checks are local.
    expect(commandsSoFar()).toEqual(commandsBefore);
  });

  it("AII-FR-HOKG: a stored token is enough, and an empty URL is not", async () => {
    backend({ agentic: withClaude(gatewayVerified()) });
    const user = await openLevel();
    expect(verifyButton()).toBeEnabled();
    await user.clear(gatewayUrl());
    expect(verifyButton()).toBeDisabled();
  });
});

describe("verifying a Custom Gateway (AII-FR-DKDC)", () => {
  it("AII-FR-DKDC, AII-FR-IUUM, AII-FR-53: submits the gateway shape only, with no path, then shows the gateway line and a masked hint", async () => {
    backend(
      {},
      {
        verify_agentic_integration: () => gatewayVerified(),
      },
    );
    const user = await openLevel();
    // A token typed into the other sub-tab is not carried across.
    await user.click(agenticLevel().getByTestId("agentic-oauth-token"));
    await user.paste(SAMPLE_TOKEN);
    await user.click(tab(/Custom Gateway/));
    await user.click(gatewayUrl());
    await user.paste(`  ${GATEWAY_URL}  `);
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);

    await user.click(verifyButton());
    await waitFor(() => expect(status()).toHaveTextContent("Bedrock gateway · token stored · verified"));

    expect(argsOf("verify_agentic_integration")).toEqual({
      vendor: "claude_code",
      config: {
        authMode: "custom_gateway",
        gatewayBaseUrl: GATEWAY_URL,
        gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
        gatewayToken: GATEWAY_TOKEN,
      },
    });
    expect(gatewayToken()).toHaveValue("");
    expect(gatewayToken()).toHaveAttribute("placeholder", "•••• k2Qz");
    expect(levelText()).not.toContain(GATEWAY_TOKEN);
    expect(levelText()).not.toContain(SAMPLE_TOKEN);
    const values = Array.from(screen.getByTestId("agentic-level").querySelectorAll("input,textarea")).map(
      (el) => (el as HTMLInputElement).value,
    );
    expect(values).not.toContain(GATEWAY_TOKEN);
    expect(values).not.toContain(SAMPLE_TOKEN);
  });

  it("AII-FR-DKDC, AII-FR-52: re-verifies with the stored token and sends neither a token nor entries", async () => {
    backend(
      { agentic: withClaude(gatewayVerified({ envVars: ["A=1"] })) },
      { verify_agentic_integration: () => gatewayVerified({ envVars: ["A=1"] }) },
    );
    const user = await openLevel();
    await user.click(verifyButton());
    await waitFor(() => expect(argsOf("verify_agentic_integration")).toBeDefined());
    const config = (argsOf("verify_agentic_integration") as { config: Record<string, unknown> }).config;
    expect(config).toEqual({
      authMode: "custom_gateway",
      gatewayBaseUrl: GATEWAY_URL,
      gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN",
    });
    expect("gatewayToken" in config).toBe(false);
    expect("envVars" in config).toBe(false);
  });

  it("AII-FR-DKDC: a failure states the typed failure, and changes nothing stored", async () => {
    let failure = "token_var_invalid";
    backend(
      { agentic: withClaude(gatewayVerified()) },
      {
        verify_agentic_integration: () => {
          throw failure;
        },
      },
    );
    const user = await openLevel();
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    await user.click(verifyButton());
    await waitFor(() => expect(status()).toHaveTextContent(/token variable name is not valid/));
    expect(gatewayToken()).toHaveAttribute("placeholder", "•••• k2Qz");
    expect(levelText()).not.toContain(GATEWAY_TOKEN);

    failure = "keychain_unavailable";
    await user.click(verifyButton());
    await waitFor(() => expect(status()).toHaveTextContent(aiErrorMessage("keychain_unavailable")));
    expect(status()).not.toHaveTextContent(/· verified/);
    expect(agenticLevel().queryByRole("dialog")).not.toBeInTheDocument();
    expect(levelText()).not.toContain(GATEWAY_TOKEN);
  });
});

describe("the Environment variables field (AII-FR-EJMG)", () => {
  it("AII-FR-EJMG: sends the typed entries in order when they changed, and says where they are stored", async () => {
    backend(
      {},
      { verify_agentic_integration: () => cliVerified("claude_code", "/usr/bin/claude", { envVars: ["A=1", "B=two words"] }) },
    );
    const user = await openLevel();
    await user.type(agenticLevel().getByLabelText("Binary"), "/usr/bin/claude");
    await user.click(agenticLevel().getByTestId("agentic-oauth-token"));
    await user.paste(SAMPLE_TOKEN);
    expect(agenticLevel().getByTestId("agentic-env-note")).toHaveTextContent(
      /Stored in the settings file, so put credentials in the token field\./,
    );

    fireEvent.change(envField(), { target: { value: "A=1\n\n  B=two words  \n" } });
    await user.click(verifyButton());
    await waitFor(() => expect(argsOf("verify_agentic_integration")).toBeDefined());
    expect(argsOf("verify_agentic_integration")).toEqual({
      vendor: "claude_code",
      config: { path: "/usr/bin/claude", oauthToken: SAMPLE_TOKEN, envVars: ["A=1", "B=two words"] },
    });
    await waitFor(() => expect(envField()).toHaveValue("A=1\nB=two words"));
  });

  it("AII-FR-EJMG, AII-FR-22: an edit returns the tab to an unverified presentation without invoking anything", async () => {
    backend({ agentic: withClaude(cliVerified("claude_code", "/usr/bin/claude", { envVars: ["A=1"] })) });
    await openLevel();
    expect(status()).toHaveTextContent(/· verified/);
    expect(status()).not.toHaveTextContent(/Not verified/);
    const before = commandsSoFar();
    fireEvent.change(envField(), { target: { value: "A=2" } });
    expect(status()).toHaveTextContent(/Not verified/);
    expect(commandsSoFar()).toEqual(before);
    // Both sub-tabs show the same stored entries.
    const user = userEvent.setup();
    await user.click(tab(/Custom Gateway/));
    expect(envField()).toHaveValue("A=2");
  });

  it("AII-FR-EJMG: an emptied field is sent as an empty list", async () => {
    backend(
      { agentic: withClaude(cliVerified("claude_code", "/usr/bin/claude", { envVars: ["A=1"] })) },
      { verify_agentic_integration: () => cliVerified("claude_code", "/usr/bin/claude") },
    );
    const user = await openLevel();
    fireEvent.change(envField(), { target: { value: "" } });
    await user.click(verifyButton());
    await waitFor(() => expect(argsOf("verify_agentic_integration")).toBeDefined());
    expect(argsOf("verify_agentic_integration")).toEqual({
      vendor: "claude_code",
      config: { path: "/usr/bin/claude", envVars: [] },
    });
  });
});

describe("the candidate and its lifetime (AII-FR-21, AII-FR-22, AII-FR-53)", () => {
  it("AII-FR-21, AII-FR-53: leaving the tab discards an unverified gateway candidate", async () => {
    backend({ agentic: withClaude(cliVerified("claude_code", "/usr/bin/claude")) });
    const user = await openLevel();
    await user.click(tab(/Custom Gateway/));
    await user.click(gatewayUrl());
    await user.paste(GATEWAY_URL);
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);

    await user.click(agenticLevel().getByRole("tab", { name: "Codex" }));
    await user.click(agenticLevel().getByRole("tab", { name: "Claude Code" }));

    expect(tab(/^Subscription$/)).toHaveAttribute("aria-selected", "true");
    await user.click(tab(/Custom Gateway/));
    expect(gatewayUrl()).toHaveValue("");
    expect(gatewayToken()).toHaveValue("");
  });

  it("AII-FR-22: opening the other sub-tab on a verified record reads as unverified", async () => {
    backend({ agentic: withClaude(gatewayVerified()) });
    const user = await openLevel();
    expect(status()).toHaveTextContent(/· verified/);
    expect(status()).not.toHaveTextContent(/Not verified/);
    await user.click(tab(/^Subscription$/));
    // The record stores no binary, so the Subscription sub-tab looks for one
    // (AII-FR-17). Either way it no longer reads as verified.
    await waitFor(() => expect(status()).not.toHaveTextContent(/· verified/));
    await waitFor(() => expect(status()).not.toHaveTextContent(/Looking for the binary/));
    await user.click(tab(/Custom Gateway/));
    expect(status()).toHaveTextContent("Bedrock gateway · token stored · verified");
  });

  it("AII-FR-27: a gateway token that can no longer be read says so", async () => {
    backend({
      agentic: withClaude(
        gatewayVerified({ state: "key_unavailable", gatewayKeyState: "unavailable", gatewayMaskedHint: "k2Qz" }),
      ),
    });
    await openLevel();
    expect(status()).toHaveTextContent(/The stored gateway token can no longer be read/);
  });

  it("AII-FR-26: Clear returns the tab to Subscription with empty gateway fields and entries", async () => {
    backend(
      { agentic: withClaude(gatewayVerified({ envVars: ["A=1"] })) },
      {
        clear_agentic_integration: () => AGENTIC_ALL.map((v) => agenticIntegration(v)),
      },
    );
    const user = await openLevel();
    await user.click(agenticLevel().getByRole("button", { name: "Clear" }));
    await waitFor(() => expect(tab(/^Subscription$/)).toHaveAttribute("aria-selected", "true"));
    await user.click(tab(/Custom Gateway/));
    expect(gatewayUrl()).toHaveValue("");
    expect(envField()).toHaveValue("");
    expect(gatewayVar()).toHaveValue("ANTHROPIC_AUTH_TOKEN");
  });
});

describe("what each tab renders and carries (AII-FR-49, AII-FR-30, AII-FR-PHFX, AII-FR-EJMG)", () => {
  it("AII-FR-PHFX: the name and the token share one joined frame, Verify follows it, and the name shows the stored name", async () => {
    backend({ agentic: withClaude(gatewayVerified({ gatewayTokenVar: "ANTHROPIC_API_KEY" })) });
    await openLevel();
    expect(gatewayVar()).toHaveValue("ANTHROPIC_API_KEY");
    const frame = agenticLevel().getByRole("group", { name: "Token" });
    expect(frame).toContainElement(gatewayVar());
    expect(frame).toContainElement(gatewayToken());
    expect(frame).toHaveTextContent("=");
    // SET-FR-21: the same joined frame as the Docker image name and tag.
    expect(frame).toHaveClass("joined-field");
    expect(gatewayVar().compareDocumentPosition(gatewayToken()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(frame).not.toContainElement(verifyButton());
    expect(frame.parentElement).toBe(verifyButton().parentElement);
    expect(frame.compareDocumentPosition(verifyButton()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(agenticLevel().getByLabelText("Token variable name")).toBe(gatewayVar());
    expect(agenticLevel().getByLabelText("Gateway token")).toBe(gatewayToken());
  });

  it("AII-FR-PHFX, AII-FR-HOKG: the row's one note names a wrong name before a wrong token", async () => {
    backend({ agentic: withClaude(gatewayVerified()) });
    const user = await openLevel();
    await user.click(gatewayToken());
    await user.paste("two words");
    await user.clear(gatewayVar());
    await user.type(gatewayVar(), "1BAD");
    const note = agenticLevel().getByTestId("agentic-gateway-note");
    expect(note).toHaveTextContent(/A variable name uses letters, digits, and underscores/);
    expect(gatewayVar()).toHaveAttribute("aria-invalid", "true");
    expect(gatewayToken()).toHaveAttribute("aria-invalid", "true");
    await user.clear(gatewayVar());
    await user.type(gatewayVar(), "GOOD_NAME");
    expect(note).toHaveTextContent(/no spaces or control characters/);
    expect(gatewayVar()).not.toHaveAttribute("aria-invalid");
  });

  it("AII-FR-49, AII-FR-16: no other tab renders a gateway, token, or environment field", async () => {
    backend();
    const user = await openLevel();
    for (const name of ["Codex", "OpenCode", "Claude Agent API", "Custom agent API"]) {
      await user.click(agenticLevel().getByRole("tab", { name }));
      await waitFor(() => expect(agenticLevel().getByRole("tab", { name })).toHaveAttribute("aria-selected", "true"));
      for (const id of [
        "agentic-gateway-url",
        "agentic-gateway-token-var",
        "agentic-gateway-token",
        "agentic-oauth-token",
        "agentic-env-vars",
      ]) {
        expect(agenticLevel().queryByTestId(id), `${name}: ${id}`).not.toBeInTheDocument();
      }
    }
  });

  it("AII-FR-49, AII-FR-53: a token typed under one sub-tab goes when the other opens, and reaches no other tab", async () => {
    backend();
    const user = await openLevel();
    await user.click(agenticLevel().getByTestId("agentic-oauth-token"));
    await user.paste(SAMPLE_TOKEN);
    await user.click(tab(/Custom Gateway/));
    await user.click(tab(/^Subscription$/));
    expect(agenticLevel().getByTestId("agentic-oauth-token")).toHaveValue("");

    await user.click(tab(/Custom Gateway/));
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    await user.click(tab(/^Subscription$/));
    await user.click(tab(/Custom Gateway/));
    expect(gatewayToken()).toHaveValue("");

    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    await user.click(agenticLevel().getByRole("tab", { name: "Codex" }));
    const values = Array.from(screen.getByTestId("agentic-level").querySelectorAll("input,textarea")).map(
      (el) => (el as HTMLInputElement).value,
    );
    expect(values).not.toContain(GATEWAY_TOKEN);
  });

  it("AII-FR-EJMG: the note on where entries are stored shows in both sub-tabs, and the gateway one names the Bedrock entries", async () => {
    backend();
    const user = await openLevel();
    const note = () => agenticLevel().getByTestId("agentic-env-note");
    expect(note()).toHaveTextContent(/Stored in the settings file/);
    const bedrockNames = [
      "AWS_REGION",
      "ANTHROPIC_DEFAULT_OPUS_MODEL",
      "ANTHROPIC_DEFAULT_SONNET_MODEL",
      "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    ];
    for (const name of bedrockNames) expect(note().textContent).not.toContain(name);
    await user.click(tab(/Custom Gateway/));
    expect(note()).toHaveTextContent(/Stored in the settings file/);
    // The gateway sub-tab names the optional Bedrock entries.
    for (const name of bedrockNames) expect(note()).toHaveTextContent(name);
  });

  it("AII-FR-51, AII-FR-HOKG: a bad environment line leaves Verify unavailable in the Subscription sub-tab too", async () => {
    backend({ agentic: withClaude(cliVerified("claude_code", "/usr/bin/claude")) });
    await openLevel();
    expect(verifyButton()).toBeEnabled();
    fireEvent.change(envField(), { target: { value: "no equals sign here" } });
    expect(verifyButton()).toBeDisabled();
    expect(agenticLevel().getByTestId("agentic-env-note")).toHaveTextContent("Line 1 is not written as NAME=value.");
  });

  it("AII-FR-DKDC: the success line names no model count and no version", async () => {
    backend({ agentic: withClaude(gatewayVerified({ version: "2.1.4", modelsOrigin: "probed" })) });
    await openLevel();
    expect(status()).toHaveTextContent("Bedrock gateway · token stored · verified");
    expect(status()).not.toHaveTextContent(/model|2\.1\.4/);
  });

  it("AII-FR-IUUM, AII-FR-DKDC: the gateway sub-tab needs no binary to verify, and sends no path", async () => {
    backend({}, { verify_agentic_integration: () => gatewayVerified() });
    const user = await openLevel();
    await user.click(tab(/Custom Gateway/));
    expect(agenticLevel().queryByLabelText("Binary")).not.toBeInTheDocument();
    await user.click(gatewayUrl());
    await user.paste(GATEWAY_URL);
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    expect(status()).toHaveTextContent("Not verified — verify this gateway before use.");
    expect(verifyButton()).toBeEnabled();
    await user.click(verifyButton());
    await waitFor(() => expect(status()).toHaveTextContent("Bedrock gateway · token stored · verified"));
    expect("path" in (argsOf("verify_agentic_integration") as { config: object }).config).toBe(false);
  });

  it("AII-FR-17: a detection still running does not hold up the gateway sub-tab", async () => {
    let finish: (value: unknown) => void = () => {};
    backend(
      {},
      {
        detect_agentic_cli_binary: () => new Promise((resolve) => (finish = resolve)),
        verify_agentic_integration: () => gatewayVerified(),
      },
    );
    const user = await openLevel();
    await waitFor(() => expect(status()).toHaveTextContent("Looking for the binary…"));
    await user.click(tab(/Custom Gateway/));
    expect(status()).not.toHaveTextContent("Looking for the binary…");
    await user.click(gatewayUrl());
    await user.paste(GATEWAY_URL);
    await user.click(gatewayToken());
    await user.paste(GATEWAY_TOKEN);
    expect(verifyButton()).toBeEnabled();
    // The detection ends, and the Subscription sub-tab reports what it found.
    finish({ path: null });
    await user.click(tab(/^Subscription$/));
    await waitFor(() => expect(status()).toHaveTextContent(/No binary found/));
  });

  it("AII-FR-17: a gateway record opens with no detection, and Subscription detects once it is open", async () => {
    backend({ agentic: withClaude(gatewayVerified()) });
    const user = await openLevel();
    await waitFor(() => expect(status()).toHaveTextContent("Bedrock gateway · token stored · verified"));
    expect(commandsSoFar()).not.toContain("detect_agentic_cli_binary");
    await user.click(tab(/^Subscription$/));
    await waitFor(() => expect(commandsSoFar()).toContain("detect_agentic_cli_binary"));
    expect(agenticLevel().getByLabelText("Binary")).toBeInTheDocument();
  });
});

describe("the pure rules", () => {
  it("AII-FR-HOKG: variable names and tokens are checked structurally", () => {
    expect(gatewayTokenVarValidation("ANTHROPIC_AUTH_TOKEN")).toBe("");
    expect(gatewayTokenVarValidation("_x1")).toBe("");
    for (const bad of ["", "1A", "A B", "A-B", "É"]) {
      expect(gatewayTokenVarValidation(bad), bad).not.toBe("");
    }
    expect(gatewayTokenValidation("")).toBe("");
    expect(gatewayTokenValidation("abc-123_XYZ")).toBe("");
    expect(gatewayTokenValidation("a b")).not.toBe("");
    expect(gatewayTokenValidation("a\tb")).not.toBe("");
    expect(gatewayTokenValidation("é")).not.toBe("");
  });

  it("AII-FR-EJMG: lines are trimmed, blank lines dropped, and a bad line is named by number", () => {
    expect(parseEnvText("A=1\r\n\n  B = x \nC=")).toEqual({
      entries: ["A=1"],
      error: "Line 3 is not written as NAME=value.",
    });
    expect(parseEnvText("A=1\n  B=a=b  \nC=")).toEqual({ entries: ["A=1", "B=a=b", "C="], error: "" });
    expect(parseEnvText("=x").error).toBe("Line 1 is not written as NAME=value.");
    expect(parseEnvText("").entries).toEqual([]);
  });

  it("AII-FR-HOKG: Verify gating reads the record", () => {
    const stored = gatewayVerified();
    const fields = { ...emptyDraft(), gatewayUrl: GATEWAY_URL };
    expect(canVerifyGateway(stored, fields)).toBe(true);
    expect(canVerifyGateway({ ...stored, gatewayKeyState: "unset" }, fields)).toBe(false);
    expect(canVerifyGateway({ ...stored, gatewayKeyState: "unset" }, { ...fields, gatewayToken: "tok" })).toBe(true);
  });

  it("AII-FR-IUUM: the submitted shape follows the open sub-tab", () => {
    const stored = gatewayVerified();
    const draft = {
      ...emptyDraft(),
      path: "/p",
      authMode: "custom_gateway" as const,
      gatewayUrl: " https://g.example ",
      gatewayTokenVar: " MY_VAR ",
      gatewayToken: " tok ",
      oauthToken: SAMPLE_TOKEN,
    };
    expect(cliConfigFor(stored, draft)).toEqual({
      authMode: "custom_gateway",
      gatewayBaseUrl: "https://g.example",
      gatewayTokenVar: "MY_VAR",
      gatewayToken: "tok",
    });
    // AII-FR-DKDC: edited entries travel with the gateway shape too.
    expect(cliConfigFor(stored, { ...draft, envText: "AWS_REGION=eu-west-1" })).toEqual({
      authMode: "custom_gateway",
      gatewayBaseUrl: "https://g.example",
      gatewayTokenVar: "MY_VAR",
      gatewayToken: "tok",
      envVars: ["AWS_REGION=eu-west-1"],
    });
    expect(cliConfigFor(stored, { ...draft, authMode: "subscription", envText: "X=1" })).toEqual({
      path: "/p",
      oauthToken: SAMPLE_TOKEN,
      envVars: ["X=1"],
    });
    // Codex and OpenCode send the path alone.
    expect(cliConfigFor(agenticIntegration("codex" as AgenticVendorId), draft)).toEqual({ path: "/p" });
    expect(claudeDraftEdited(stored, { ...draft, gatewayUrl: GATEWAY_URL, gatewayTokenVar: "ANTHROPIC_AUTH_TOKEN", gatewayToken: "" })).toBe(false);
    // AIC-FR-QHLN: a gateway status ignores the path, which a gateway does not use.
    const gatewayDraft = { ...draftForIntegration(stored), path: "/anything" };
    expect(agenticStatus(stored, gatewayDraft, "idle", "", false).text).toBe("Bedrock gateway · token stored · verified");
  });

  it("AII-FR-DKDC: the gateway failures read as plain text and quote nothing the author typed", () => {
    expect(aiErrorMessage("env_var_invalid:2")).toMatch(/entry 2 is not written as NAME=value/);
    expect(aiErrorMessage("env_var_reserved:PATH")).toMatch(/^PATH is set by Synthesis/);
    expect(aiErrorMessage("token_var_invalid")).toMatch(/token variable name is not valid/);
    expect(aiErrorMessage("timed_out")).toMatch(/did not answer in time/);
  });
});
