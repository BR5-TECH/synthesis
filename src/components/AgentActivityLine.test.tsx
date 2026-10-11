import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import { ActivityLine, activityLevel, clockOf, kindLabel } from "./AgentActivityLine";

describe("the shared agent-activity row", () => {
  it("RUN-FR-03: the label of a kind reads its underscores as spaces", () => {
    expect(kindLabel("tool_result")).toBe("tool result");
    expect(kindLabel("message")).toBe("message");
  });

  it("RUN-FR-03: a kind that is not known takes the plain level", () => {
    expect(activityLevel("error")).toBe("err");
    expect(activityLevel("unrecognized")).toBe("warn");
    expect(activityLevel("nobody_has_met_this")).toBe("info");
  });

  it("RUN-FR-11: the time is the local time of the instant the record carries", () => {
    const at = "2026-09-06T09:04:31Z";
    const expected = new Date(at).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
    expect(clockOf(at)).toBe(expected);
    expect(clockOf("not a time")).toBe("");
  });

  it("RUN-FR-YQAE: a row has the time, the kind with its level, and what the caller passes as the summary", () => {
    render(
      <ActivityLine at="2026-09-06T09:04:31Z" kind="file_change" title="stdout">
        <span data-testid="summary">edited a file</span>
      </ActivityLine>,
    );
    const label = screen.getByText("file change");
    expect(label).toHaveAttribute("data-level", "ok");
    expect(label).toHaveAttribute("title", "stdout");
    expect(screen.getByTestId("summary")).toHaveTextContent("edited a file");
  });
});
