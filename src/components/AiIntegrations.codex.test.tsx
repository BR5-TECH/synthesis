// The Codex tab of the Agentic AI section —
// `specifications/ui/AII-ai-integrations.md` AII-FR-OTQY, AII-FR-LOYM,
// AII-FR-RNSO, AII-FR-XXSQ, AII-FR-NXAV, AII-FR-BMGM, AII-FR-24, AII-FR-26,
// AII-FR-30, AII-FR-49 and AII-FR-53, against the wire shapes of
// `specifications/core/AIC-agentic-integrations.md`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { BACKEND_DEFAULT_LABEL, SAME_AS_DEFAULT_LABEL } from "./aiIntegrationsShared";
import type { AgenticIntegration, ModelOption } from "../types";
import {
  AGENTIC_ALL,
  BothLevels,
  agenticIntegration,
  agenticLevel,
  argsOfFor,
  backendFor,
  cliVerified,
  commandsFor,
} from "../test/aiIntegrationsFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const commands = commandsFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
});

afterEach(cleanup);

/** A fake value shaped like an access token. Not a real credential. */
const ACCESS_TOKEN = "cdx-FAKE-TEST-TOKEN-NOT-REAL-Q7xk";

const CODEX_EFFORTS = [
  { id: "low", label: "Low" },
  { id: "medium", label: "Medium" },
  { id: "high", label: "High" },
  { id: "xhigh", label: "Extra high" },
  { id: "max", label: "Max" },
  { id: "ultra", label: "Ultra" },
];

function codexModel(id: string, label: string, efforts: string[]): ModelOption {
  return {
    id,
    label,
    reasoning: { mandatory: true, supportedEfforts: efforts, defaultEffort: efforts[0] },
  };
}

const ALL = ["low", "medium", "high", "xhigh", "max", "ultra"];
const CODEX_MODELS: ModelOption[] = [
  codexModel("gpt-6.2-sol", "GPT-6.2-Sol", ALL),
  codexModel("gpt-6-luna", "GPT-6-Luna", ALL.slice(0, 5)),
  codexModel("gpt-5.5", "GPT-5.5", ALL.slice(0, 4)),
];

/** A Codex record as the backend sends it: its own catalog and levels. */
function codex(over: Partial<AgenticIntegration> = {}): AgenticIntegration {
  return agenticIntegration("codex", {
    codexAuthMode: "login_folder",
    models: CODEX_MODELS,
    reasoningEfforts: CODEX_EFFORTS,
    ...over,
  });
}

function codexVerified(over: Partial<AgenticIntegration> = {}): AgenticIntegration {
  return cliVerified("codex", "/usr/bin/codex", {
    codexAuthMode: "login_folder",
    models: CODEX_MODELS,
    reasoningEfforts: CODEX_EFFORTS,
    version: "0.172.0",
    ...over,
  });
}

/** The list with `record` in Codex's place. */
function listWith(record: AgenticIntegration): AgenticIntegration[] {
  return [
    cliVerified("claude_code", "/usr/bin/claude"),
    record,
    ...AGENTIC_ALL.slice(2).map((v) => agenticIntegration(v)),
  ];
}

async function openCodex(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await agenticLevel().findByRole("tab", { name: /Codex/ }));
  return within(await agenticLevel().findByTestId("agentic-panel-codex"));
}

/** The labels a native `<select>` offers, in order. */
function labelsOf(select: HTMLElement): string[] {
  return Array.from((select as HTMLSelectElement).options).map((o) => o.textContent ?? "");
}

describe("the Codex tab", () => {
  it("AII-FR-OTQY, AII-FR-16, AII-FR-LOYM: Codex is implemented and renders its two sub-tabs above the binary field", async () => {
    backend({ agentic: listWith(codex()) });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    expect(agenticLevel().getByRole("tab", { name: /Codex/ })).toHaveAttribute(
      "data-unimplemented",
      "false",
    );
    expect(panel.queryByTestId("to-be-implemented")).toBeNull();

    const strip = panel.getByRole("tablist", { name: "Codex authentication" });
    const tabs = within(strip).getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Login folder", "Access token"]);
    // The sub-tab that opens is the record's mode.
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(panel.getByLabelText("Binary")).toBeEnabled();

    // AII-FR-16 / AII-FR-49: no Claude Code field reaches the Codex tab.
    expect(panel.queryByTestId("agentic-oauth-token")).toBeNull();
    expect(panel.queryByTestId("agentic-env-vars")).toBeNull();
    expect(panel.queryByTestId("agentic-gateway-url")).toBeNull();
  });

  it("AII-FR-LOYM: a record in access_token mode opens on the Access token sub-tab, and choosing a sub-tab invokes nothing", async () => {
    backend({
      agentic: listWith(
        codexVerified({ codexAuthMode: "access_token", keyState: "set", maskedHint: "Q7xk" }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    expect(panel.getByTestId("agentic-auth-tab-access_token")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(panel.getByLabelText("Binary")).toBeInTheDocument();
    expect(panel.getByLabelText("Access token")).toBeInTheDocument();

    const before = commands().length;
    await user.click(panel.getByTestId("agentic-auth-tab-login_folder"));
    expect(panel.getByTestId("agentic-auth-tab-login_folder")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // Both sub-tabs keep the binary field.
    expect(panel.getByLabelText("Binary")).toBeInTheDocument();
    expect(panel.queryByLabelText("Access token")).toBeNull();
    expect(commands().length).toBe(before);
    // AII-FR-22: a change of sub-tab is an edit.
    expect(panel.getByTestId("agentic-status")).toHaveTextContent(/Not verified/);
  });

  it("AII-FR-RNSO, AII-FR-20, AII-FR-17: Login folder states where the login lives, renders no credential field, and verifies with the path and the mode", async () => {
    backend(
      { agentic: listWith(codex()) },
      {
        detect_agentic_cli_binary: () => ({ path: "/opt/homebrew/bin/codex" }),
        verify_agentic_integration: () => codexVerified({ binaryPath: "/opt/homebrew/bin/codex" }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    // AII-FR-17: Codex detects in either sub-tab when it holds no path.
    await waitFor(() => expect(panel.getByLabelText("Binary")).toHaveValue("/opt/homebrew/bin/codex"));
    expect(argsOf("detect_agentic_cli_binary")).toEqual({ vendor: "codex" });

    const note = panel.getByTestId("codex-login-folder-note");
    expect(note).toHaveTextContent("codex login");
    expect(note).toHaveTextContent("~/.codex");
    expect(note).toHaveTextContent(/a run mounts that folder/);
    expect(panel.queryByRole("textbox", { name: /token/i })).toBeNull();
    expect(
      agenticLevel().getByTestId("agentic-panel-codex").querySelector('input[type="password"]'),
    ).toBeNull();

    // Verify sits on the binary row.
    const binaryRow = panel.getByLabelText("Binary").parentElement as HTMLElement;
    await user.click(within(binaryRow).getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(argsOf("verify_agentic_integration")).toEqual({
        vendor: "codex",
        config: { path: "/opt/homebrew/bin/codex", authMode: "login_folder" },
      }),
    );
    await waitFor(() =>
      expect(panel.getByTestId("agentic-status")).toHaveTextContent(/verified · 0\.172\.0/),
    );
  });

  it("AII-FR-XXSQ, AII-FR-NXAV, AII-FR-30, AII-FR-53: a typed token is sent once with the mode, then leaves the field and the page", async () => {
    backend(
      { agentic: listWith(codexVerified()) },
      {
        verify_agentic_integration: () =>
          codexVerified({ codexAuthMode: "access_token", keyState: "set", maskedHint: "Q7xk" }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    await user.click(panel.getByTestId("agentic-auth-tab-access_token"));

    const field = panel.getByLabelText("Access token");
    expect(field).toHaveAttribute("type", "password");
    expect(field).toHaveValue("");
    expect(panel.getByTestId("agentic-access-token-note")).toHaveTextContent(
      /access-token settings of your ChatGPT workspace/,
    );
    // AII-FR-NXAV: with no typed token and none stored, Verify is unavailable.
    const tokenRow = field.parentElement as HTMLElement;
    const verify = within(tokenRow).getByRole("button", { name: "Verify" });
    expect(verify).toBeDisabled();
    // Verify sits on the token row, not on the binary row.
    const binaryRow = panel.getByLabelText("Binary").parentElement as HTMLElement;
    expect(within(binaryRow).queryByRole("button", { name: "Verify" })).toBeNull();

    await user.type(field, `  ${ACCESS_TOKEN}  `);
    expect(verify).toBeEnabled();
    await user.click(verify);

    await waitFor(() =>
      expect(argsOf("verify_agentic_integration")).toEqual({
        vendor: "codex",
        config: { path: "/usr/bin/codex", authMode: "access_token", accessToken: ACCESS_TOKEN },
      }),
    );
    // AII-FR-53 / AII-FR-30: the field is empty, the hint stands in for the
    // token, and the token is nowhere on the page.
    await waitFor(() => expect(panel.getByLabelText("Access token")).toHaveValue(""));
    await waitFor(() =>
      expect(panel.getByLabelText("Access token")).toHaveAttribute("placeholder", "•••• Q7xk"),
    );
    expect(document.body.innerHTML).not.toContain(ACCESS_TOKEN);
  });

  it("AII-FR-NXAV: an empty field with a stored token re-verifies without one", async () => {
    backend(
      {
        agentic: listWith(
          codexVerified({ codexAuthMode: "access_token", keyState: "set", maskedHint: "Q7xk" }),
        ),
      },
      {
        verify_agentic_integration: () =>
          codexVerified({ codexAuthMode: "access_token", keyState: "set", maskedHint: "Q7xk" }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    expect(panel.getByTestId("agentic-access-token-note")).toHaveTextContent(
      /Leave empty to re-verify with the stored token/,
    );
    const tokenRow = panel.getByLabelText("Access token").parentElement as HTMLElement;
    await user.click(within(tokenRow).getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(argsOf("verify_agentic_integration")).toEqual({
        vendor: "codex",
        config: { path: "/usr/bin/codex", authMode: "access_token" },
      }),
    );
  });

  it("AII-FR-NXAV, AII-FR-30: a token with a space is refused inline, quotes none of it, and cannot be submitted", async () => {
    backend({
      agentic: listWith(
        codexVerified({ codexAuthMode: "access_token", keyState: "set", maskedHint: "Q7xk" }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    const field = panel.getByLabelText("Access token");
    await user.type(field, "cdx-SECRET part");
    const note = panel.getByTestId("agentic-access-token-note");
    expect(note).toHaveTextContent(/no spaces, line breaks, or control characters/);
    expect(note).not.toHaveTextContent("SECRET");
    expect(field).toHaveAttribute("aria-invalid", "true");
    const tokenRow = field.parentElement as HTMLElement;
    // A stored token does not make a malformed typed one submittable.
    expect(within(tokenRow).getByRole("button", { name: "Verify" })).toBeDisabled();
    expect(commands()).not.toContain("verify_agentic_integration");
  });

  it("AII-FR-53: a token typed under Access token goes when the author leaves the sub-tab", async () => {
    backend({ agentic: listWith(codexVerified()) });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    await user.click(panel.getByTestId("agentic-auth-tab-access_token"));
    await user.type(panel.getByLabelText("Access token"), ACCESS_TOKEN);
    await user.click(panel.getByTestId("agentic-auth-tab-login_folder"));
    await user.click(panel.getByTestId("agentic-auth-tab-access_token"));
    expect(panel.getByLabelText("Access token")).toHaveValue("");
    expect(document.body.innerHTML).not.toContain(ACCESS_TOKEN);
  });

  it("AII-FR-15: an access token the vault cannot read is named on the status line", async () => {
    backend({
      agentic: listWith(
        codexVerified({ codexAuthMode: "access_token", state: "key_unavailable" }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    expect(panel.getByTestId("agentic-status")).toHaveTextContent(
      "The stored access token can no longer be read. Enter one and verify again.",
    );
  });

  it("AII-FR-26: Clear returns Codex to its Login folder sub-tab", async () => {
    let list = listWith(
      codexVerified({ codexAuthMode: "access_token", keyState: "set", maskedHint: "Q7xk" }),
    );
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        clear_agentic_integration: () => {
          list = listWith(codex());
          return list;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    expect(panel.getByTestId("agentic-auth-tab-access_token")).toHaveAttribute(
      "aria-selected",
      "true",
    );

    const detections = () =>
      commands().filter((cmd) => cmd === "detect_agentic_cli_binary").length;
    const detectedBefore = detections();
    await user.click(panel.getByRole("button", { name: "Clear" }));
    await waitFor(() =>
      expect(panel.getByTestId("agentic-auth-tab-login_folder")).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    expect(panel.getByTestId("agentic-empty")).toHaveTextContent(/login folder or give it an access token/);
    // No path, no hint, no selections, and no detection that refills the field.
    expect(panel.getByLabelText("Binary")).toHaveValue("");
    expect(panel.getByLabelText("Reasoning effort")).toHaveValue("");
    await user.click(panel.getByTestId("agentic-auth-tab-access_token"));
    expect(panel.getByLabelText("Access token")).not.toHaveAttribute("placeholder", "•••• Q7xk");
    expect(detections()).toBe(detectedBefore);
  });
});

describe("the Codex effort selectors", () => {
  it("AII-FR-24, AII-FR-BMGM: the default selector lists the default model's own ladder, labelled from the vendor list", async () => {
    backend({ agentic: listWith(codexVerified({ selectedModel: "gpt-6-luna" })) });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);

    expect(labelsOf(panel.getByLabelText("Reasoning effort"))).toEqual([
      BACKEND_DEFAULT_LABEL,
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Max",
    ]);
  });

  it("AII-FR-BMGM: the list follows the default model at once when it changes", async () => {
    let list = listWith(codexVerified({ selectedModel: "gpt-6-luna" }));
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_model: (args) => {
          list = listWith(codexVerified({ selectedModel: (args?.modelId as string) ?? null }));
          return list[1];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    expect(labelsOf(panel.getByLabelText("Reasoning effort"))).not.toContain("Ultra");

    await user.click(panel.getByRole("combobox", { name: "Model" }));
    await user.click(
      within(panel.getByRole("listbox", { name: "Model" })).getByRole("option", {
        name: "GPT-6.2-Sol",
      }),
    );
    await waitFor(() =>
      expect(labelsOf(panel.getByLabelText("Reasoning effort"))).toContain("Ultra"),
    );
  });

  it("AII-FR-BMGM: the effort list changes in the same render as the model, with no request of its own", async () => {
    let list = listWith(codexVerified({ selectedModel: "gpt-6-luna" }));
    let release: () => void = () => {};
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_model: (args) =>
          new Promise((resolve) => {
            release = () => {
              list = listWith(codexVerified({ selectedModel: (args?.modelId as string) ?? null }));
              resolve(list[1]);
            };
          }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    await user.click(panel.getByRole("combobox", { name: "Model" }));
    await user.click(
      within(panel.getByRole("listbox", { name: "Model" })).getByRole("option", {
        name: "GPT-6.2-Sol",
      }),
    );
    // While the change is in flight the record still names luna, and the list agrees with it.
    await waitFor(() => expect(commands()).toContain("set_agentic_integration_model"));
    expect(labelsOf(panel.getByLabelText("Reasoning effort"))).not.toContain("Ultra");
    const before = commands().length;

    release();
    await waitFor(() =>
      expect(labelsOf(panel.getByLabelText("Reasoning effort"))).toContain("Ultra"),
    );
    // The list followed the new model without asking the backend for anything.
    expect(commands().slice(before).filter((cmd) => cmd !== "append_log_records")).toEqual([]);
  });

  it("AII-FR-BMGM: a row's model change governs its effort list, and a stored effort it does not offer reads as the default", async () => {
    let list = listWith(
      codexVerified({ selectedModel: "gpt-6.2-sol", effortOverrides: { work: "ultra" } }),
    );
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_model: (args) => {
          const record = list[1];
          list = listWith({
            ...record,
            modelOverrides: { ...record.modelOverrides, [args?.turnKind as string]: args?.modelId as string },
          });
          return list[1];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    await user.click(panel.getByTestId("agentic-per-task-summary"));
    expect(panel.getByLabelText("Work effort")).toHaveValue("ultra");

    await user.click(panel.getByRole("combobox", { name: "Work model" }));
    await user.click(
      within(panel.getByRole("listbox", { name: "Work model" })).getByRole("option", {
        name: "GPT-6-Luna",
      }),
    );
    await waitFor(() => expect(labelsOf(panel.getByLabelText("Work effort"))).not.toContain("Ultra"));
    expect(panel.getByLabelText("Work effort")).toHaveValue("");
    expect(labelsOf(panel.getByLabelText("Work effort"))[0]).toBe(SAME_AS_DEFAULT_LABEL);
    // Another row still follows the default model.
    expect(labelsOf(panel.getByLabelText("Review effort"))).toContain("Ultra");
  });

  it("AII-FR-BMGM: a stored effort the model does not offer renders as the default entry", async () => {
    backend({
      agentic: listWith(codexVerified({ selectedModel: "gpt-5.5", selectedEffort: "ultra" })),
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    expect(panel.getByLabelText("Reasoning effort")).toHaveValue("");
  });

  it("AII-FR-24: with no default model, the default selector lists the levels every ladder shares", async () => {
    backend({ agentic: listWith(codexVerified()) });
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    expect(labelsOf(panel.getByLabelText("Reasoning effort"))).toEqual([
      BACKEND_DEFAULT_LABEL,
      "Low",
      "Medium",
      "High",
      "Extra high",
    ]);
  });

  it("AII-FR-BMGM, AII-FR-24: a row's effort list follows the row's own model, else the default model", async () => {
    let list = listWith(
      codexVerified({
        selectedModel: "gpt-6.2-sol",
        modelOverrides: { review: "gpt-5.5" },
        effortOverrides: { work: "ultra" },
      }),
    );
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_effort: (args) => {
          const record = list[1];
          const kind = args?.turnKind as string;
          list = listWith({
            ...record,
            effortOverrides: { ...record.effortOverrides, [kind]: args?.effortId as string },
          });
          return list[1];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    await user.click(panel.getByTestId("agentic-per-task-summary"));

    // Work follows the default model, which offers Ultra.
    const work = panel.getByLabelText("Work effort");
    expect(labelsOf(work)).toContain("Ultra");
    expect(work).toHaveValue("ultra");
    // Review runs on its own model, which stops at Extra high.
    const review = panel.getByLabelText("Review effort");
    expect(labelsOf(review)).toEqual([
      SAME_AS_DEFAULT_LABEL,
      "Low",
      "Medium",
      "High",
      "Extra high",
    ]);

    await user.selectOptions(review, "xhigh");
    await waitFor(() =>
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "codex",
        turnKind: "review",
        effortId: "xhigh",
      }),
    );
  });
});

describe("the Codex refusals", () => {
  it("AII-FR-20, AII-FR-NXAV: a token refusal is named in words that fit an access token and quote nothing", async () => {
    backend(
      { agentic: listWith(codexVerified()) },
      {
        verify_agentic_integration: () => {
          throw "token_malformed";
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();
    const panel = await openCodex(user);
    await user.click(panel.getByTestId("agentic-auth-tab-access_token"));
    await user.type(panel.getByLabelText("Access token"), ACCESS_TOKEN);
    const tokenRow = panel.getByLabelText("Access token").parentElement as HTMLElement;
    await user.click(within(tokenRow).getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(panel.getByTestId("agentic-status")).toHaveTextContent("That is not a valid token."),
    );
    expect(panel.getByTestId("agentic-status")).not.toHaveTextContent(/OAuth/);
    // AII-FR-30: the refused token is echoed into no text of the page. The masked
    // field itself still holds what the author typed, for a retry (AII-FR-53).
    expect(document.body.textContent).not.toContain(ACCESS_TOKEN);
  });
});
