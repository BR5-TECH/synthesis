/**
 * The one table of OpenSpec failure notices
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md`; the class
 * table of `../../../specifications/core/OFP-openspec-failure-policy.md`).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const logWarn = vi.fn();
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: (...args: unknown[]) => logWarn(...args),
  logError: vi.fn(),
}));

import {
  classifyingCode,
  diagnosticLines,
  diagnosticsText,
  failureClass,
  hasNotice,
  logFailureNotice,
  noticeFailureOf,
  noticeSentence,
  plainText,
  refusalString,
  repairSentence,
  settingsActionOf,
  shortForm,
  toOpenSpecFailure,
} from "./failureNotice";
import type { OpenSpecFailure } from "../../types";

const f = (over: Partial<OpenSpecFailure> & { code: string }): OpenSpecFailure => ({
  diagnostics: [],
  ...over,
});

beforeEach(() => {
  logWarn.mockReset();
});

describe("the notice table", () => {
  it("OFN-FR-PVYE, OFP-FR-PHOW: each code of the class table has the class of that table and a sentence", () => {
    const table: Array<[OpenSpecFailure, string, string]> = [
      [f({ code: "openspec_mode_inactive" }), "environment", "OpenSpec mode is off for this project."],
      [f({ code: "cli_unavailable", detail: "not_found" }), "environment", "The OpenSpec CLI is not installed on this machine."],
      [
        f({ code: "cli_unavailable", detail: "unsupported" }),
        "environment",
        "The OpenSpec CLI on this machine is too old. OpenSpec mode needs version 1.0 or later.",
      ],
      [
        f({ code: "cli_unavailable", detail: "failed" }),
        "environment",
        "The OpenSpec CLI on this machine does not start or does not report its version.",
      ],
      [
        f({ code: "cli_output_unreadable" }),
        "environment",
        "The OpenSpec CLI gave an answer that the application cannot read. Check the version of the OpenSpec CLI.",
      ],
      [
        f({ code: "openspec_image_cli_unavailable", detail: "not_found" }),
        "environment",
        "The agent image of this project does not hold the OpenSpec CLI.",
      ],
      [
        f({ code: "openspec_image_cli_unavailable", detail: "unsupported" }),
        "environment",
        "The OpenSpec CLI in the agent image of this project is too old. It must be version 1.0 or later.",
      ],
      [
        f({ code: "openspec_image_cli_unavailable", detail: "failed" }),
        "environment",
        "The OpenSpec CLI in the agent image of this project does not start. The image needs Node 22.12.0 or later.",
      ],
      [f({ code: "cli_timed_out" }), "transient", "The OpenSpec CLI did not finish in time, also after one more try."],
      [f({ code: "cli_failed", detail: "2" }), "transient", "The OpenSpec CLI stopped before it answered, also after one more try."],
      [f({ code: "cli_rejected" }), "content", "OpenSpec reported a problem in the OpenSpec files of this change."],
      [
        f({ code: "change_not_ready", detail: "design,tasks" }),
        "content",
        "The change misses these artifacts: design, tasks. Complete them before Apply.",
      ],
      [f({ code: "tasks_incomplete", change: "c" }), "content", 'The change "c" has tasks that are not done.'],
      [f({ code: "archive_tasks_incomplete", change: "c" }), "content", 'The change "c" has tasks that are not done.'],
      [
        f({ code: "archive_spec_update_failed", change: "c" }),
        "content",
        'The deltas of "c" do not apply to the live specifications.',
      ],
      [
        f({ code: "archive_spec_validation_failed", change: "c" }),
        "content",
        'The live specifications are not valid after the deltas of "c".',
      ],
      [f({ code: "archive_validation_failed", change: "c" }), "content", 'The change "c" is not valid.'],
      [
        f({ code: "archive_target_exists", change: "c" }),
        "content",
        'An archive folder for "c" with the date of today exists already.',
      ],
      [f({ code: "archive_error", change: "c" }), "content", 'OpenSpec could not archive "c".'],
      [f({ code: "archive_failed", cause: "archive_new_code", change: "c" }), "content", 'OpenSpec could not archive "c".'],
    ];
    for (const [failure, klass, sentence] of table) {
      expect(failureClass(failure)).toBe(klass);
      expect(noticeSentence(failure)).toBe(sentence);
      expect(hasNotice(failure)).toBe(true);
    }
  });

  it("OFN-FR-PVYE, OFP-FR-FKRV: a code that the table does not name gets no notice, also as the cause of an archive", () => {
    for (const code of ["change_not_found", "change_archived", "invalid_change_name", "proposal_pending", "commit_failed", "no_open_project"]) {
      expect(failureClass(f({ code }))).toBe("none");
      expect(noticeSentence(f({ code }))).toBeNull();
      expect(hasNotice(f({ code }))).toBe(false);
    }
    expect(failureClass(f({ code: "archive_failed", cause: "commit_failed" }))).toBe("none");
    // OFP class table: an archive cause is `content` only where it starts with
    // `archive_` and is not `archive_failed`.
    for (const failure of [
      f({ code: "archive_failed", change: "c" }),
      f({ code: "archive_failed", cause: "unknown_stream", change: "c" }),
      f({ code: "archive_failed", cause: "new_status_code", change: "c" }),
      f({ code: "archive_failed", cause: "archive_failed", change: "c" }),
      f({ code: "change_not_archivable", cause: "archive_failed", change: "c" }),
    ]) {
      expect(failureClass(failure)).toBe("none");
      expect(noticeSentence(failure)).toBeNull();
      expect(hasNotice(failure)).toBe(false);
    }
    // A wrapping refusal with no cause names no OpenSpec code.
    expect(failureClass(f({ code: "openspec_validation_unavailable" }))).toBe("none");
  });

  it("OFN-FR-RNWG, OFP-FR-BYYT: the notice reads the cause where present, and the code otherwise", () => {
    const wrapped = f({ code: "apply_progress_unavailable", cause: "cli_timed_out" });
    expect(classifyingCode(wrapped)).toBe("cli_timed_out");
    expect(failureClass(wrapped)).toBe("transient");
    expect(noticeSentence(wrapped)).toBe("The OpenSpec CLI did not finish in time, also after one more try.");
    const archive = f({ code: "change_not_archivable", cause: "cli_unavailable", detail: "unsupported" });
    expect(failureClass(archive)).toBe("environment");
    expect(noticeSentence(archive)).toContain("too old");
    expect(classifyingCode(f({ code: "cli_rejected" }))).toBe("cli_rejected");
  });

  it("OFN notice table: the change name falls back to the change of the surface", () => {
    expect(noticeSentence(f({ code: "tasks_incomplete" }), "add-logout")).toBe(
      'The change "add-logout" has tasks that are not done.',
    );
    expect(noticeSentence(f({ code: "tasks_incomplete", change: "own" }), "other")).toContain('"own"');
    // With no change at all, the sentence reads "this change", with no quotes.
    expect(noticeSentence(f({ code: "archive_spec_update_failed" }))).toBe(
      "The deltas of this change do not apply to the live specifications.",
    );
  });

  it("OFN-FR-QZEC: the short form is the sentence alone", () => {
    const failure = f({ code: "cli_failed", repairRunId: "g-1", diagnostics: [{ message: "x" }] });
    expect(shortForm(failure)).toBe("The OpenSpec CLI stopped before it answered, also after one more try.");
    expect(shortForm(f({ code: "change_not_found" }))).toBeNull();
  });

  it("OFN-FR-EPIG: an environment notice routes to the OpenSpec section, and an image notice to the Docker section", () => {
    expect(settingsActionOf(f({ code: "cli_unavailable", detail: "not_found" }))).toBe("openspec");
    expect(settingsActionOf(f({ code: "openspec_mode_inactive" }))).toBe("openspec");
    expect(settingsActionOf(f({ code: "openspec_image_cli_unavailable", detail: "failed" }))).toBe("docker");
    expect(settingsActionOf(f({ code: "cli_timed_out" }))).toBeNull();
    expect(settingsActionOf(f({ code: "cli_rejected" }))).toBeNull();
  });
});

describe("the repair sentences", () => {
  it("OFN-FR-RCWY: a repair run adds that a rework run started to repair the change", () => {
    expect(repairSentence(f({ code: "archive_failed", repairRunId: "g-1" }))).toBe(
      "A rework run started to repair the change.",
    );
  });

  it("OFN-FR-LRCZ: a repair refusal is said in words, and every other code by its code", () => {
    const said = (repairRefusal: string) => repairSentence(f({ code: "archive_failed", repairRefusal }));
    expect(said("proposal_pending")).toBe("No repair run started: the change holds a pending edit proposal.");
    expect(said("openspec_run_active")).toBe("No repair run started: a run of the change is active.");
    for (const image of ["vendor_image_unconfigured", "vendor_image_invalid", "vendor_execution_unsupported", "docker_backend_unverified", "docker_socket_unavailable"]) {
      expect(said(image)).toBe("No repair run started: the agent image is not ready.");
    }
    expect(said("write_failed")).toBe("No repair run started (write_failed).");
    expect(repairSentence(f({ code: "archive_failed" }))).toBeNull();
  });
});

describe("the diagnostics", () => {
  it("OFN-FR-IHSQ: a line holds the severity, the code, and the path where present, then the message; the fix follows on its own line", () => {
    expect(
      diagnosticLines({ severity: "error", code: "c1", path: "specs/a.md", message: "Bad", fix: "Fix it" }),
    ).toEqual(["error c1 specs/a.md: Bad", "Fix: Fix it"]);
    expect(diagnosticLines({ message: "Only the message" })).toEqual(["Only the message"]);
    expect(diagnosticLines({ severity: "warning", message: "W", fix: "" })).toEqual(["warning: W"]);
  });

  it("OFN-FR-UNEF: a control character renders as a space", () => {
    expect(plainText("a\u0000b\nc\td\u001be\u007f")).toBe("a b c d e ");
    expect(diagnosticLines({ message: "line one\nline two", code: "x\ty" })).toEqual(["x y: line one line two"]);
  });

  it("OFN-FR-DZSQ: every diagnostic as plain text, in the order of the CLI", () => {
    expect(
      diagnosticsText([
        { severity: "error", message: "First", fix: "Do this" },
        { message: "Second" },
      ]),
    ).toBe("error: First\nFix: Do this\nSecond");
  });
});

describe("reading a rejection", () => {
  it("OFP-FR-QLQJ: an OpenSpecFailure object is read field by field, and a diagnostic with no message is dropped", () => {
    const read = toOpenSpecFailure({
      code: "archive_failed",
      cause: "archive_spec_update_failed",
      change: "add-logout",
      threadId: "t-1",
      repairRunId: "g-2",
      diagnostics: [{ severity: "error", message: "m", fix: "x" }, { severity: "info" }, "noise"],
    });
    expect(read).toEqual({
      code: "archive_failed",
      cause: "archive_spec_update_failed",
      change: "add-logout",
      detail: null,
      threadId: "t-1",
      repairRunId: "g-2",
      repairRefusal: null,
      diagnostics: [{ severity: "error", code: null, path: null, message: "m", fix: "x" }],
    });
  });

  it("OFP-FR-ZDEE: a typed string gives the code and the detail, and a wrapping refusal its cause", () => {
    expect(toOpenSpecFailure("cli_unavailable: not_found")).toEqual({
      code: "cli_unavailable",
      detail: "not_found",
      diagnostics: [],
    });
    expect(toOpenSpecFailure(new Error("Error: cli_timed_out"))).toEqual({
      code: "cli_timed_out",
      detail: null,
      diagnostics: [],
    });
    expect(toOpenSpecFailure("apply_progress_unavailable: cli_failed").cause).toBe("cli_failed");
    expect(toOpenSpecFailure("archive_failed: add-logout: archive_error")).toMatchObject({
      change: "add-logout",
      cause: "archive_error",
    });
    expect(toOpenSpecFailure('{"code":"cli_rejected","diagnostics":[{"message":"m"}]}').diagnostics).toHaveLength(1);
  });

  it("OFP-FR-QLQJ: a rejection gets a notice only where its code is an OpenSpec failure, and reads as a typed string", () => {
    expect(noticeFailureOf({ code: "cli_rejected", diagnostics: [] })?.code).toBe("cli_rejected");
    expect(noticeFailureOf("unknown_run")).toBeNull();
    expect(refusalString({ code: "openspec_validation_unavailable", cause: "cli_failed", diagnostics: [] })).toBe(
      "openspec_validation_unavailable: cli_failed",
    );
    expect(refusalString({ code: "cli_unavailable", detail: "failed", diagnostics: [] })).toBe("cli_unavailable: failed");
    expect(refusalString("stream_busy")).toBe("stream_busy");
    // A wrapping refusal of an archive keeps its change, and reads back whole.
    const archive = { code: "archive_failed", cause: "commit_failed", change: "add-refunds", diagnostics: [] };
    expect(refusalString(archive)).toBe("archive_failed: add-refunds: commit_failed");
    expect(toOpenSpecFailure(refusalString(archive))).toMatchObject({
      code: "archive_failed",
      change: "add-refunds",
      cause: "commit_failed",
    });
    expect(refusalString({ code: "change_not_archivable", cause: "proposal_pending", diagnostics: [] })).toBe(
      "change_not_archivable: proposal_pending",
    );
    // A wrapper with a cause and a detail writes the cause, so the classifying
    // code survives the string; the CLI state stays in the object alone.
    const both = { code: "openspec_validation_unavailable", cause: "cli_unavailable", detail: "failed", diagnostics: [] };
    expect(refusalString(both)).toBe("openspec_validation_unavailable: cli_unavailable");
    expect(failureClass(toOpenSpecFailure(refusalString(both)))).toBe("environment");
    expect(classifyingCode(toOpenSpecFailure(refusalString(both)))).toBe("cli_unavailable");
  });
});

describe("the log record", () => {
  it("OFN-FR-LKAT: one WARN with the surface, the code, the cause, and the class, and no diagnostic, detail, or change name", () => {
    logFailureNotice(
      "stream_row_merge",
      f({
        code: "archive_failed",
        cause: "archive_spec_update_failed",
        change: "secret-change",
        detail: "some detail",
        diagnostics: [{ message: "private CLI text" }],
      }),
    );
    expect(logWarn).toHaveBeenCalledTimes(1);
    const [domains, message, fields] = logWarn.mock.calls[0];
    expect(domains).toEqual(["frontend"]);
    expect(message).toBe("an openspec failure notice appeared");
    expect(fields).toEqual({
      surface: "stream_row_merge",
      code: "archive_failed",
      cause: "archive_spec_update_failed",
      class: "content",
    });
    expect(JSON.stringify(logWarn.mock.calls)).not.toMatch(/secret-change|private CLI text|some detail/);
  });

  it("OFN-FR-LKAT: the CLI state is the one detail a record holds", () => {
    logFailureNotice("spec_map_load", f({ code: "cli_unavailable", detail: "unsupported" }));
    expect(logWarn.mock.calls[0][2]).toEqual({
      surface: "spec_map_load",
      code: "cli_unavailable",
      cause: "",
      class: "environment",
      state: "unsupported",
    });
  });
});
