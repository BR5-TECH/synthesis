import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

// The module logs a missing grammar through `api.appendLogRecords`, which is an
// `invoke`; nothing here exercises that path, but the import graph reaches it.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

import {
  HIGHLIGHT_DELAY_MS,
  MAX_TOKEN_SPANS,
  useSourceTokens,
} from "./useSourceTokens";
import type { TokenSpan } from "../state/syntaxHighlight";

/**
 * ESH-FR-VUVO against the hook itself: the deferral, and the guard that stops a
 * result describing text the buffer has moved on from reaching the surface.
 *
 * The Editor's own suite covers what the spans then look like on screen; what
 * cannot be seen from there is the *timing* — that a keystroke returns before
 * any tokenising happens, and that a pass in flight when the text or the file
 * changes is discarded rather than applied to the wrong string.
 */
function Probe({
  name,
  text,
  enabled = true,
}: {
  name: string;
  text: string;
  enabled?: boolean;
}) {
  const spans = useSourceTokens(name, text, enabled);
  return (
    <div data-testid="probe" data-count={spans === null ? "null" : spans.length}>
      {describeSpans(text, spans)}
    </div>
  );
}

/** The text each span covers, so an assertion reads as the tokens it is about. */
function describeSpans(text: string, spans: TokenSpan[] | null): string {
  if (spans === null) return "";
  return spans.map((s) => `${s.role}:${text.slice(s.start, s.end)}`).join(" ");
}

const probe = () => screen.getByTestId("probe");
const spansFor = () => probe().textContent ?? "";
const isPlain = () => probe().dataset.count === "null";

afterEach(() => {
  cleanup();
});

describe("useSourceTokens (ESH-FR-MJRH)", () => {
  it("renders plain first and takes its tokens after the rest", async () => {
    render(<Probe name="a.rs" text="fn main() {}" />);
    // Nothing is tokenised during the render that mounts it — the surface is
    // painted and interactive before any of this work happens.
    expect(isPlain()).toBe(true);
    await waitFor(() => expect(isPlain()).toBe(false));
    expect(spansFor()).toContain("keyword:fn");
  });

  it("goes back to plain the moment the text moves on, then re-colours", async () => {
    const view = render(<Probe name="a.rs" text="fn main() {}" />);
    await waitFor(() => expect(spansFor()).toContain("keyword:fn"));

    // A result computed for the previous text describes offsets into a string
    // that no longer exists, so it must not be handed out for this one.
    view.rerender(<Probe name="a.rs" text="fn other() {}" />);
    expect(isPlain()).toBe(true);
    await waitFor(() => expect(spansFor()).toContain("keyword:fn"));
    expect(spansFor()).not.toContain("main");
  });

  it("drops a pass that was in flight when the file changed", async () => {
    // Two files that BOTH highlight, so the assertion cannot be satisfied by the
    // second one simply having no language: a stale result would show here as
    // Rust roles over Go text rather than as no roles at all.
    const view = render(<Probe name="a.rs" text="fn main() { let x = 1; }" />);
    // Swap the file before the first pass can have run…
    view.rerender(<Probe name="b.go" text="func main() { var x = 1 }" />);
    expect(isPlain()).toBe(true);
    // …and give both passes room to land.
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 4));
    });
    // What is showing describes the file on screen: Go's `func`, and no trace of
    // the Rust `let` the first file's pass would have marked.
    expect(spansFor()).toContain("keyword:func");
    expect(spansFor()).not.toContain("keyword:let");
  });

  it("never hands out spans measured against text the surface has moved on from", async () => {
    // The guard that matters is the one at render: the spans index one exact
    // string, so a result for an earlier one would colour the wrong characters
    // of the current text rather than merely being out of date.
    const before = 'let a = "one";';
    const after = "let a = 1;";
    const view = render(<Probe name="a.rs" text={before} />);
    await waitFor(() => expect(spansFor()).toContain('string:"one"'));

    view.rerender(<Probe name="a.rs" text={after} />);
    // Immediately: plain, rather than the previous pass's string span — which
    // would now cover `= 1;` instead of the quotes it was measured against.
    expect(isPlain()).toBe(true);
    await waitFor(() => expect(isPlain()).toBe(false));
    expect(spansFor()).not.toContain("string:");
    expect(spansFor()).toContain("number:1");
  });

  it("holds nothing while it is switched off, and forgets what it held", async () => {
    const view = render(<Probe name="a.rs" text="fn main() {}" />);
    await waitFor(() => expect(spansFor()).toContain("keyword:fn"));

    view.rerender(<Probe name="a.rs" text="fn main() {}" enabled={false} />);
    expect(isPlain()).toBe(true);

    // Switched on again it re-derives rather than producing the earlier result
    // in the same frame — which is what stops a stale set flashing over a file
    // the surface has since been pointed at.
    view.rerender(<Probe name="a.rs" text="fn main() {}" enabled />);
    expect(isPlain()).toBe(true);
    await waitFor(() => expect(spansFor()).toContain("keyword:fn"));
  });

  // ESH-FR-MJRH: past a point the colouring is what would make the editor
  // unusable rather than what makes it readable, and the plain text — a complete
  // and fully editable reading — is what stands instead.
  it("renders a file too large to draw plain rather than drawing it", async () => {
    const line = 'let a: i32 = 1; // note "x"\n';
    // Comfortably past the ceiling: this line tokenises to several spans.
    const huge = line.repeat(Math.ceil(MAX_TOKEN_SPANS / 2));
    render(<Probe name="huge.rs" text={huge} />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 6));
    });
    expect(isPlain()).toBe(true);

    // …while a file just as real but within the ceiling still colours.
    cleanup();
    render(<Probe name="small.rs" text={line.repeat(10)} />);
    await waitFor(() => expect(isPlain()).toBe(false));
  });

  it("reports plain for a file no language resolves for", async () => {
    render(<Probe name="notes.txt" text="just some ordinary words about cats" />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 4));
    });
    expect(isPlain()).toBe(true);
  });

  it("coalesces a burst of edits into one pass at the end of it", async () => {
    const view = render(<Probe name="a.rs" text="f" />);
    for (const text of ["fn", "fn ", "fn m", "fn ma", "fn mai", "fn main"]) {
      view.rerender(<Probe name="a.rs" text={text} />);
      // Each rerender restarts the rest, so none of them tokenises anything.
      expect(isPlain()).toBe(true);
    }
    await waitFor(() => expect(isPlain()).toBe(false));
    expect(spansFor()).toContain("keyword:fn");
  });
});
