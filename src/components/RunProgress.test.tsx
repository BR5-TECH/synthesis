/**
 * `RPV-run-progress.md` — the visualization as rendered.
 *
 * The derivation is covered in `../state/runProgress.test.ts`; what is asserted
 * here is what a reader actually gets: a word for every condition, no animation
 * outside `active`, one region for assistive technology, a detail slot nothing
 * interprets, and the same behaviour under a configuration that is not the
 * graduation's.
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { RunProgress } from "./RunProgress";
import type { ProgressStage } from "../state/runProgress";

const FOUR: ProgressStage[] = [
  { id: "queued", label: "Queued", description: "Waiting for the queue" },
  { id: "authoring", label: "Author", description: "Writing the change set" },
  { id: "validation", label: "Validate", description: "Checking it" },
  { id: "acceptance", label: "Accept", description: "Review and publication" },
];

/** A configuration that is nothing to do with a graduation (RPV-FR-03, RPV-FR-02). */
const THREE: ProgressStage[] = [
  { id: "gather", label: "Gather" },
  { id: "sort", label: "Sort" },
  { id: "deliver", label: "Deliver" },
];

const region = () => screen.getByTestId("run-progress");
const stage = (id: string) =>
  region().querySelector<HTMLElement>(`[data-stage="${id}"]`)!;
const spoken = () => within(region()).getByRole("status").textContent ?? "";

afterEach(cleanup);

describe("the run progress visualization (RPV-run-progress.md)", () => {
  it("RPV-FR-02, RPV-FR-04, RPV-FR-05: renders the configured stages in order with one current", () => {
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="validation"
        condition="active"
      />,
    );
    const marks = region().querySelectorAll("[data-stage]");
    expect([...marks].map((m) => m.getAttribute("data-stage"))).toEqual([
      "queued",
      "authoring",
      "validation",
      "acceptance",
    ]);
    expect(stage("queued")).toHaveAttribute("data-status", "complete");
    expect(stage("authoring")).toHaveAttribute("data-status", "complete");
    expect(stage("validation")).toHaveAttribute("data-status", "current");
    expect(stage("acceptance")).toHaveAttribute("data-status", "not_started");
  });

  it("RPV-FR-03, RPV-FR-02: renders a different configuration under the same rules", () => {
    render(
      <RunProgress
        label="Delivery"
        stages={THREE}
        currentStage="sort"
        condition="waiting"
        conditionSentence="Waiting for a slot"
      />,
    );
    expect(stage("gather")).toHaveAttribute("data-status", "complete");
    expect(stage("sort")).toHaveAttribute("data-status", "current");
    expect(stage("deliver")).toHaveAttribute("data-status", "not_started");
    expect(region()).toHaveAttribute("data-total", "3");
    // No graduation label, stage name, or state anywhere in it.
    for (const word of ["Queued", "Author", "Validate", "Accept", "iteration"]) {
      expect(region().textContent).not.toContain(word);
    }
  });

  it("RPV-FR-04: guesses nothing when the current stage is not configured", () => {
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="somewhere-else"
        condition="active"
      />,
    );
    for (const id of ["queued", "authoring", "validation", "acceptance"]) {
      expect(stage(id)).toHaveAttribute("data-status", "not_started");
    }
    expect(region()).toHaveAttribute("data-animates", "false");
  });

  it("RPV-FR-06, RPV-FR-05: completes every stage only for complete on the last", () => {
    const { unmount } = render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="acceptance"
        condition="complete"
      />,
    );
    for (const id of ["queued", "authoring", "validation", "acceptance"]) {
      expect(stage(id)).toHaveAttribute("data-status", "complete");
    }
    unmount();

    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="complete"
      />,
    );
    // Its own stage is done; the ones after it are not, so the work never
    // reads as wholly finished before its last stage says so.
    expect(stage("authoring")).toHaveAttribute("data-status", "complete");
    expect(stage("validation")).toHaveAttribute("data-status", "not_started");
    expect(stage("acceptance")).toHaveAttribute("data-status", "not_started");
  });

  it("RPV-FR-05, RPV-FR-12: a stopped run states its outcome and completes nothing after it", () => {
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="validation"
        condition="stopped"
        conditionSentence="Stopped"
        outcome="Discarded by you"
      />,
    );
    expect(stage("acceptance")).toHaveAttribute("data-status", "not_started");
    expect(region().textContent).toContain("Discarded by you");
    expect(region()).toHaveAttribute("data-animates", "false");
  });

  it("RPV-FR-07: animates for active alone", () => {
    for (const condition of [
      "waiting",
      "paused",
      "blocked",
      "stopped",
      "complete",
    ] as const) {
      const { unmount } = render(
        <RunProgress
          label="Progress"
          stages={FOUR}
          currentStage="validation"
          condition={condition}
        />,
      );
      expect(region()).toHaveAttribute("data-animates", "false");
      unmount();
    }
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="validation"
        condition="active"
      />,
    );
    expect(region()).toHaveAttribute("data-animates", "true");
  });

  it("RPV-FR-08: carries every condition in words, and falls back to the condition's own", () => {
    const { unmount } = render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="acceptance"
        condition="waiting"
        conditionSentence="Waiting for your review"
      />,
    );
    expect(stage("acceptance").textContent).toContain("Waiting for your review");
    expect(spoken()).toContain("Waiting for your review");
    unmount();

    // A condition with no sentence still reads as a word — never as a colour,
    // a mark, or a motion alone.
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="acceptance"
        condition="blocked"
      />,
    );
    expect(stage("acceptance").textContent).toContain("Blocked");
    expect(spoken()).toContain("Blocked");
  });

  it("RPV-FR-09: renders an iteration label, and no zero where there is none", () => {
    const { unmount } = render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        iterationLabel="iteration 4"
      />,
    );
    expect(region().textContent).toContain("iteration 4");
    unmount();

    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
      />,
    );
    expect(region().textContent).not.toContain("iteration");
    expect(region().textContent).not.toContain("0");
  });

  it("RPV-FR-10 / RPV-FR-11: renders one indication per backward edge without moving the extent", () => {
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        history={[
          { from: null, to: "queued", iteration: 0 },
          { from: "queued", to: "authoring", iteration: 0 },
          { from: "authoring", to: "validation", iteration: 1 },
          { from: "validation", to: "authoring", iteration: 1 },
          { from: "acceptance", to: "authoring", iteration: 3 },
        ]}
      />,
    );
    const loops = region().querySelectorAll(".run-progress__loop");
    expect(loops.length).toBe(2);
    expect(loops[0].textContent).toContain("Validate → Author");
    expect(loops[0].textContent).toContain("iteration 1");
    expect(loops[1].textContent).toContain("Accept → Author");
    // The extent is the current stage's own position, not the history's length.
    expect(region()).toHaveAttribute("data-reached", "1");
    expect(region()).toHaveAttribute("data-total", "4");
    expect(spoken()).toContain("Went back from Validate to Author");
  });

  it("RPV-FR-11: a host that renders the moves itself gets none of its own", () => {
    // The component still reads the history — that is what a backward edge is
    // decided from — but a host rendering the same passes in its own detail
    // region says so, and the moves are read once rather than twice on one
    // surface. The announcement drops them for the same reason: a screen reader
    // hearing the passes twice is the same defect said out loud.
    const history = [
      { from: "validation", to: "authoring", iteration: 1 },
      { from: "acceptance", to: "authoring", iteration: 3 },
    ];
    const { rerender } = render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        history={history}
        showLoops={false}
      />,
    );
    expect(region().querySelectorAll(".run-progress__loop")).toHaveLength(0);
    expect(spoken()).not.toContain("Went back");
    // And nothing else about the row changed with it.
    expect(region()).toHaveAttribute("data-reached", "1");

    // Left to itself, the component still states them — the suppression is the
    // host's request rather than the component having stopped doing it.
    rerender(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        history={history}
      />,
    );
    expect(region().querySelectorAll(".run-progress__loop")).toHaveLength(2);
    expect(spoken()).toContain("Went back from Validate to Author");
  });

  it("RPV-FR-13: renders the host's detail and interprets none of it", () => {
    const { unmount } = render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        detail={<p data-testid="host-detail"># not a heading</p>}
      />,
    );
    expect(screen.getByTestId("host-detail").textContent).toBe(
      "# not a heading",
    );
    expect(screen.queryByRole("heading")).toBeNull();
    unmount();

    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
      />,
    );
    expect(screen.queryByTestId("host-detail")).toBeNull();
  });

  it("RPV-FR-14, RPV-FR-15: reads as one region naming the stages, the condition, the iteration and the outcome", () => {
    render(
      <RunProgress
        label="Progress of “thing”"
        stages={FOUR}
        currentStage="acceptance"
        condition="complete"
        iterationLabel="iteration 2"
        outcome="Published as a commit"
        history={[{ from: "validation", to: "authoring", iteration: 1 }]}
      />,
    );
    const said = spoken();
    expect(said).toContain("Progress of “thing”");
    expect(said).toContain("Waiting for the queue: done.");
    expect(said).toContain("Review and publication: done.");
    expect(said).toContain("iteration 2");
    expect(said).toContain("Published as a commit");
    // One live region for the whole thing rather than a scatter of them.
    expect(within(region()).getAllByRole("status")).toHaveLength(1);
    // The marks and labels say the same thing less well, so they are hidden.
    expect(region().querySelector("ol")).toHaveAttribute("aria-hidden", "true");
  });

  it("RPV-FR-18, RPV-FR-01: renders with no application state and invokes nothing", () => {
    // No `invoke` mock and no event mock is installed in this file at all: a
    // component that reached for either would fail to render here.
    render(
      <RunProgress
        label="Progress"
        stages={THREE}
        currentStage="gather"
        condition="active"
      />,
    );
    expect(region()).toBeInTheDocument();
  });

  it("RPV-FR-07: renders every condition's own word when the host supplies no sentence", () => {
    const expected: [Parameters<typeof RunProgress>[0]["condition"], string][] = [
      ["active", "Working"],
      ["waiting", "Waiting"],
      ["paused", "Paused"],
      ["blocked", "Blocked"],
      ["stopped", "Stopped"],
      ["complete", "Done"],
    ];
    for (const [condition, word] of expected) {
      const { unmount } = render(
        <RunProgress
          label="Progress"
          stages={FOUR}
          currentStage="validation"
          condition={condition}
        />,
      );
      // On screen and in the announcement alike — never a colour, a mark, or
      // a motion alone.
      expect(stage("validation").textContent, condition).toContain(word);
      // The announcement says the same thing as a sentence, so a completed
      // stage reads "done." there and "Done" on the row.
      expect(spoken().toLowerCase(), condition).toContain(word.toLowerCase());
      unmount();
    }
  });

  it("RPV-FR-14, RPV-FR-15: renders an outcome only for work that has ended", () => {
    for (const condition of ["active", "waiting", "paused", "blocked"] as const) {
      const { unmount } = render(
        <RunProgress
          label="Progress"
          stages={FOUR}
          currentStage="acceptance"
          condition={condition}
          outcome="Published as a commit"
        />,
      );
      expect(region().textContent, condition).not.toContain(
        "Published as a commit",
      );
      unmount();
    }
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="acceptance"
        condition="complete"
        outcome="Published as a commit"
      />,
    );
    expect(region().textContent).toContain("Published as a commit");
  });

  it("RPV-FR-18, RPV-FR-01: shows what it was last passed and retains nothing of the render before", () => {
    const { rerender } = render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        iterationLabel="iteration 1"
        history={[{ from: "validation", to: "authoring", iteration: 1 }]}
      />,
    );
    expect(stage("authoring")).toHaveAttribute("data-status", "current");
    expect(region().querySelectorAll(".run-progress__loop")).toHaveLength(1);

    rerender(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="acceptance"
        condition="waiting"
        conditionSentence="Waiting for your review"
      />,
    );
    expect(stage("acceptance")).toHaveAttribute("data-status", "current");
    expect(stage("authoring")).toHaveAttribute("data-status", "complete");
    // The previous render's loop and iteration are gone rather than kept.
    expect(region().querySelectorAll(".run-progress__loop")).toHaveLength(0);
    expect(region().textContent).not.toContain("iteration 1");
    // RPV-FR-14, RPV-FR-15: and the change is what the live region now says.
    expect(spoken()).toContain("Waiting for your review");
  });

  it("RPV-FR-14, RPV-FR-15: adds no focus stop of its own", () => {
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="authoring"
        condition="active"
        history={[{ from: "validation", to: "authoring", iteration: 1 }]}
      />,
    );
    // The component holds no control, so it takes no place in the host's tab
    // order — anything focusable inside it came from the host's detail slot.
    expect(
      region().querySelectorAll("button, a, input, [tabindex]"),
    ).toHaveLength(0);
  });

  it("RPV-FR-JSQW, RPV-FR-14: a configuration with no activatable stage adds no control", () => {
    render(
      <RunProgress
        label="Progress"
        stages={FOUR}
        currentStage="validation"
        condition="active"
      />,
    );
    expect(within(region()).queryAllByRole("button")).toHaveLength(0);
  });

  it("RPV-FR-LQUP: an activatable stage is a keyboard-operable control naming its stage", async () => {
    const activated: string[] = [];
    render(
      <RunProgress
        label="Progress"
        stages={FOUR.map((stage) =>
          stage.id === "validation" ? { ...stage, activatable: true } : stage,
        )}
        currentStage="validation"
        condition="active"
        onActivateStage={(id) => activated.push(id)}
      />,
    );
    const entry = screen.getByTestId("run-progress-stage-validation");
    expect(entry).toHaveAccessibleName(/Validate/);
    entry.focus();
    await userEvent.keyboard("{Enter}");
    expect(activated).toEqual(["validation"]);
  });

  it("RPV-FR-MAIP, RPV-FR-TZBL: an inactivatable stage keeps its entry and says in words why it does nothing", async () => {
    const activated: string[] = [];
    render(
      <RunProgress
        label="Progress"
        stages={FOUR.map((stage) =>
          stage.id === "validation"
            ? { ...stage, activatable: true }
            : { ...stage, disabledReason: `${stage.label} wrote no log yet.` },
        )}
        currentStage="validation"
        condition="active"
        onActivateStage={(id) => activated.push(id)}
      />,
    );
    const disabled = screen.getByTestId("run-progress-stage-queued");
    expect(disabled).toHaveAttribute("aria-disabled", "true");
    expect(disabled).toHaveAccessibleName(/Queued wrote no log yet\./);
    // It keeps its mark, its label, and its condition.
    expect(within(disabled).getByText("Queued")).toBeInTheDocument();
    expect(disabled.querySelector(".run-progress__mark")).not.toBeNull();
    await userEvent.click(disabled);
    expect(activated).toEqual([]);
    // RPV-FR-TZBL: what it is and that it is disabled are carried in words and
    // in accessible semantics rather than by a treatment alone.
    expect(disabled).toHaveAttribute("aria-disabled", "true");
    expect(disabled.getAttribute("aria-label")).toMatch(/wrote no log yet/);
  });

  it("RPV-FR-NEVJ, RPV-FR-WVPV: activating reports it and changes nothing else about the row", async () => {
    const activated: string[] = [];
    render(
      <RunProgress
        label="Progress"
        stages={FOUR.map((stage) => ({ ...stage, activatable: true }))}
        currentStage="validation"
        condition="active"
        onActivateStage={(id) => activated.push(id)}
      />,
    );
    await userEvent.click(screen.getByTestId("run-progress-stage-queued"));
    expect(activated).toEqual(["queued"]);
    // Completion is still positional and the current stage has not moved.
    expect(region()).toHaveAttribute("data-reached", "2");
    expect(stage("validation")).toHaveAttribute("data-status", "current");
    expect(stage("queued")).toHaveAttribute("data-status", "complete");
  });

  it("RPV-FR-RTQB, RPV-FR-03: which stages are activatable is the host's data, at any stage count", () => {
    render(
      <RunProgress
        label="Progress"
        stages={THREE.map((stage) =>
          stage.id === "sort"
            ? { ...stage, activatable: true }
            : { ...stage, disabledReason: `${stage.label} holds nothing.` },
        )}
        currentStage="sort"
        condition="active"
      />,
    );
    expect(within(region()).getAllByRole("button")).toHaveLength(3);
    expect(screen.getByTestId("run-progress-stage-sort")).not.toHaveAttribute(
      "aria-disabled",
    );
    expect(screen.getByTestId("run-progress-stage-gather")).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("RPV-FR-MAIP: a stage with no reason for being inactivatable is plain text, not a control", () => {
    render(
      <RunProgress
        label="Progress"
        stages={THREE.map((stage) =>
          stage.id === "sort" ? { ...stage, activatable: true } : stage,
        )}
        currentStage="sort"
        condition="active"
      />,
    );
    // Only the activatable stage is a control; the two the host said nothing
    // about are read as text rather than announced as controls that do nothing.
    expect(within(region()).getAllByRole("button")).toHaveLength(1);
    expect(screen.getByTestId("run-progress-stage-sort")).toBeInTheDocument();
    expect(screen.queryByTestId("run-progress-stage-gather")).toBeNull();
    // They keep their mark, their label, and their condition either way.
    expect(within(stage("gather")).getByText("Gather")).toBeInTheDocument();
    expect(stage("gather").querySelector(".run-progress__mark")).not.toBeNull();
  });

  it("RPV-FR-02: renders a two-stage configuration, the smallest it accepts", () => {
    render(
      <RunProgress
        label="Progress"
        stages={[
          { id: "start", label: "Start" },
          { id: "end", label: "End" },
        ]}
        currentStage="end"
        condition="complete"
      />,
    );
    expect(region()).toHaveAttribute("data-total", "2");
    expect(region()).toHaveAttribute("data-reached", "2");
  });
});

