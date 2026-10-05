import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { graduationErrorMessage } from "../state/graduation";
import type { WorkStreamSummary } from "../types";
import { GraduationStart } from "./GraduationStart";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const invoked = vi.mocked(invoke);

/** One live stream of the project, as the streams listing reports it. */
const stream = (
  id: string,
  over: Partial<WorkStreamSummary["stream"]> = {},
): WorkStreamSummary => ({
  stream: {
    id,
    name: id,
    projectKey: "/Users/demo/dev/acme",
    branch: `synthesis/stream/${id}`,
    baseBranch: "main",
    baseRevision: "a91bc04",
    worktreePath: `/tmp/${id}`,
    createdAt: "2026-09-06T09:00:00Z",
    isMissing: false,
    ...over,
  },
  queuedRunCount: 0,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

/** GSD-FR-WQPD: a stream a run holds, which the new run waits behind. */
const busy = (id: string): WorkStreamSummary => ({
  ...stream(id, { busyRunId: "g-9" }),
  queuedRunCount: 0,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

/** GSD-FR-WQPD: a stream with runs already queued on it. */
const queued = (id: string, count = 2): WorkStreamSummary => ({
  ...stream(id),
  queuedRunCount: count,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

let streams: WorkStreamSummary[];
let refusal: string | null;

beforeEach(() => {
  streams = [stream("editor-work"), stream("registry-work")];
  refusal = null;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (command === "list_work_streams") return streams;
    if (command === "start_graduation") {
      if (refusal) throw refusal;
      return { id: "g-1" };
    }
    return undefined;
  });
});
afterEach(cleanup);

function draw() {
  const onClose = vi.fn();
  const onStarted = vi.fn();
  render(
    <GraduationStart
      draftId="d-1"
      draftName="artifact-window"
      onClose={onClose}
      onStarted={onStarted}
    />,
  );
  return { onClose, onStarted };
}

const dialog = () => screen.getByRole("dialog");
const picker = () => screen.getByRole("combobox", { name: "Work stream" });
const graduate = () => screen.getByTestId("graduation-start-confirm");
/** One position of the standing-work ladder, by the value it sends. */
const position = (value: string) =>
  screen.getByTestId("standing-work-choice").querySelector<HTMLInputElement>(
    `input[type="radio"][value="${value}"]`,
  ) as HTMLInputElement;
const messageField = () =>
  screen.getByRole("textbox", { name: /Commit message/ });
const commands = () => invoked.mock.calls.map(([command]) => command);
/**
 * The commands the window itself made: no capacity read, no queue read, and no
 * `append_log_records`. The log batch is flushed off a timer (`../logging`), so
 * whether it lands inside a test is timing, not behaviour.
 */
const windowCommands = () =>
  commands().filter(
    (c) =>
      c !== "get_graduation_capacity" &&
      c !== "list_graduation_queue" &&
      c !== "append_log_records",
  );

/**
 * Record what the dialog brings into view.
 *
 * jsdom implements no scrolling at all, so the elements a surface asks for are
 * the only evidence there is that anything below the fold is reachable.
 */
function watchScrolling(): HTMLElement[] {
  const seen: HTMLElement[] = [];
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: function scrollIntoView(this: HTMLElement) {
      seen.push(this);
    },
  });
  return seen;
}

describe("the window a graduation starts in", () => {
  it("GSD-FR-QMTF: it is a modal overlay of the window, named by the title it shows", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(dialog()).toHaveAttribute("aria-modal", "true");
    // Named by the title it shows, so what it is called is what is read.
    expect(dialog()).toHaveAccessibleName(/Graduate .artifact-window./);
    expect(document.querySelector(".scrim")).not.toBeNull();
  });

  it("GSD-FR-WMTD: Escape, the backdrop, Cancel and the close control all dismiss it", async () => {
    const first = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.keyboard("{Escape}");
    expect(first.onClose).toHaveBeenCalled();
    cleanup();

    const second = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(document.querySelector(".scrim") as HTMLElement);
    expect(second.onClose).toHaveBeenCalled();
    cleanup();

    const third = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(third.onClose).toHaveBeenCalled();
    cleanup();

    const fourth = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(fourth.onClose).toHaveBeenCalled();
  });

  it("GSD-FR-WMTD: a click inside the window dismisses nothing", async () => {
    const { onClose } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(screen.getByRole("dialog"));
    await userEvent.click(picker());
    expect(onClose).not.toHaveBeenCalled();
  });

  it("GSD-FR-WMTD: the first question takes the keyboard as soon as there is something to answer", async () => {
    draw();
    await waitFor(() => expect(picker()).toHaveFocus());
  });

  it("GSD-FR-WMTD, GSD-FR-LDGM: a window that rests on New stream puts the keyboard in its name field", async () => {
    streams = [];
    draw();
    await screen.findByTestId("graduation-start-no-streams");
    // Focus left outside an overlay reaches the surface underneath it, so the
    // dialog takes it into the first question it asks.
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus(),
    );
    expect(dialog().contains(document.activeElement)).toBe(true);
  });

  it("GSD-FR-WMTD: a window whose listing could not be read still holds the keyboard", async () => {
    invoked.mockImplementation(async (command: string) => {
      if (command === "list_work_streams") throw "no_project_open";
      return undefined;
    });
    draw();
    await screen.findByTestId("graduation-start-unreadable");
    await waitFor(() => expect(dialog().contains(document.activeElement)).toBe(true));
  });

  it("GSD-FR-VKLD: a run that will wait is asked the stream, the choice and the message, and nothing else", async () => {
    streams = [busy("editor-work"), stream("registry-work")];
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    // Every control that takes an answer, so a question added without a
    // requirement behind it fails here.
    const answered = Array.from(
      dialog().querySelectorAll<HTMLElement>("input, select, textarea"),
    )
      .map((control) => control.id || control.getAttribute("value"))
      .sort();
    expect(answered).toEqual([
      "commit",
      "commit_and_push",
      // GSD-FR-BZHW: the destination is asked first, and it is the only other
      // question.
      "direct",
      "graduation-stream",
      "keep",
      "new",
      "standing-work-message-graduation-start",
      "stream",
    ]);
  });

  it("GSD-FR-VKLD, GSD-FR-WQPD: a run that starts at once is asked the stream alone", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    const answered = Array.from(
      dialog().querySelectorAll<HTMLElement>("input, select, textarea"),
    ).map((control) => control.id || control.getAttribute("value"));
    expect(answered).toEqual(["stream", "new", "direct", "graduation-stream"]);
  });
});

describe("which stream the run belongs to", () => {
  it("GSD-FR-ZPWN: it offers the project's live streams, with what each is holding", async () => {
    streams = [
      { ...stream("editor-work"), queuedRunCount: 2, aheadOfBase: 0 },
      stream("registry-work"),
    ];
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(
      within(picker())
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["editor-work · 2 queued", "registry-work"]);
  });

  it("GSD-FR-ZPWN / WSS-FR-OQYG: a stream with no working copy is not one this window offers", async () => {
    streams = [stream("editor-work", { isMissing: true }), stream("registry-work")];
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(
      within(picker())
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["registry-work"]);
  });

  it("GSD-FR-HRJE: a stream a run holds is offered, and the window says this run waits", async () => {
    streams = [
      stream("editor-work", { busyRunId: "g-9" }),
      stream("registry-work"),
    ];
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(screen.getByTestId("graduation-start-busy")).toHaveTextContent(
      /A run is working in .editor-work. now\. This one waits behind it\./,
    );
    // Offered like any other: the busy stream is the one chosen, and starting
    // is available.
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith(
      "start_graduation",
      expect.objectContaining({ streamId: "editor-work" }),
    );
  });

  it("GSD-FR-HRJE: a stream nothing is working in says nothing about waiting", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(screen.queryByTestId("graduation-start-busy")).toBeNull();
  });
});

describe("what the run does with work standing in the stream", () => {
  // GSD-FR-WQPD: the choice is asked only where the run will wait, so every
  // scenario here graduates onto a stream that is occupied.
  beforeEach(() => {
    streams = [busy("editor-work"), queued("registry-work")];
  });

  it("GSD-FR-WQPD: a free stream is asked nothing, and an occupied one is asked", async () => {
    streams = [stream("editor-work"), busy("registry-work")];
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(screen.queryByTestId("standing-work-choice")).toBeNull();
    // Choosing a stream the run must wait for asks the question.
    await userEvent.selectOptions(picker(), "registry-work");
    expect(screen.getByTestId("standing-work-choice")).toBeInTheDocument();
    // And going back to the free one puts it away again.
    await userEvent.selectOptions(picker(), "editor-work");
    expect(screen.queryByTestId("standing-work-choice")).toBeNull();
  });

  it("GSD-FR-WQPD: runs already queued on a free stream are a wait of their own", async () => {
    streams = [queued("editor-work", 1)];
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    expect(screen.getByTestId("standing-work-choice")).toBeInTheDocument();
  });

  it("GSD-FR-WQPD: a run onto a free stream carries the resting position and no message", async () => {
    streams = [stream("editor-work")];
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "editor-work",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });

  it("GSD-FR-WQPD: an answer given for one stream is not sent for a free one", async () => {
    streams = [busy("editor-work"), stream("registry-work")];
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(position("keep"));
    // The author changes their mind about the stream, and the run now starts
    // at once — so the answer they gave about waiting is not what travels.
    await userEvent.selectOptions(picker(), "registry-work");
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "registry-work",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });

  it("GSD-FR-TBQX: three positions, resting on committing it first", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    const positions = within(screen.getByTestId("standing-work-choice"))
      .getAllByRole("radio")
      .map((radio) => (radio as HTMLInputElement).value);
    expect(positions).toEqual(["keep", "commit", "commit_and_push"]);
    expect(position("commit")).toBeChecked();
  });

  it("GSD-FR-TBQX: the answer travels with the start", async () => {
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.selectOptions(picker(), "registry-work");
    await userEvent.click(position("keep"));
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith({ id: "g-1" }));
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "registry-work",
      standingWork: "keep",
      standingWorkMessage: null,
    });
  });

  it("GSD-FR-TBQX: the resting position is what an author who answers nothing sends", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(graduate());
    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith("start_graduation", {
        draftId: "d-1",
        streamId: "editor-work",
        standingWork: "commit",
        standingWorkMessage: null,
      }),
    );
  });

  it("GSD-FR-TBQX: pushing is a position of its own, sent as one", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(position("commit_and_push"));
    await userEvent.click(graduate());
    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith(
        "start_graduation",
        expect.objectContaining({ standingWork: "commit_and_push" }),
      ),
    );
  });

  it("GSD-FR-NWSC: each position says what the run does when its turn comes, and no path is named", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    const choice = screen.getByTestId("standing-work-choice");
    // The question is about the moment the run starts, not about now.
    expect(choice).toHaveTextContent(/when this run starts/i);
    expect(choice).toHaveTextContent(
      /The run works on top of it, and it becomes part of what the run commits\./,
    );
    expect(choice).toHaveTextContent(/the run starts from that commit/);
    expect(choice).toHaveTextContent(
      /A push the remote refuses does not stop the run\./,
    );
    // No path set anywhere: what stands in the stream at the dispatch is not
    // what stands there now, so a list here would name the wrong files.
    expect(within(dialog()).queryAllByRole("listitem")).toHaveLength(0);
  });

  it("GSD-FR-NWSC: opening the window reads the streams and nothing of a working copy", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    // GSD-FR-LHQY: the capacity read is the one other command a window makes.
    expect(windowCommands()).toEqual([
      "list_work_streams",
    ]);
  });
});

describe("a start in flight", () => {
  beforeEach(() => {
    streams = [busy("editor-work"), queued("registry-work")];
  });

  it("GSD-FR-BFOU: while the call runs, nothing about the window can be changed or sent again", async () => {
    let accept: ((run: unknown) => void) | null = null;
    invoked.mockImplementation(async (command: string) => {
      if (command === "list_work_streams") return streams;
      if (command !== "start_graduation") return undefined;
      return new Promise((resolve) => {
        accept = resolve;
      });
    });
    const { onStarted, onClose } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(graduate());

    // One enqueue is all-or-nothing, so a second activation must not start a
    // second one, and nothing may be dismissed out from under it.
    await waitFor(() => expect(graduate()).toBeDisabled());
    expect(graduate()).toHaveTextContent("Starting…");
    expect(picker()).toBeDisabled();
    expect(position("keep")).toBeDisabled();
    expect(messageField()).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    await userEvent.click(graduate());
    await userEvent.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(
      invoked.mock.calls.filter(([command]) => command === "start_graduation"),
    ).toHaveLength(1);

    accept!({ id: "g-1" });
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
  });

  it("GSD-FR-BFOU: the window closes on the run the backend reports and not before it", async () => {
    let accept: ((run: unknown) => void) | null = null;
    invoked.mockImplementation(async (command: string) => {
      if (command === "list_work_streams") return streams;
      if (command !== "start_graduation") return undefined;
      return new Promise((resolve) => {
        accept = resolve;
      });
    });
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(graduate());
    await waitFor(() => expect(graduate()).toBeDisabled());
    expect(onStarted).not.toHaveBeenCalled();
    accept!({ id: "g-7" });
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith({ id: "g-7" }));
  });
});

describe("a project with no work stream", () => {
  it("GSD-FR-LDGM: it says there is no stream, and offers a new stream and the worktree", async () => {
    streams = [];
    draw();
    expect(
      await screen.findByTestId("graduation-start-no-streams"),
    ).toHaveTextContent(/no work stream yet/);
    // The existing-stream destination has nothing to choose from; the other
    // two stay offered, and the dialog rests on New stream.
    expect(screen.getByRole("radio", { name: "Existing stream" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "New stream" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Work directly" })).toBeEnabled();
    expect(screen.queryByRole("combobox", { name: "Work stream" })).toBeNull();
    // Nothing to wait for is nothing to decide about either.
    expect(screen.queryByTestId("standing-work-choice")).toBeNull();
  });

  it("GSD-FR-LDGM, GSD-FR-QGTC: with no stream and no name, nothing can be sent", async () => {
    streams = [];
    draw();
    await screen.findByTestId("graduation-start-no-streams");
    expect(graduate()).toBeDisabled();
    await userEvent.click(graduate());
    expect(commands()).not.toContain("start_graduation");
    expect(commands()).not.toContain("create_work_stream");
  });

  it("GSD-FR-CXVA: a listing that could not be read says so rather than saying the project has no stream", async () => {
    invoked.mockImplementation(async (command: string) => {
      if (command === "list_work_streams") throw "no_project_open";
      return undefined;
    });
    draw();
    // The read failed, so what the project holds is unknown — and an empty
    // state would be a statement this window cannot make.
    expect(
      await screen.findByTestId("graduation-start-unreadable"),
    ).toHaveTextContent(/could not be read/);
    expect(screen.queryByTestId("graduation-start-no-streams")).toBeNull();
    // GSD-FR-CXVA: no existing stream is offered until the listing is read.
    expect(screen.getByRole("radio", { name: "Existing stream" })).toBeDisabled();
    // The refusal stands beside it, in words rather than as its code.
    const alert = screen.getByTestId("graduation-start-error");
    expect(alert).toHaveTextContent(graduationErrorMessage("no_project_open"));
    expect(alert.textContent).not.toContain("no_project_open");
    // Nothing is named yet, so nothing can be confirmed.
    expect(graduate()).toBeDisabled();
  });
});

describe("a refused start", () => {
  it("GSD-FR-JYRP: each typed refusal is read in words, and the window stays open", async () => {
    refusal = "docker_backend_unverified";
    const { onStarted, onClose } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(graduate());
    const alert = await screen.findByTestId("graduation-start-error");
    expect(alert).toHaveAttribute("role", "alert");
    // The code itself is not the message: each refusal names what the author
    // has to put right.
    expect(alert).toHaveTextContent(graduationErrorMessage("docker_backend_unverified"));
    expect(alert.textContent).not.toContain("docker_backend_unverified");
    expect(onStarted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    // The refused start is the whole of what the window did.
    expect(windowCommands()).toEqual([
      "list_work_streams",
      "start_graduation",
    ]);
    // The choice the author made is still theirs to retry.
    expect(graduate()).toBeEnabled();
  });

  it("GSD-FR-JYRP: a refusal is brought into view rather than left below the fold", async () => {
    // The body scrolls once the standing-work question is in it, and the
    // refusal renders under everything else.
    streams = [busy("editor-work")];
    refusal = "docker_backend_unverified";
    const scrolled = watchScrolling();
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(graduate());
    const alert = await screen.findByTestId("graduation-start-error");
    expect(scrolled).toContain(alert);
  });

  it("GSD-FR-JYRP: every answer stands exactly as the author left it", async () => {
    refusal = "vendor_image_unconfigured";
    streams = [busy("editor-work"), queued("registry-work")];
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.selectOptions(picker(), "registry-work");
    await userEvent.click(position("commit_and_push"));
    await userEvent.type(messageField(), "Notes from the meeting");
    await userEvent.click(graduate());
    await screen.findByTestId("graduation-start-error");

    expect(picker()).toHaveValue("registry-work");
    expect(position("commit_and_push")).toBeChecked();
    expect(messageField()).toHaveValue("Notes from the meeting");
    // And a retry sends what is still on screen rather than the resting
    // positions.
    refusal = null;
    await userEvent.click(graduate());
    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith("start_graduation", {
        draftId: "d-1",
        streamId: "registry-work",
        standingWork: "commit_and_push",
        standingWorkMessage: "Notes from the meeting",
      }),
    );
  });
});

describe("the message a commit takes", () => {
  beforeEach(() => {
    streams = [busy("editor-work")];
  });

  it("GSD-FR-MZTB: it is optional, and what an empty one commits under is shown", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    // The run's own name, which is the draft's, so what "optional" means is
    // legible without a sentence.
    expect(messageField()).toHaveAttribute("placeholder", "artifact-window");
    expect(messageField()).toHaveValue("");
  });

  it("GSD-FR-MZTB: a message the author writes travels with the start", async () => {
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.type(messageField(), "Notes from the review meeting");
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith(
      "start_graduation",
      expect.objectContaining({
        standingWorkMessage: "Notes from the review meeting",
      }),
    );
  });

  it("GSD-FR-MZTB: a message of spaces alone is no message at all", async () => {
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.type(messageField(), "   ");
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith(
      "start_graduation",
      expect.objectContaining({ standingWorkMessage: null }),
    );
  });

  it("GSD-FR-MZTB: a message field the author's own choice makes appear is brought into view", async () => {
    const scrolled = watchScrolling();
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    const block = () => screen.getByTestId("standing-work-message");
    // It stands there at the resting position, and nothing scrolls: the
    // question above it is what the author is reading.
    expect(scrolled).not.toContain(block());

    await userEvent.click(position("keep"));
    await userEvent.click(position("commit"));
    expect(scrolled).toContain(block());
  });

  it("GSD-FR-MZTB: a choice that commits nothing is asked for no message", async () => {
    draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.click(position("keep"));
    expect(screen.queryByTestId("standing-work-message")).toBeNull();
    // And the two positions that do commit ask for one.
    await userEvent.click(position("commit_and_push"));
    expect(messageField()).toBeInTheDocument();
  });

  it("GSD-FR-MZTB: a message written before a choice that commits nothing is not sent", async () => {
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.type(messageField(), "Notes from the review meeting");
    // The author changes their mind: nothing is committed now, so there is no
    // commit for the message to be about.
    await userEvent.click(position("keep"));
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith(
      "start_graduation",
      expect.objectContaining({
        standingWork: "keep",
        standingWorkMessage: null,
      }),
    );
  });

  it("GSD-FR-MZTB: a message written for one stream is not sent for a free one", async () => {
    streams = [busy("editor-work"), stream("registry-work")];
    const { onStarted } = draw();
    await waitFor(() => expect(picker()).toBeInTheDocument());
    await userEvent.type(messageField(), "Notes from the review meeting");
    await userEvent.selectOptions(picker(), "registry-work");
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "registry-work",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });
});
