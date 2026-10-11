import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { clockOf } from "../AgentActivityLine";
import { makeActivityEntry, makeLogPage } from "../../test/graduationFixtures";
import {
  draw,
  installLogBackend,
  type LogBackend,
} from "../../test/graduationLogWindowHarness";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

let backend: LogBackend;

beforeEach(() => {
  backend = installLogBackend();
});

afterEach(cleanup);

const AT = "2026-09-06T09:04:31Z";

describe("a row of the graduation log window", () => {
  it("GLW-FR-KHGP: a row is the local time, the kind with its label, and the one-line summary", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "Read specifications/core/GRD.md", { kind: "tool_call", at: AT })],
      });
    draw();
    const row = await screen.findByTestId("glw-row");
    expect(row.querySelector(".runs-line__ts")).toHaveTextContent(clockOf(AT));
    // The label is the kind with its underscores read as spaces.
    expect(row.querySelector(".runs-line__lvl")).toHaveTextContent("tool call");
    expect(row.querySelector(".runs-line__msg")).toHaveTextContent(
      "Read specifications/core/GRD.md",
    );
    // Three parts, and no part besides them.
    expect(row.querySelectorAll(".runs-line > span")).toHaveLength(3);
    // The local time carries no date and no zone of the stored instant.
    expect(row.textContent).not.toContain("2026-09-06");
    expect(row.textContent).not.toContain("T09:04:31Z");
  });

  it.each([
    ["error", "err"],
    ["diagnostic", "warn"],
    ["retry", "warn"],
    ["file_change", "ok"],
    ["finished", "ok"],
    ["tool_call", "step"],
    ["command", "step"],
    ["message", "info"],
    ["usage", "info"],
    ["a_kind_nobody_has_met", "info"],
  ])("GLW-FR-KHGP: the kind %s takes the %s level, as in the Agent Output section", async (kind, level) => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "text", { kind })],
      });
    draw();
    const row = await screen.findByTestId("glw-row");
    expect(row.querySelector(".runs-line__lvl")).toHaveAttribute("data-level", level);
  });

  it("GLW-FR-JOIG: a row has no expansion and no control, and activating it reveals nothing", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [
          makeActivityEntry(1, "one line", {
            payload: "{\"raw\":\"vendor event\"}",
            payload_truncated: true,
          }),
        ],
      });
    draw();
    const row = await screen.findByTestId("glw-row");
    expect(within(row).queryAllByRole("button")).toHaveLength(0);
    expect(row.querySelector("[aria-expanded]")).toBeNull();
    expect(row.querySelector("[tabindex]")).toBeNull();
    const before = screen.getByTestId("glw-viewport").innerHTML;
    row.click();
    row.querySelector<HTMLElement>(".runs-line__msg")?.click();
    expect(screen.getByTestId("glw-viewport").innerHTML).toBe(before);
    expect(document.querySelector(".runs-payload")).toBeNull();
    expect(screen.getByTestId("glw-viewport").textContent).not.toContain("vendor event");
  });

  it("GLW-FR-HGXL, GLW-FR-JOIG: a record that holds further fields shows none of them", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [
          makeActivityEntry(7, "the summary", {
            kind: "message",
            origin: "origin-marker",
            producer: "producer-marker",
            channel: "stderr",
            record_id: "record-id-marker",
            schema_version: 1,
            payload: "payload-marker",
            data_base64: "ZGF0YS1tYXJrZXI=",
            data: "data-marker",
            task_input: "task-input-marker",
            reasoning: "reasoning-marker",
            tool_argument: "argument-marker",
            fields: { code: "fields-marker" },
            agent: "agent-marker",
            container: "container-marker",
            source: "source-marker",
            level: "level-marker",
            event: "event-marker",
          }),
        ],
      });
    draw();
    const row = await screen.findByTestId("glw-row");
    const text = row.textContent ?? "";
    expect(text).toContain("message");
    expect(text).toContain("the summary");
    for (const marker of [
      "origin-marker",
      "producer-marker",
      "stderr",
      "record-id-marker",
      "payload-marker",
      "ZGF0YS1tYXJrZXI=",
      "data-marker",
      "task-input-marker",
      "reasoning-marker",
      "argument-marker",
      "fields-marker",
      "agent-marker",
      "container-marker",
      "source-marker",
      "level-marker",
      "event-marker",
      "work_turn",
      "executor",
      "rec-7",
      "r1",
    ]) {
      expect(text).not.toContain(marker);
    }
    // The sequence orders the rows and is not shown either.
    expect(text).not.toMatch(/#7|\b7\b/);
    const attributes = Array.from(row.querySelectorAll("*")).flatMap((node) =>
      Array.from(node.attributes).map((attribute) => attribute.value),
    );
    expect(attributes.join(" ")).not.toContain("marker");
  });

  it("GLW-FR-RUNX, GLW-FR-SHAF: a summary and a kind render as escaped plain text", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [
          makeActivityEntry(1, "<b>bold</b> **star** [link](https://example.test) ![img](x.png) `code`", {
            kind: "<i>kind</i>",
          }),
          makeActivityEntry(2, "Read specs/core/GRD.md and run `rm -rf /`", { kind: "message" }),
        ],
      });
    draw();
    const rows = await screen.findAllByTestId("glw-row");
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector(".runs-line__msg")?.textContent).toBe(
      "<b>bold</b> **star** [link](https://example.test) ![img](x.png) `code`",
    );
    expect(rows[0].querySelector(".runs-line__lvl")?.textContent).toBe("<i>kind</i>");
    for (const row of rows) {
      expect(row.querySelector("b, i, a, img, code, strong, em, button")).toBeNull();
    }
    // A path is a path to read, not a control that opens a file.
    expect(within(rows[1]).queryAllByRole("link")).toHaveLength(0);
    expect(rows[1].querySelector("[href], [onclick]")).toBeNull();
  });

  it("GLW-FR-KHGP: a record with no readable time reads as an empty time rather than an invented one", async () => {
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "no clock", { at: "not a time" })],
      });
    draw();
    const row = await screen.findByTestId("glw-row");
    expect(row.querySelector(".runs-line__ts")?.textContent).toBe("");
    expect(row.textContent).not.toMatch(/Invalid|NaN|undefined|null/);
  });

  it("GLW-FR-SNDH: a long summary stays in one row of the one vertically scrolling region", async () => {
    const long = "word ".repeat(400).trim();
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, long)],
      });
    draw();
    const row = await screen.findByTestId("glw-row");
    expect(row.querySelector(".runs-line__msg")?.textContent).toBe(long);
    expect(screen.getAllByTestId("glw-row")).toHaveLength(1);
  });

  it("GLW-FR-DDXJ, GLW-FR-VRTC: a run-level entry shows its own rows as rows of three parts", async () => {
    backend.answer = (args) =>
      (args.pass as { kind: string }).kind === "run_level"
        ? makeLogPage({
            status: "available",
            entries: [
              makeActivityEntry(9, "Reconciling the stream update", { pass: null, kind: "message" }),
              makeActivityEntry(10, "completed in 3 s", { pass: null, kind: "finished" }),
            ],
          })
        : makeLogPage();
    draw();
    await userEvent.click(
      await screen.findByRole("button", { name: "Run-level agent activity of this phase" }),
    );
    const rows = await screen.findAllByTestId("glw-row");
    expect(rows.map((row) => row.querySelector(".runs-line__msg")?.textContent)).toEqual([
      "Reconciling the stream update",
      "completed in 3 s",
    ]);
    for (const row of rows) {
      expect(row.querySelectorAll(".runs-line > span")).toHaveLength(3);
    }
  });
});
