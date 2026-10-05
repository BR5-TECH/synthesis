import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { aiErrorMessage, reasoningDefaultNote, reasoningOptions } from "./AiIntegrations";
import { AI_ERRORS, type ModelOption } from "../types";
import {
  API_ALL,
  BothLevels,
  apiIntegration,
  apiLevel,
  apiVerified,
  argsOfFor,
  backendFor,
  openApiTab,
  openRouterWith,
  optionsOf,
  pick,
  withLadder,
} from "../test/aiIntegrationsFixtures";

// The section reaches the backend only through `invoke`; mock the bridge so
// jsdom never needs the Tauri runtime — and so no test can spawn a real CLI or
// make a real request.
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const backend = backendFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The reasoning selector (AII-FR-41 .. FR-47)
// ---------------------------------------------------------------------------

describe("the reasoning selector", () => {
  it("offers exactly the levels the selected model declares (AII-FR-41, AII-FR-42, AII-FR-44)", async () => {
    // AII-FR-42: never a depth the selected model cannot honour.
    backend(
      openRouterWith(
        [withLadder("a/model", ["high", "medium", "low"])],
        "a/model",
      ),
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await apiLevel().findByRole("combobox", { name: "Reasoning" }),
    );
    expect(
      within(apiLevel().getByRole("listbox", { name: "Reasoning" }))
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Model default", "Off", "high", "medium", "low"]);
  });

  it("renders in no other tab of the level (AII-FR-41, AII-FR-42, AII-FR-44)", async () => {
    // AII-FR-41: the OpenRouter tab, and only it.
    backend({
      api: API_ALL.map((p) =>
        apiVerified(p, "https://example.com/v1", {
          models: [withLadder("a/model", ["high"])],
          selectedModel: "a/model",
        }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(
      await apiLevel().findByRole("combobox", { name: "Reasoning" }),
    ).toBeInTheDocument();

    for (const name of [/Anthropic/, /OpenAI/, /Custom/]) {
      await openApiTab(user, apiLevel(), name);
      expect(
        apiLevel().queryByRole("combobox", { name: "Reasoning" }),
        `${name} renders no reasoning selector`,
      ).not.toBeInTheDocument();
    }
  });

  it("renders none on the provider default or a model that cannot reason (AII-FR-41, AII-FR-05)", async () => {
    // AII-FR-41: the provider-default entry carries no model whose capability
    // is known, so there is nothing to offer a depth for.
    backend(openRouterWith([withLadder("a/model", ["high"])], null));
    render(<BothLevels />);
    await apiLevel().findByRole("combobox", { name: "Model" });
    expect(
      apiLevel().queryByRole("combobox", { name: "Reasoning" }),
    ).not.toBeInTheDocument();

    cleanup();
    backend(
      openRouterWith([{ id: "plain/model", label: "Plain" }], "plain/model"),
    );
    render(<BothLevels />);
    await apiLevel().findByRole("combobox", { name: "Model" });
    expect(
      apiLevel().queryByRole("combobox", { name: "Reasoning" }),
    ).not.toBeInTheDocument();
  });

  it("offers off and on for a model with no ladder, and no off when mandatory (AII-FR-43, AII-FR-44)", async () => {
    // AII-FR-43 — asserted on the pure helper, so all three shapes are covered
    // without three renders.
    const ladderless = {
      id: "m",
      label: "m",
      reasoning: {
        mandatory: false,
        defaultEnabled: true,
        supportedEfforts: null,
        defaultEffort: null,
      },
    };
    expect(reasoningOptions(ladderless)?.map((o) => o.label)).toEqual([
      "Model default",
      "Off",
      "On",
    ]);

    // A model that always reasons offers no off entry, in either shape.
    expect(
      reasoningOptions({
        ...ladderless,
        reasoning: { ...ladderless.reasoning, mandatory: true },
      })?.map((o) => o.label),
    ).toEqual(["Model default", "On"]);
    expect(
      reasoningOptions(withLadder("m", ["high", "low"], { mandatory: true }))?.map(
        (o) => o.label,
      ),
    ).toEqual(["Model default", "high", "low"]);

    // AII-FR-41: no descriptor, no control.
    expect(reasoningOptions({ id: "m", label: "m" })).toBeNull();
    expect(reasoningOptions(null)).toBeNull();

    // AII-FR-44: the default entry says what the model does when nothing is
    // asked of it, rather than standing unexplained.
    expect(
      reasoningDefaultNote(withLadder("m", ["high", "low"], { mandatory: true })),
    ).toMatch(/always reasons/i);
    expect(reasoningDefaultNote(withLadder("m", ["medium"]))).toMatch(
      /reasons at medium/i,
    );
    expect(
      reasoningDefaultNote({
        ...ladderless,
        reasoning: { ...ladderless.reasoning, defaultEnabled: false },
      }),
    ).toMatch(/does not reason unless/i);
  });

  it("applies a reasoning choice at once (AII-FR-45)", async () => {
    // AII-FR-45: no save action in between, exactly as a model selection.
    backend(
      openRouterWith([withLadder("a/model", ["high", "low"])], "a/model"),
      {
        set_ai_api_reasoning: (args) => ({
          ...apiVerified("openrouter", "https://openrouter.ai/api/v1", {
            models: [withLadder("a/model", ["high", "low"])],
            selectedModel: "a/model",
          }),
          selectedReasoning: args?.choice,
        }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await apiLevel().findByRole("combobox", { name: "Reasoning" });
    await pick(user, apiLevel(), "Reasoning", "low");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "openrouter",
        choice: { kind: "effort", effort: "low" },
      }),
    );
    expect(
      apiLevel().getByRole("combobox", { name: "Reasoning" }),
    ).toHaveTextContent("low");

    // AII-FR-45: the model-default entry invokes the same operation with none.
    await pick(user, apiLevel(), "Reasoning", "Model default");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "openrouter",
        choice: null,
      }),
    );
  });

  it("re-renders against the newly selected model (AII-FR-46)", async () => {
    // AII-FR-46: a level the new model does not offer is not carried across.
    const models = [
      withLadder("rich/model", ["max", "high"]),
      withLadder("poor/model", ["high"]),
    ];
    backend(openRouterWith(models, "rich/model"), {
      // The backend cleared the stale choice (AAP-FR-29); the surface renders
      // whatever the returned record carries rather than working it out.
      set_ai_api_model: () => ({
        ...apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          models,
          selectedModel: "poor/model",
        }),
        selectedReasoning: null,
      }),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    await apiLevel().findByRole("combobox", { name: "Reasoning" });
    expect(await optionsOf(user, apiLevel(), "Reasoning")).toEqual([
      "Model default",
      "Off",
      "max",
      "high",
    ]);
    await user.keyboard("{Escape}");

    await pick(user, apiLevel(), "Model", "poor/model");
    await waitFor(() =>
      expect(
        apiLevel().getByRole("combobox", { name: "Reasoning" }),
      ).toHaveTextContent("Model default"),
    );
    expect(await optionsOf(user, apiLevel(), "Reasoning")).toEqual([
      "Model default",
      "Off",
      "high",
    ]);
  });

  it("offers a level this application has never heard of (AII-FR-47, AII-FR-45)", async () => {
    // AII-FR-47: the record's levels are the whole of what the selector
    // presents, so a provider adding one needs no change here.
    backend(
      openRouterWith([withLadder("a/model", ["ludicrous", "high"])], "a/model"),
      {
        set_ai_api_reasoning: (args) => ({
          ...apiVerified("openrouter", "https://openrouter.ai/api/v1", {
            models: [withLadder("a/model", ["ludicrous", "high"])],
            selectedModel: "a/model",
          }),
          selectedReasoning: args?.choice,
        }),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await apiLevel().findByRole("combobox", { name: "Reasoning" });
    expect(await optionsOf(user, apiLevel(), "Reasoning")).toEqual([
      "Model default",
      "Off",
      "ludicrous",
      "high",
    ]);
    await user.keyboard("{Escape}");

    await pick(user, apiLevel(), "Reasoning", "ludicrous");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "openrouter",
        choice: { kind: "effort", effort: "ludicrous" },
      }),
    );
  });

  it("renders a rejected reasoning choice beside the action", async () => {
    // The typed refusals of AAP-FR-27 say what to do next rather than reading
    // as a generic failure.
    expect(aiErrorMessage(AI_ERRORS.reasoningMandatory)).toMatch(
      /always reasons/i,
    );
    expect(aiErrorMessage(AI_ERRORS.reasoningUnsupported)).toMatch(
      /does not support reasoning/i,
    );
    expect(aiErrorMessage(AI_ERRORS.noModelSelected)).toMatch(/choose a model/i);

    backend(
      openRouterWith([withLadder("a/model", ["high"])], "a/model"),
      {
        set_ai_api_reasoning: () => {
          throw AI_ERRORS.reasoningMandatory;
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await apiLevel().findByRole("combobox", { name: "Reasoning" });
    await pick(user, apiLevel(), "Reasoning", "high");
    await waitFor(() =>
      expect(apiLevel().getByTestId("ai-api-action-error")).toHaveTextContent(
        /always reasons/i,
      ),
    );
    // AII-FR-09: the status line is for configuration errors, not this.
    expect(apiLevel().getByTestId("ai-api-status")).toHaveTextContent(/verified/i);
  });
});

// ---------------------------------------------------------------------------
// The gaps a review of the above found: shapes and branches the tests reached
// only through their pure helpers.
// ---------------------------------------------------------------------------

describe("the reasoning selector's off/on shape", () => {
  /** A model that reasons but declares no ladder of its own. */
  function ladderless(id: string, mandatory = false): ModelOption {
    return {
      id,
      label: id,
      reasoning: {
        mandatory,
        defaultEnabled: true,
        supportedEfforts: null,
        defaultEffort: null,
      },
    };
  }

  it("sends {kind:'on'} and {kind:'off'} over the wire (AII-FR-43, AII-FR-44, AII-FR-45)", async () => {
    backend(openRouterWith([ladderless("a/model")], "a/model"), {
      set_ai_api_reasoning: (args) => ({
        ...apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          models: [ladderless("a/model")],
          selectedModel: "a/model",
        }),
        selectedReasoning: args?.choice,
      }),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    await apiLevel().findByRole("combobox", { name: "Reasoning" });
    await pick(user, apiLevel(), "Reasoning", "On");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "openrouter",
        choice: { kind: "on" },
      }),
    );

    await pick(user, apiLevel(), "Reasoning", "Off");
    await waitFor(() =>
      expect(argsOf("set_ai_api_reasoning")).toEqual({
        provider: "openrouter",
        choice: { kind: "off" },
      }),
    );
  });

  it("renders a stored off/on choice back on the control (AII-FR-44)", async () => {
    backend({
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          models: [ladderless("a/model")],
          selectedModel: "a/model",
          selectedReasoning: { kind: "on" },
        }),
        ...API_ALL.filter((p) => p !== "openrouter").map((p) =>
          apiIntegration(p),
        ),
      ],
    });
    render(<BothLevels />);
    expect(
      await apiLevel().findByRole("combobox", { name: "Reasoning" }),
    ).toHaveTextContent("On");
  });

  it("renders the note that explains the model default (AII-FR-43, AII-FR-44)", async () => {
    // The note is what AII-FR-43 requires in place of a disabled control for a
    // model that always reasons — a claim about rendered text, so the wiring
    // from the *selected* model has to be asserted, not just the helper.
    backend(openRouterWith([ladderless("a/model", true)], "a/model"));
    render(<BothLevels />);
    expect(
      await apiLevel().findByTestId("ai-api-reasoning-note"),
    ).toHaveTextContent(/always reasons/i);
    // ...and no off entry is offered for it.
    const user = userEvent.setup();
    expect(await optionsOf(user, apiLevel(), "Reasoning")).toEqual([
      "Model default",
      "On",
    ]);
  });

  it("keeps the reasoning row between the model row and activation (AII-FR-05)", async () => {
    backend(
      openRouterWith(
        [withLadder("a/model", ["high", "low"])],
        "a/model",
      ),
    );
    render(<BothLevels />);
    const panel = await screen.findByTestId("ai-api-panel-openrouter");

    const rows = [
      within(panel).getByRole("combobox", { name: "Model" }),
      within(panel).getByTestId("ai-api-models-origin"),
      within(panel).getByRole("combobox", { name: "Reasoning" }),
      within(panel).getByTestId("ai-api-reasoning-note"),
      within(panel).getByRole("button", { name: /use this integration/i }),
    ];
    for (let i = 1; i < rows.length; i++) {
      expect(
        rows[i - 1].compareDocumentPosition(rows[i]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
        `row ${i} follows row ${i - 1}`,
      ).toBeTruthy();
    }
  });

  it("puts activation directly after the model row when nothing reasons (AII-FR-41, AII-FR-05)", async () => {
    backend(openRouterWith([{ id: "plain/model", label: "Plain" }], "plain/model"));
    render(<BothLevels />);
    const panel = await screen.findByTestId("ai-api-panel-openrouter");

    expect(
      within(panel).queryByTestId("ai-api-reasoning-note"),
    ).not.toBeInTheDocument();
    const origin = within(panel).getByTestId("ai-api-models-origin");
    const activate = within(panel).getByRole("button", {
      name: /use this integration/i,
    });
    expect(
      origin.compareDocumentPosition(activate) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
