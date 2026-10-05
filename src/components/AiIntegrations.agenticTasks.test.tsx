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

import {
  AGENTIC_TURN_KINDS,
  type AgenticIntegration,
  type AgenticVendorId,
} from "../types";
import {
  AGENTIC_ALL,
  AGENTIC_NAMES,
  BothLevels,
  agenticIntegration,
  agenticLevel,
  argsOfFor,
  backendFor,
  cliVerified,
  commandsFor,
  optionsOf,
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
// The Agentic level
// ---------------------------------------------------------------------------

describe("the Agentic level", () => {
  it("applies a model and an effort at once, with no save in between (AII-FR-23, AII-FR-24)", async () => {
    // AII-FR-23 / FR-24.
    let list = [
      cliVerified("claude_code", "/usr/bin/claude", { modelsOrigin: "probed" }),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ];
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_model: (args) => {
          list = list.map((i) =>
            i.vendor === "claude_code"
              ? { ...i, selectedModel: (args?.modelId as string) ?? null }
              : i,
          );
          return list[0];
        },
        set_agentic_integration_effort: (args) => {
          list = list.map((i) =>
            i.vendor === "claude_code"
              ? { ...i, selectedEffort: (args?.effortId as string) ?? null }
              : i,
          );
          return list[0];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await agenticLevel().findByRole("combobox", { name: "Model" });
    expect(await optionsOf(user, agenticLevel(), "Model")).toEqual([
      "Backend default",
      "Opus",
      "Sonnet",
    ]);
    expect(agenticLevel().getByTestId("agentic-models-origin")).toHaveTextContent(
      "from the installed CLI",
    );

    await user.click(
      within(agenticLevel().getByRole("listbox", { name: "Model" })).getByRole(
        "option",
        { name: "Opus" },
      ),
    );
    await user.selectOptions(
      agenticLevel().getByLabelText("Reasoning effort"),
      "high",
    );

    await waitFor(() => {
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        // AII-FR-23: the selector above the per-task rows names no task, which
        // is what sets the default every task falls back to.
        turnKind: null,
        modelId: "opus",
      });
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "claude_code",
        // AII-FR-24: the selector above the per-task rows names no task, which
        // is what sets the default every task falls back to.
        turnKind: null,
        effortId: "high",
      });
    });
    // No detection in this set: the fixture's CLI already has a stored path, so
    // AII-FR-17 correctly leaves it alone.
    expect(new Set(commands())).toEqual(
      new Set([
        "list_ai_api_integrations",
        "list_agentic_integrations",
        "set_agentic_integration_model",
        "set_agentic_integration_effort",
      ]),
    );
  });

  it("holds a reasoning effort per task, over one default (AII-FR-24, AII-FR-PMZK)", async () => {
    // AII-FR-24: one row per kind of work a run hands to this backend, each
    // following the effort above it until it is given one of its own.
    let list = [
      cliVerified("claude_code", "/usr/bin/claude", { selectedEffort: "high" }),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ];
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_effort: (args) => {
          const turnKind = (args?.turnKind as string | null) ?? null;
          const effortId = (args?.effortId as string | null) ?? null;
          list = list.map((i) => {
            if (i.vendor !== "claude_code") return i;
            if (turnKind === null) return { ...i, selectedEffort: effortId };
            const overrides = { ...i.effortOverrides };
            // A null effort against a named task **clears** rather than stores
            // a null, so the row returns to the default.
            if (effortId === null) delete overrides[turnKind];
            else overrides[turnKind] = effortId;
            return { ...i, effortOverrides: overrides };
          });
          return list[0];
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    // Closed until the author opens it, and the summary counts rather than
    // names — so a backend configured one way for everything reads as the two
    // defaults and one closed line (AII-FR-PMZK).
    const summary = await agenticLevel().findByTestId("agentic-per-task-summary");
    expect(summary).toHaveTextContent("3 follow the defaults");
    await user.click(summary);

    await user.selectOptions(agenticLevel().getByLabelText("Review effort"), "low");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "claude_code",
        turnKind: "review",
        effortId: "low",
      });
    });
    await waitFor(() => {
      expect(
        agenticLevel().getByTestId("agentic-per-task-summary"),
      ).toHaveTextContent("1 of 3 differs from the defaults");
    });
    // Every other row still follows the effort default above it, and still does
    // after the tab is left and re-entered — the rows read the record rather
    // than any state of their own.
    for (const other of ["Work", "Reconciliation"]) {
      expect(agenticLevel().getByLabelText(`${other} effort`)).toHaveValue("");
    }
    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    await agenticLevel().findByTestId("agentic-per-task");
    expect(agenticLevel().getByLabelText("Review effort")).toHaveValue("low");
    for (const other of ["Work", "Reconciliation"]) {
      expect(agenticLevel().getByLabelText(`${other} effort`)).toHaveValue("");
    }

    await user.selectOptions(agenticLevel().getByLabelText("Review effort"), "");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "claude_code",
        turnKind: "review",
        effortId: null,
      });
    });
    await waitFor(() => {
      expect(
        agenticLevel().getByTestId("agentic-per-task-summary"),
      ).toHaveTextContent("3 follow the defaults");
    });
  });

  it("renders no effort selector for a vendor that declares none (AII-FR-24)", async () => {
    // AII-FR-24.
    backend({
      agentic: AGENTIC_ALL.map((v) =>
        agenticIntegration(v, v === "opencode" ? { reasoningEfforts: [] } : {}),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    expect(
      await agenticLevel().findByLabelText("Reasoning effort"),
    ).toBeInTheDocument();
    await user.click(agenticLevel().getByRole("tab", { name: /OpenCode/ }));
    await screen.findByTestId("agentic-panel-opencode");
    expect(
      agenticLevel().queryByLabelText("Reasoning effort"),
    ).not.toBeInTheDocument();
    // The per-task section loses its effort column with the default above it,
    // and puts no disabled control in its place (AII-FR-24).
    await user.click(agenticLevel().getByTestId("agentic-per-task-summary"));
    expect(
      agenticLevel().queryByLabelText("Review effort"),
    ).not.toBeInTheDocument();
  });

  /**
   * A backend that keeps both override maps, so a test can watch one move and
   * the other stand still (AII-FR-QDLW).
   *
   * It is the whole of what the two operations do for one vendor: a null task
   * kind writes the default, a named one writes that kind's entry, and a null
   * value against a named kind removes the entry rather than storing a null.
   */
  function overridingBackend(seed: AgenticIntegration[]) {
    let list = seed;
    const update = (
      args: Record<string, unknown> | undefined,
      map: "modelOverrides" | "effortOverrides",
      selection: "selectedModel" | "selectedEffort",
      valueKey: "modelId" | "effortId",
    ) => {
      const turnKind = (args?.turnKind as string | null) ?? null;
      const value = (args?.[valueKey] as string | null) ?? null;
      const vendor = args?.vendor as AgenticVendorId;
      list = list.map((i) => {
        if (i.vendor !== vendor) return i;
        if (turnKind === null) return { ...i, [selection]: value };
        const overrides = { ...i[map] };
        if (value === null) delete overrides[turnKind];
        else overrides[turnKind] = value;
        return { ...i, [map]: overrides };
      });
      return list.find((i) => i.vendor === vendor);
    };
    backend(
      { agentic: list },
      {
        list_agentic_integrations: () => list,
        set_agentic_integration_model: (args) =>
          update(args, "modelOverrides", "selectedModel", "modelId"),
        set_agentic_integration_effort: (args) =>
          update(args, "effortOverrides", "selectedEffort", "effortId"),
      },
    );
    return () => list;
  }

  /** Choose `option` in the per-task model row of `kind` (AII-FR-23). */
  async function chooseTaskModel(
    user: ReturnType<typeof userEvent.setup>,
    row: string,
    option: string,
  ) {
    await user.click(agenticLevel().getByRole("combobox", { name: `${row} model` }));
    await user.click(
      within(
        agenticLevel().getByRole("listbox", { name: `${row} model` }),
      ).getByRole("option", { name: option }),
    );
  }

  it("holds a model per task, over one default (AII-FR-23, AII-FR-PMZK)", async () => {
    // AII-FR-23 / AII-FR-PMZK: one row per kind of work a run hands to this
    // backend, each following the model default above it until it is given one
    // of its own.
    overridingBackend([
      cliVerified("claude_code", "/usr/bin/claude", { selectedModel: "opus" }),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ]);
    render(<BothLevels />);
    const user = userEvent.setup();

    // Closed until the author opens it, and the summary counts rather than
    // names.
    const section = await agenticLevel().findByTestId("agentic-per-task");
    expect(section).not.toHaveAttribute("open");
    const summary = agenticLevel().getByTestId("agentic-per-task-summary");
    expect(summary).toHaveTextContent("Per task · 3 follow the defaults");
    await user.click(summary);
    // The rows live inside the disclosure rather than beside it, so the click
    // is what reveals them. jsdom queries a closed `<details>` as readily as an
    // open one, which is why this is asserted rather than assumed.
    expect(section).toHaveAttribute("open");

    // A row's model selector offers "same as default" first, then the same
    // options the default at the head of its column offers.
    expect(await optionsOf(user, agenticLevel(), "Work model")).toEqual([
      "Same as default",
      "Opus",
      "Sonnet",
    ]);
    await user.keyboard("{Escape}");

    await chooseTaskModel(user, "Work", "Sonnet");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        turnKind: "work",
        modelId: "sonnet",
      });
    });
    await waitFor(() => {
      expect(
        agenticLevel().getByTestId("agentic-per-task-summary"),
      ).toHaveTextContent("1 of 3 differs from the defaults");
    });
    // Every other row still follows the model default, and still does after the
    // tab is left and re-entered — the rows read the record rather than any
    // state of their own.
    for (const other of ["Review", "Reconciliation"]) {
      expect(
        agenticLevel().getByRole("combobox", { name: `${other} model` }),
      ).toHaveTextContent("Same as default");
    }
    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    await user.click(
      await agenticLevel().findByTestId("agentic-per-task-summary"),
    );
    expect(
      agenticLevel().getByRole("combobox", { name: "Work model" }),
    ).toHaveTextContent("Sonnet");
    for (const other of ["Review", "Reconciliation"]) {
      expect(
        agenticLevel().getByRole("combobox", { name: `${other} model` }),
      ).toHaveTextContent("Same as default");
    }

    // "Same as default" invokes the same operation naming that task with no
    // model, which returns the row to the default.
    await chooseTaskModel(user, "Work", "Same as default");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        turnKind: "work",
        modelId: null,
      });
    });
    await waitFor(() => {
      expect(
        agenticLevel().getByTestId("agentic-per-task-summary"),
      ).toHaveTextContent("3 follow the defaults");
    });
  });

  it("names the canonical task kind of both selectors of a row (AII-FR-QDLW, AII-FR-23, AII-FR-24)", async () => {
    // AII-FR-QDLW / AII-FR-23 / AII-FR-24: both selectors of the row the author
    // reads as Reconciliation name `semantic_rebase`, which is the kind the
    // backend is keyed by.
    const current = overridingBackend([
      cliVerified("claude_code", "/usr/bin/claude", {
        selectedModel: "opus",
        selectedEffort: "high",
      }),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ]);
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(await agenticLevel().findByTestId("agentic-per-task-summary"));

    // The section holds the three rows in the order a run reaches them, and each
    // row is that kind's model selector and effort selector side by side — so
    // reading down the comboboxes walks a row at a time rather than a column.
    expect(
      within(agenticLevel().getByTestId("agentic-per-task"))
        .getAllByRole("combobox")
        .map((c) => c.getAttribute("aria-label")),
    ).toEqual([
      "Work model",
      "Work effort",
      "Review model",
      "Review effort",
      "Reconciliation model",
      "Reconciliation effort",
    ]);

    await chooseTaskModel(user, "Reconciliation", "Sonnet");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        turnKind: "semantic_rebase",
        modelId: "sonnet",
      });
    });
    await user.selectOptions(
      agenticLevel().getByLabelText("Reconciliation effort"),
      "low",
    );
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "claude_code",
        turnKind: "semantic_rebase",
        effortId: "low",
      });
    });

    // The record comes back holding both, both selectors of the row show their
    // own value, and the summary counts that one row once rather than twice
    // (AII-FR-PMZK).
    await waitFor(() => {
      expect(current()[0].modelOverrides).toEqual({ semantic_rebase: "sonnet" });
      expect(current()[0].effortOverrides).toEqual({ semantic_rebase: "low" });
    });
    expect(
      agenticLevel().getByRole("combobox", { name: "Reconciliation model" }),
    ).toHaveTextContent("Sonnet");
    expect(agenticLevel().getByLabelText("Reconciliation effort")).toHaveValue("low");
    expect(
      agenticLevel().getByTestId("agentic-per-task-summary"),
    ).toHaveTextContent("1 of 3 differs from the defaults");

    // Set back to "same as default", each operation names the same kind with no
    // value and both selectors read "same as default" again.
    await chooseTaskModel(user, "Reconciliation", "Same as default");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        turnKind: "semantic_rebase",
        modelId: null,
      });
    });
    await user.selectOptions(
      agenticLevel().getByLabelText("Reconciliation effort"),
      "",
    );
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "claude_code",
        turnKind: "semantic_rebase",
        effortId: null,
      });
    });
    await waitFor(() => {
      expect(
        agenticLevel().getByRole("combobox", { name: "Reconciliation model" }),
      ).toHaveTextContent("Same as default");
    });
    expect(agenticLevel().getByLabelText("Reconciliation effort")).toHaveValue("");
  });

  it("gives a row's two selectors no shared state (AII-FR-QDLW, AII-FR-PMZK)", async () => {
    // AII-FR-QDLW: a row's model selector and effort selector are independent
    // of each other and of every other row, so returning one to "same as
    // default" returns that kind of work to one default and never to two.
    // AII-FR-PMZK: a row that overrides both counts once, not twice.
    overridingBackend([
      cliVerified("claude_code", "/usr/bin/claude", {
        selectedModel: "opus",
        selectedEffort: "high",
        modelOverrides: { work: "sonnet" },
        effortOverrides: { review: "low", work: "low" },
      }),
      ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
    ]);
    render(<BothLevels />);
    const user = userEvent.setup();

    const summary = () => agenticLevel().getByTestId("agentic-per-task-summary");
    const modelOf = (row: string) =>
      agenticLevel().getByRole("combobox", { name: `${row} model` });
    const effortOf = (row: string) => agenticLevel().getByLabelText(`${row} effort`);

    await agenticLevel().findByTestId("agentic-per-task");
    // Two kinds of work are configured apart — Work in both dimensions and
    // Review in its effort alone — and the summary counts kinds of work rather
    // than selectors touched.
    expect(summary()).toHaveTextContent("2 of 3 differ from the defaults");
    await user.click(summary());

    // A row overriding one dimension leaves the other reading the default
    // beside it: Work overrides both, Review its effort alone.
    expect(modelOf("Work")).toHaveTextContent("Sonnet");
    expect(effortOf("Work")).toHaveValue("low");
    expect(modelOf("Review")).toHaveTextContent("Same as default");
    expect(effortOf("Review")).toHaveValue("low");
    expect(modelOf("Reconciliation")).toHaveTextContent("Same as default");
    expect(effortOf("Reconciliation")).toHaveValue("");

    // The model selector of a row overriding both, set back on its own: only
    // the model operation is invoked, the effort beside it is untouched, and
    // the row still counts because it still differs.
    const effortCallsBefore = commands().filter(
      (c) => c === "set_agentic_integration_effort",
    ).length;
    await chooseTaskModel(user, "Work", "Same as default");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_model")).toEqual({
        vendor: "claude_code",
        turnKind: "work",
        modelId: null,
      });
    });
    expect(
      commands().filter((c) => c === "set_agentic_integration_effort").length,
    ).toBe(effortCallsBefore);
    await waitFor(() => {
      expect(modelOf("Work")).toHaveTextContent("Same as default");
    });
    expect(effortOf("Work")).toHaveValue("low");
    expect(summary()).toHaveTextContent("2 of 3 differ from the defaults");

    // The effort selector of that same row set back too returns the kind of
    // work to both defaults, and only then does the count fall. Independence
    // holds in this direction as well: no model operation rides along, and the
    // The Work row's model is untouched by a change to its own effort.
    const modelCallsBefore = commands().filter(
      (c) => c === "set_agentic_integration_model",
    ).length;
    await user.selectOptions(effortOf("Work"), "");
    await waitFor(() => {
      expect(argsOf("set_agentic_integration_effort")).toEqual({
        vendor: "claude_code",
        turnKind: "work",
        effortId: null,
      });
    });
    expect(
      commands().filter((c) => c === "set_agentic_integration_model").length,
    ).toBe(modelCallsBefore);
    // The model beside it reads what it was left at, untouched by the effort
    // operation that just ran.
    expect(modelOf("Work")).toHaveTextContent("Same as default");
    await waitFor(() => {
      expect(summary()).toHaveTextContent("1 of 3 differs from the defaults");
    });

    // Left and re-entered, the section counts the record rather than any state
    // of its own.
    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    await user.click(agenticLevel().getByRole("tab", { name: /Claude Code/ }));
    await agenticLevel().findByTestId("agentic-per-task");
    expect(summary()).toHaveTextContent("1 of 3 differs from the defaults");
  });

  it("gives every agentic vendor one per-task section and only some an effort column (AII-FR-23, AII-FR-24, AII-FR-QDLW, AII-FR-PMZK)", async () => {
    // AII-FR-23 / AII-FR-24 / AII-FR-QDLW / AII-FR-PMZK: every agentic tab
    // renders one per-task section of three rows; a vendor that declares no
    // effort levels renders neither the effort default nor the effort column,
    // while its model column renders as in every other tab.
    backend({
      agentic: AGENTIC_ALL.map((v) =>
        v === "opencode"
          ? cliVerified(v, "/usr/bin/opencode", { reasoningEfforts: [] })
          : v === "claude_code"
            ? cliVerified(v, "/usr/bin/claude")
            : agenticIntegration(v, { state: "verified" }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();
    await agenticLevel().findByTestId("agentic-panel-claude_code");

    for (const vendor of AGENTIC_ALL) {
      await user.click(
        agenticLevel().getByRole("tab", {
          name: new RegExp(AGENTIC_NAMES[vendor]),
        }),
      );
      await agenticLevel().findByTestId(`agentic-panel-${vendor}`);

      // One section, in every tab, below the defaults above it. Its summary
      // reads out of five whichever vendor the tab is for.
      const modelDefault = agenticLevel().getByRole("combobox", {
        name: "Model",
      });
      const section = agenticLevel().getByTestId("agentic-per-task");
      expect(
        agenticLevel().getByTestId("agentic-per-task-summary"),
      ).toHaveTextContent("3 follow the defaults");
      // Below both defaults rather than under either one of them — the whole
      // point of the merge, and the one thing a row-level assertion cannot see.
      expect(
        modelDefault.compareDocumentPosition(section) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      const grid = section.querySelector(".agentic-per-task__grid");
      const headings = Array.from(
        section.querySelectorAll(".agentic-per-task__heading"),
      ).map((h) => h.textContent);
      const labels = () =>
        within(section)
          .getAllByRole("combobox")
          .map((r) => r.getAttribute("aria-label"));

      if (vendor === "opencode") {
        // The one vendor that declares no levels: no effort default and no
        // effort column, and nothing standing in for one. The grid drops a
        // track and the heading goes with it, so the row genuinely closes up
        // rather than leaving a cell the author cannot use.
        expect(
          agenticLevel().queryByLabelText("Reasoning effort"),
        ).not.toBeInTheDocument();
        expect(headings).toEqual(["Model"]);
        expect(grid).toHaveClass("agentic-per-task__grid--no-effort");
        expect(section.querySelectorAll(".agentic-per-task__cell")).toHaveLength(
          AGENTIC_TURN_KINDS.length,
        );
        expect(labels()).toEqual([
          "Work model",
          "Review model",
          "Reconciliation model",
        ]);
        continue;
      }
      expect(
        agenticLevel().getByLabelText("Reasoning effort"),
      ).toBeInTheDocument();
      expect(headings).toEqual(["Model", "Effort"]);
      expect(grid).not.toHaveClass("agentic-per-task__grid--no-effort");
      expect(section.querySelectorAll(".agentic-per-task__cell")).toHaveLength(
        AGENTIC_TURN_KINDS.length * 2,
      );
      expect(labels()).toEqual([
        "Work model",
        "Work effort",
        "Review model",
        "Review effort",
        "Reconciliation model",
        "Reconciliation effort",
      ]);
    }
  });

  it("counts only the per-task selectors it renders (AII-FR-PMZK, AII-FR-24)", async () => {
    // AII-FR-PMZK: the summary counts what the section renders, so it and the
    // rows beneath it always agree. A vendor that declares no effort levels can
    // still hold a stored effort override — the backend keys its maps on the
    // turn kind alone and keeps a selection through a degradation (AIC-FR-10,
    // AII-FR-27) — and an override the author is shown no selector for is
    // counted by neither the summary nor a row. It is held, not lost.
    const current = overridingBackend([
      cliVerified("opencode", "/usr/bin/opencode", {
        reasoningEfforts: [],
        selectedModel: "opus",
        effortOverrides: { review: "low" },
      }),
      ...AGENTIC_ALL.filter((v) => v !== "opencode").map((v) =>
        agenticIntegration(v),
      ),
    ]);
    render(<BothLevels />);
    const user = userEvent.setup();
    // Seeded first, so its tab is the one the level opens on.
    await agenticLevel().findByTestId("agentic-panel-opencode");

    // The hidden override moves neither the summary nor the row it belongs to.
    const summary = () => agenticLevel().getByTestId("agentic-per-task-summary");
    expect(summary()).toHaveTextContent("3 follow the defaults");
    await user.click(summary());
    expect(
      agenticLevel().queryByLabelText("Reasoning effort"),
    ).not.toBeInTheDocument();
    for (const row of ["Work", "Review", "Reconciliation"]) {
      expect(
        agenticLevel().getByRole("combobox", { name: `${row} model` }),
      ).toHaveTextContent("Same as default");
      // Nothing stands where the effort column would be — not a control, and
      // not a disabled one either.
      expect(
        agenticLevel().queryByLabelText(`${row} effort`),
      ).not.toBeInTheDocument();
    }

    // Held rather than cleared: the section writes nothing on its account.
    await user.click(agenticLevel().getByRole("tab", { name: /Codex/ }));
    await user.click(agenticLevel().getByRole("tab", { name: /OpenCode/ }));
    await agenticLevel().findByTestId("agentic-panel-opencode");
    expect(commands()).not.toContain("set_agentic_integration_effort");
    expect(
      current().find((i) => i.vendor === "opencode")?.effortOverrides,
    ).toEqual({ review: "low" });

    // A model override in the same record is counted, because that column is
    // rendered — so the count reads out of four and reports what is on screen.
    await chooseTaskModel(user, "Review", "Sonnet");
    await waitFor(() => {
      expect(summary()).toHaveTextContent("1 of 3 differs from the defaults");
    });
    expect(
      agenticLevel().getByRole("combobox", { name: "Review model" }),
    ).toHaveTextContent("Sonnet");
  });

  it("keeps a refused task effort on screen and says why (AII-FR-28)", async () => {
    // AII-FR-28, on the effort half of a row. It is mechanically unlike the
    // model half: a native `<select>` holds a DOM value the record does not, so
    // only a re-render can put the refused row back the way it was.
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/usr/bin/claude", {
            selectedModel: "opus",
            selectedEffort: "high",
          }),
          ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
        ],
      },
      {
        set_agentic_integration_effort: () => {
          throw "unknown_effort";
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(await agenticLevel().findByTestId("agentic-per-task-summary"));
    await user.selectOptions(agenticLevel().getByLabelText("Review effort"), "low");

    const error = await agenticLevel().findByTestId("agentic-action-error");
    expect(error).toHaveTextContent(/reasoning effort/i);
    // The selector snaps back, its neighbour is untouched, and the summary
    // counts what it counted.
    await waitFor(() => {
      expect(agenticLevel().getByLabelText("Review effort")).toHaveValue("");
    });
    expect(
      agenticLevel().getByRole("combobox", { name: "Review model" }),
    ).toHaveTextContent("Same as default");
    expect(
      agenticLevel().getByTestId("agentic-per-task-summary"),
    ).toHaveTextContent("3 follow the defaults");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps a refused task model on screen and says why beside it (AII-FR-28)", async () => {
    // AII-FR-28: a rejected update leaves the previous value showing, leaves the
    // effort selector beside it untouched, leaves the summary count where it
    // was, and renders the typed error inline — no modal and no transient
    // notification.
    backend(
      {
        agentic: [
          cliVerified("claude_code", "/usr/bin/claude", { selectedModel: "opus" }),
          ...AGENTIC_ALL.slice(1).map((v) => agenticIntegration(v)),
        ],
      },
      {
        set_agentic_integration_model: () => {
          throw "unknown_model";
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.click(await agenticLevel().findByTestId("agentic-per-task-summary"));
    await chooseTaskModel(user, "Review", "Sonnet");

    const error = await agenticLevel().findByTestId("agentic-action-error");
    expect(error).toHaveTextContent(/model/i);
    // The selector reads what it read before, its neighbour is untouched, and
    // the summary counts what it counted.
    expect(
      agenticLevel().getByRole("combobox", { name: "Review model" }),
    ).toHaveTextContent("Same as default");
    expect(agenticLevel().getByLabelText("Review effort")).toHaveValue("");
    expect(
      agenticLevel().getByTestId("agentic-per-task-summary"),
    ).toHaveTextContent("3 follow the defaults");
    // Inline beside the row rather than anywhere that takes the screen.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
