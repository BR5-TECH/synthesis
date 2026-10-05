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

import { matchesOption } from "./FilterableSelect";
import {
  API_ALL,
  BothLevels,
  agenticLevel,
  apiIntegration,
  apiLevel,
  apiVerified,
  argsOfFor,
  backendFor,
  commandsFor,
  manyModels,
  openApiTab,
  openRouterWith,
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
const commands = commandsFor(invokeMock);
const argsOf = argsOfFor(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The filterable model selector (AII-FR-36 .. FR-40)
// ---------------------------------------------------------------------------

describe("the model selector", () => {
  it("filters a large catalogue locally and says how much it is showing (AII-FR-36, AII-FR-37, AII-FR-38)", async () => {
    // AII-FR-36 / FR-37 / FR-38: typing narrows options the record already
    // carries, issues nothing, and reports what it did.
    const models = [
      ...manyModels(300),
      { id: "anthropic/claude-opus-5", label: "Claude Opus 5" },
      { id: "anthropic/claude-opus-4.6", label: "Claude Opus 4.6" },
    ];
    backend({
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1", { models }),
        ...API_ALL.filter((p) => p !== "openrouter").map((p) =>
          apiIntegration(p),
        ),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await apiLevel().findByRole("combobox", { name: "Model" }),
    );
    const filter = apiLevel().getByLabelText("Filter model");
    expect(filter).toHaveFocus();
    // 302 models plus the provider-default entry.
    expect(apiLevel().getByTestId("ai-api-model-count")).toHaveTextContent(
      "303 of 303 shown",
    );

    const before = invokeMock.mock.calls.length;
    await user.type(filter, "opus");
    expect(
      within(apiLevel().getByRole("listbox", { name: "Model" }))
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Claude Opus 5", "Claude Opus 4.6"]);
    expect(apiLevel().getByTestId("ai-api-model-count")).toHaveTextContent(
      "2 of 303 shown",
    );
    expect(
      invokeMock.mock.calls.length,
      "filtering issues no operation and no request",
    ).toBe(before);

    // AII-FR-36: what was typed is not state about anything.
    await user.keyboard("{Escape}");
    await user.click(apiLevel().getByRole("combobox", { name: "Model" }));
    expect(apiLevel().getByLabelText("Filter model")).toHaveValue("");
    expect(apiLevel().getByTestId("ai-api-model-count")).toHaveTextContent(
      "303 of 303 shown",
    );
  });

  it("matches label and id in any case, and says when nothing matches (AII-FR-37, AII-FR-39)", async () => {
    // AII-FR-37: the id is matched too, so a provider-qualified model is
    // reachable by the half of its name the author remembers.
    expect(
      matchesOption({ id: "anthropic/claude-opus-5", label: "Claude Opus 5" }, "OPUS"),
    ).toBe(true);
    expect(
      matchesOption({ id: "anthropic/claude-opus-5", label: "Claude Opus 5" }, "anthropic"),
    ).toBe(true);
    expect(
      matchesOption({ id: "a/b", label: "Something" }, "thin"),
      "anchored at neither end",
    ).toBe(true);
    expect(matchesOption({ id: "a/b", label: "Something" }, "  ")).toBe(true);
    expect(matchesOption({ id: "a/b", label: "Something" }, "zzz")).toBe(false);

    backend({
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          selectedModel: "model-a",
        }),
        ...API_ALL.filter((p) => p !== "openrouter").map((p) =>
          apiIntegration(p),
        ),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await apiLevel().findByRole("combobox", { name: "Model" }),
    );
    await user.type(apiLevel().getByLabelText("Filter model"), "zzz");
    // AII-FR-39: a first-class row, not an empty panel, and nothing selectable.
    expect(apiLevel().getByTestId("ai-api-model-empty")).toBeInTheDocument();
    expect(
      within(apiLevel().getByRole("listbox", { name: "Model" })).queryAllByRole(
        "option",
      ),
    ).toHaveLength(0);

    // The selection is left exactly as it was.
    await user.keyboard("{Escape}");
    expect(apiLevel().getByRole("combobox", { name: "Model" })).toHaveTextContent(
      "Model A",
    );
    expect(commands()).not.toContain("set_ai_api_model");
  });

  it("is operable from the keyboard alone (AII-FR-40, AII-FR-12, AII-FR-23)", async () => {
    // AII-FR-40: arrows move the highlight, Enter commits, Escape closes
    // leaving the selection untouched, and focus returns to the selector.
    backend({
      api: [
        apiVerified("openrouter", "https://openrouter.ai/api/v1"),
        ...API_ALL.filter((p) => p !== "openrouter").map((p) =>
          apiIntegration(p),
        ),
      ],
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    const combobox = await apiLevel().findByRole("combobox", { name: "Model" });
    await user.click(combobox);
    // From the provider-default entry down onto the first model.
    await user.keyboard("{ArrowDown}{Enter}");
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "openrouter",
        modelId: "model-a",
      }),
    );
    expect(combobox).toHaveFocus();

    // Escape commits nothing.
    const before = invokeMock.mock.calls.length;
    await user.click(combobox);
    await user.keyboard("{ArrowDown}{Escape}");
    expect(invokeMock.mock.calls.length).toBe(before);
    expect(combobox).toHaveFocus();
  });

  it("is the same control in the agentic level (AII-FR-23, AII-FR-36)", async () => {
    // AII-FR-23 / FR-36: nothing about choosing a model is learned twice.
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(
      await agenticLevel().findByRole("combobox", { name: "Model" }),
    );
    expect(agenticLevel().getByLabelText("Filter model")).toHaveFocus();
    expect(agenticLevel().getByTestId("agentic-model-count")).toBeInTheDocument();
  });
});

describe("the filterable selector's remaining branches", () => {
  const openRouterModels = [
    { id: "model-a", label: "Model A" },
    { id: "model-b", label: "Model B" },
  ];

  function renderOpenRouter(selectedModel: string | null = null) {
    // The echo matters: without it the record never advances, and the
    // "re-picking what is already stored invokes nothing" guard would suppress
    // the second commit of a wrap-around test for the wrong reason.
    backend(openRouterWith(openRouterModels, selectedModel), {
      set_ai_api_model: (args) =>
        apiVerified("openrouter", "https://openrouter.ai/api/v1", {
          models: openRouterModels,
          selectedModel: (args?.modelId as string | null) ?? null,
        }),
    });
    render(<BothLevels />);
    return userEvent.setup();
  }

  it("wraps the highlight in both directions (AII-FR-40)", async () => {
    const user = renderOpenRouter();
    const combobox = await apiLevel().findByRole("combobox", { name: "Model" });

    // Three entries: the provider default, then two models. ArrowUp from the
    // first wraps to the last.
    await user.click(combobox);
    // The highlight is announced, not merely painted: a screen reader follows
    // `aria-activedescendant` rather than the background colour.
    const filter = apiLevel().getByLabelText("Filter model");
    const activeLabel = () => {
      const id = filter.getAttribute("aria-activedescendant");
      return id ? document.getElementById(id)?.textContent : null;
    };
    expect(activeLabel()).toBe("Provider default");
    await user.keyboard("{ArrowUp}");
    expect(activeLabel()).toBe("Model B");
    await user.keyboard("{Enter}");
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "openrouter",
        modelId: "model-b",
      }),
    );

    // ArrowDown past the last wraps back to the first — the default entry.
    await user.click(combobox);
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{Enter}");
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "openrouter",
        modelId: null,
      }),
    );
  });

  it("commits nothing when the filter matches nothing (AII-FR-39)", async () => {
    const user = renderOpenRouter("model-a");
    const combobox = await apiLevel().findByRole("combobox", { name: "Model" });

    await user.click(combobox);
    await user.type(apiLevel().getByLabelText("Filter model"), "zzz");
    const before = invokeMock.mock.calls.length;
    // Enter with nothing selectable must do nothing at all, rather than
    // committing whatever the highlight last pointed at.
    await user.keyboard("{Enter}");
    await user.keyboard("{ArrowDown}{ArrowUp}{Enter}");
    expect(invokeMock.mock.calls.length).toBe(before);
    expect(combobox).toHaveTextContent("Model A");
  });

  it("invokes nothing when the current selection is re-picked (AII-FR-12)", async () => {
    const user = renderOpenRouter("model-a");
    await apiLevel().findByRole("combobox", { name: "Model" });
    // As in AII-FR-11: the agentic level's mount is a chain, and this fixture
    // leaves its first CLI tab without a stored path, so `detect_agentic_cli_
    // binary` is still in flight when the combobox appears. Baselined before it
    // lands, the re-pick below is blamed for a command the mount issued.
    await waitFor(() =>
      expect(commands()).toContain("detect_agentic_cli_binary"),
    );
    const before = invokeMock.mock.calls.length;

    await pick(user, apiLevel(), "Model", "Model A");
    expect(
      invokeMock.mock.calls.length,
      "re-picking what is already stored spends no round trip",
    ).toBe(before);
    expect(apiLevel().getByRole("combobox", { name: "Model" })).toHaveFocus();
  });

  it("commits a hovered option and closes on an outside click (AII-FR-40)", async () => {
    const user = renderOpenRouter();
    const combobox = await apiLevel().findByRole("combobox", { name: "Model" });

    // The mouse moves the highlight; the keyboard then commits it.
    await user.click(combobox);
    await user.hover(
      within(apiLevel().getByRole("listbox", { name: "Model" })).getByRole(
        "option",
        { name: "Model B" },
      ),
    );
    await user.keyboard("{Enter}");
    await waitFor(() =>
      expect(argsOf("set_ai_api_model")).toEqual({
        provider: "openrouter",
        modelId: "model-b",
      }),
    );

    // An outside click dismisses without committing.
    const before = invokeMock.mock.calls.length;
    await user.click(combobox);
    await user.click(document.body);
    await waitFor(() =>
      expect(
        apiLevel().queryByRole("listbox", { name: "Model" }),
      ).not.toBeInTheDocument(),
    );
    expect(invokeMock.mock.calls.length).toBe(before);
  });

  it("floats on the application's opaque menu surface", async () => {
    // The regression this guards is invisible to behaviour: the panel renders,
    // filters, and commits perfectly while being transparent, so the rows
    // beneath show through it. `.menu` is what supplies the elevated
    // background, border, and shadow; `.input` is what makes the resting
    // control line up with the fields above it. Both are stylesheet classes —
    // a hand-rolled background referencing a token that does not exist fails
    // silently in CSS.
    const user = renderOpenRouter();
    const combobox = await apiLevel().findByRole("combobox", { name: "Model" });
    expect(combobox).toHaveClass("input", "fsel__trigger");

    await user.click(combobox);
    const listbox = apiLevel().getByRole("listbox", { name: "Model" });
    const panel = listbox.parentElement;
    expect(panel).toHaveClass("menu", "fsel__menu");
    expect(apiLevel().getByLabelText("Filter model")).toHaveClass("input");
  });

  it("is keyboard-operable in the agentic level too (AII-FR-40, AII-FR-12, AII-FR-23, AII-FR-36)", async () => {
    backend();
    render(<BothLevels />);
    const user = userEvent.setup();

    const combobox = await agenticLevel().findByRole("combobox", {
      name: "Model",
    });
    await user.click(combobox);
    await user.keyboard("{ArrowDown}{Enter}");
    await waitFor(() =>
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        // The selector above the per-task rows names no task (AII-FR-23).
        turnKind: null,
        modelId: "opus",
      }),
    );
    expect(combobox).toHaveFocus();
  });
});

describe("a model list that predates reasoning (AII-FR-48)", () => {
  it("says the endpoint must be verified again (AII-FR-48, AII-FR-41)", async () => {
    // Exactly the shape a store written before this application knew about
    // reasoning holds: models probed and stored, none carrying a descriptor.
    const stale = [
      { id: "anthropic/claude-opus-5", label: "anthropic/claude-opus-5" },
      { id: "openai/gpt-5", label: "openai/gpt-5" },
    ];
    backend(openRouterWith(stale, "anthropic/claude-opus-5"));
    render(<BothLevels />);

    expect(
      await apiLevel().findByTestId("ai-api-reasoning-stale"),
    ).toHaveTextContent(/verify this endpoint again/i);
    expect(
      apiLevel().queryByRole("combobox", { name: "Reasoning" }),
    ).not.toBeInTheDocument();
  });

  it("says nothing once a refreshed list declares reasoning (AII-FR-48, AII-FR-41)", async () => {
    backend(
      openRouterWith(
        [withLadder("a/model", ["high", "low"]), { id: "plain", label: "Plain" }],
        "a/model",
      ),
    );
    render(<BothLevels />);

    await apiLevel().findByRole("combobox", { name: "Reasoning" });
    expect(
      apiLevel().queryByTestId("ai-api-reasoning-stale"),
    ).not.toBeInTheDocument();
  });

  it("says nothing merely because the selected model cannot reason (AII-FR-48, AII-FR-41)", async () => {
    // One model declaring reasoning is enough to prove the list is current, so
    // selecting a model that cannot reason is an ordinary state, not a stale
    // one — the reasoning row is absent and no notice explains it away.
    backend(
      openRouterWith(
        [withLadder("a/model", ["high"]), { id: "plain", label: "Plain" }],
        "plain",
      ),
    );
    render(<BothLevels />);

    await apiLevel().findByRole("combobox", { name: "Model" });
    expect(
      apiLevel().queryByTestId("ai-api-reasoning-stale"),
    ).not.toBeInTheDocument();
    expect(
      apiLevel().queryByRole("combobox", { name: "Reasoning" }),
    ).not.toBeInTheDocument();
  });

  it("stays out of the other providers' tabs (AII-FR-41)", async () => {
    backend({
      api: API_ALL.map((p) =>
        apiVerified(p, "https://example.com/v1", {
          models: [{ id: "m", label: "m" }],
          selectedModel: "m",
        }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(
      await apiLevel().findByTestId("ai-api-reasoning-stale"),
    ).toBeInTheDocument();
    for (const name of [/Anthropic/, /OpenAI/, /Custom/]) {
      await openApiTab(user, apiLevel(), name);
      expect(
        apiLevel().queryByTestId("ai-api-reasoning-stale"),
        `${name} carries no reasoning notice`,
      ).not.toBeInTheDocument();
    }
  });
});
