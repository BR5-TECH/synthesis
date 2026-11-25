/**
 * The comparison pop-up of a diagram hunk (`EDT-editor.md` EDT-FR-BRVQ,
 * EDT-FR-NZLC, EDT-FR-JMTD, EDT-FR-HCWP, EDT-FR-PQAS; `DCR-draft-change-review.md`
 * DCR-FR-ESIY, DCR-FR-OLDZ, DCR-FR-GAFN, DCR-FR-UKMP).
 *
 * Driven through the store, as the review that owns the hunk drives it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../../test/mermaidMock")).mermaidModule);

import { DiagramComparisonHost } from "./index";
import {
  closeComparison,
  closeComparisonOf,
  closeDiagramEditor,
  comparisonRequest,
  openComparison,
  resetDiagramEditorForTest,
  setDiagramEditorOpening,
  updateComparison,
  type ComparisonContent,
} from "./store";
import { resetMermaidMock } from "../../test/mermaidMock";
import { resetMermaidForTest } from "../markdownMermaidRender";

const OLD = "flowchart TD\n  A --> B";
const NEW = "flowchart TD\n  A --> B\n  B --> C";

function content(over: Partial<ComparisonContent> = {}): ComparisonContent {
  return {
    hunkId: "h1",
    oldSource: OLD,
    newSource: NEW,
    canReject: true,
    acceptBlocked: null,
    busy: false,
    error: null,
    onAccept: vi.fn(async () => true),
    onReject: vi.fn(async () => true),
    ...over,
  };
}

function open(
  c: ComparisonContent,
  opener: HTMLElement | null = null,
  fallback: HTMLElement | null = null,
) {
  act(() => openComparison("review:p1", "pending", c, opener, fallback));
}

const dialog = () => screen.getByRole("dialog", { name: "Compare Mermaid diagrams" });

async function sourceIn(name: string): Promise<string> {
  const pane = within(dialog()).getByRole("region", { name });
  const svg = await waitFor(() => {
    const found = pane.querySelector(".mermaid-diagram svg");
    expect(found).not.toBeNull();
    return found as SVGElement;
  });
  return svg.getAttribute("data-source") ?? "";
}

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  document.documentElement.removeAttribute("data-theme");
  render(<DiagramComparisonHost />);
});

afterEach(() => {
  cleanup();
  document.documentElement.removeAttribute("data-theme");
});

describe("the panes (EDT-FR-NZLC, DCR-FR-ESIY)", () => {
  it("EDT-FR-NZLC: is a modal dialog with a scrim, two panes with headings, and a Close control", async () => {
    open(content());
    expect(dialog()).toHaveAttribute("aria-modal", "true");
    expect(screen.getByTestId("diagram-comparison-scrim")).toBeInTheDocument();
    const left = within(dialog()).getByRole("region", { name: "Current diagram" });
    const right = within(dialog()).getByRole("region", { name: "Proposed diagram" });
    expect(within(left).getByRole("heading", { name: "Current" })).toBeInTheDocument();
    expect(within(right).getByRole("heading", { name: "Proposed" })).toBeInTheDocument();
    expect(await sourceIn("Current diagram")).toBe(OLD);
    expect(await sourceIn("Proposed diagram")).toBe(NEW);
    expect(within(dialog()).getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("EDT-FR-BRVQ: holds no source pane, no syntax hints, no legend toggle, and no edit control", () => {
    open(content());
    expect(within(dialog()).queryByRole("button", { name: /syntax hints|details/i })).toBeNull();
    expect(within(dialog()).queryByRole("textbox")).toBeNull();
    expect(within(dialog()).queryByRole("button", { name: /edit/i })).toBeNull();
    expect(dialog().querySelector(".diagram-hints")).toBeNull();
  });

  it("DCR-FR-ESIY, EDT-FR-NZLC: an addition says No diagram yet on the left, and a removal says Diagram removed on the right", async () => {
    open(content({ oldSource: null }));
    const left = within(dialog()).getByRole("region", { name: "Current diagram" });
    expect(within(left).getByText("No diagram yet")).toBeInTheDocument();
    expect(left.querySelector("svg")).toBeNull();
    expect(await sourceIn("Proposed diagram")).toBe(NEW);

    act(() => closeComparison("close"));
    open(content({ newSource: null }));
    const right = within(dialog()).getByRole("region", { name: "Proposed diagram" });
    expect(within(right).getByText("Diagram removed")).toBeInTheDocument();
    expect(await sourceIn("Current diagram")).toBe(OLD);
  });

  it("EDT-FR-JMTD: a source that does not render shows its source and the message in its pane, and the other pane is not affected", async () => {
    open(content({ newSource: "flowchart TD\n  INVALID <script>x</script>" }));
    const right = within(dialog()).getByRole("region", { name: "Proposed diagram" });
    expect(await within(right).findByRole("alert")).toHaveTextContent("Diagram error");
    expect(right).toHaveTextContent("INVALID <script>x</script>");
    expect(right.querySelector("script")).toBeNull();
    expect(await sourceIn("Current diagram")).toBe(OLD);
  });

  it("EDT-FR-JMTD: renders again in the colours of the new theme", async () => {
    open(content());
    await sourceIn("Proposed diagram");
    const pane = within(dialog()).getByRole("region", { name: "Proposed diagram" });
    expect(pane.querySelector("svg.mermaid-diagram, .mermaid-diagram svg")).toHaveAttribute(
      "data-theme",
      "default",
    );
    act(() => {
      document.documentElement.dataset.theme = "dark";
    });
    await waitFor(() =>
      expect(pane.querySelector(".mermaid-diagram svg")).toHaveAttribute("data-theme", "dark"),
    );
  });

  it("EDT-FR-JMTD: each pane is zoomed on its own", async () => {
    open(content());
    await sourceIn("Current diagram");
    await sourceIn("Proposed diagram");
    const left = within(dialog()).getByRole("region", { name: "Current diagram" });
    const right = within(dialog()).getByRole("region", { name: "Proposed diagram" });
    const zoom = (pane: HTMLElement) => pane.querySelector(".diagram-viewer__zoom")?.textContent;
    const before = zoom(right);
    fireEvent.keyDown(within(left).getByRole("img", { name: "Mermaid diagram" }), { key: "+" });
    await waitFor(() => expect(zoom(left)).not.toBe(before));
    expect(zoom(right)).toBe(before);
    // The Fit control of the pane puts it back.
    fireEvent.click(within(left).getByRole("button", { name: /Fit current diagram/i }));
    await waitFor(() => expect(zoom(left)).toBe(before));
  });

  it("DCR-FR-GAFN: an update of the content draws the new source and keeps the focus where it was", async () => {
    open(content());
    expect(await sourceIn("Proposed diagram")).toBe(NEW);
    const accept = within(dialog()).getByRole("button", { name: "Accept" });
    expect(accept).toHaveFocus();
    const next = "flowchart TD\n  A --> B\n  B --> D";
    act(() => updateComparison("review:p1", content({ newSource: next })));
    await waitFor(async () => expect(await sourceIn("Proposed diagram")).toBe(next));
    expect(within(dialog()).getByRole("button", { name: "Accept" })).toBe(accept);
    expect(accept).toHaveFocus();
  });

  it("DCR-FR-GAFN: an update that another review sends changes nothing", async () => {
    open(content());
    await sourceIn("Proposed diagram");
    act(() => updateComparison("other:p9", content({ newSource: "flowchart TD\n  Z --> Y" })));
    expect(comparisonRequest()?.content.newSource).toBe(NEW);
  });
});

describe("the decision controls (DCR-FR-OLDZ)", () => {
  it("DCR-FR-OLDZ: Accept and Reject stand in the foot, and a hunk that has no Reject on its chip has none here", () => {
    open(content({ canReject: false }));
    expect(within(dialog()).getByRole("button", { name: "Accept" })).toBeEnabled();
    expect(within(dialog()).queryByRole("button", { name: "Reject" })).toBeNull();
  });

  it("DCR-FR-OLDZ, DCR-FR-31: Accept is off with its reason for a hunk that is lost, and the focus goes to Reject", () => {
    open(content({ acceptBlocked: "The text this change alters is no longer in the prompt" }));
    const accept = within(dialog()).getByRole("button", { name: "Accept" });
    expect(accept).toBeDisabled();
    expect(accept).toHaveAttribute("title", "The text this change alters is no longer in the prompt");
    expect(within(dialog()).getByRole("status")).toHaveTextContent(/no longer in the prompt/);
    expect(within(dialog()).getByRole("button", { name: "Reject" })).toHaveFocus();
  });

  it("EDT-FR-HCWP: with no decision control on, the focus goes to Close", () => {
    open(content({ busy: true }));
    expect(within(dialog()).getByRole("button", { name: "Close" })).toHaveFocus();
    expect(within(dialog()).getByRole("button", { name: "Accept" })).toBeDisabled();
    expect(within(dialog()).getByRole("button", { name: "Reject" })).toBeDisabled();
  });

  it("DCR-FR-OLDZ: Accept calls the operation of the chip and closes the pop-up on success", async () => {
    const c = content();
    open(c);
    fireEvent.click(within(dialog()).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(c.onAccept).toHaveBeenCalledTimes(1);
    expect(c.onReject).not.toHaveBeenCalled();
  });

  it("DCR-FR-OLDZ: Reject calls the operation of the chip and closes the pop-up on success", async () => {
    const c = content();
    open(c);
    fireEvent.click(within(dialog()).getByRole("button", { name: "Reject" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(c.onReject).toHaveBeenCalledTimes(1);
    expect(c.onAccept).not.toHaveBeenCalled();
  });

  it("DCR-FR-OLDZ, DCR-FR-16: a refused decision keeps the pop-up open and shows the error beside the controls", async () => {
    const c = content({ onAccept: vi.fn(async () => false) });
    open(c);
    fireEvent.click(within(dialog()).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(within(dialog()).getByRole("button", { name: "Accept" })).toBeEnabled());
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // The review gives the typed error with the next content.
    act(() => updateComparison("review:p1", { ...c, error: "Nothing was changed." }));
    expect(within(dialog()).getByRole("alert")).toHaveTextContent("Nothing was changed.");
  });

  it("DCR-FR-OLDZ: a decision that throws keeps the pop-up open", async () => {
    const c = content({
      onAccept: vi.fn(async () => {
        throw new Error("write failed");
      }),
    });
    open(c);
    fireEvent.click(within(dialog()).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(c.onAccept).toHaveBeenCalled());
    await waitFor(() => expect(within(dialog()).getByRole("button", { name: "Accept" })).toBeEnabled());
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("DCR-FR-OLDZ: the controls are off while a decision runs, so it cannot start twice", async () => {
    let settle: (decided: boolean) => void = () => {};
    const c = content({ onAccept: vi.fn(() => new Promise<boolean>((resolve) => (settle = resolve))) });
    open(c);
    fireEvent.click(within(dialog()).getByRole("button", { name: "Accept" }));
    expect(within(dialog()).getByRole("button", { name: "Accept" })).toBeDisabled();
    expect(within(dialog()).getByRole("button", { name: "Reject" })).toBeDisabled();
    fireEvent.click(within(dialog()).getByRole("button", { name: "Accept" }));
    expect(c.onAccept).toHaveBeenCalledTimes(1);
    await act(async () => settle(true));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("closing and focus (EDT-FR-HCWP, DCR-FR-UKMP, EDT-FR-PQAS)", () => {
  it("EDT-FR-HCWP, DCR-FR-OLDZ: Escape and Close close the pop-up and decide nothing", () => {
    const c = content();
    open(c);
    fireEvent.keyDown(within(dialog()).getByRole("button", { name: "Accept" }), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    open(c);
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(c.onAccept).not.toHaveBeenCalled();
    expect(c.onReject).not.toHaveBeenCalled();
  });

  it("EDT-FR-HCWP: a click on the scrim does nothing", () => {
    open(content());
    fireEvent.mouseDown(screen.getByTestId("diagram-comparison-scrim"));
    fireEvent.click(screen.getByTestId("diagram-comparison-scrim"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("EDT-FR-HCWP: Tab and Shift+Tab cycle inside the pop-up, and a key outside it sends the focus back in", () => {
    open(content());
    const dlg = dialog();
    const accept = within(dlg).getByRole("button", { name: "Accept" });
    const close = within(dlg).getByRole("button", { name: "Close" });
    // Accept is the last control, so Tab wraps to the first.
    fireEvent.keyDown(accept, { key: "Tab" });
    expect(dlg.contains(document.activeElement)).toBe(true);
    close.focus();
    fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(within(dlg).getByRole("button", { name: "Reject" }));
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    fireEvent.keyDown(outside, { key: "a" });
    expect(dlg.contains(document.activeElement)).toBe(true);
    outside.remove();
  });

  it("DCR-FR-UKMP, EDT-FR-HCWP: on close, the focus returns to the Compare button", () => {
    const opener = document.createElement("button");
    document.body.append(opener);
    open(content(), opener);
    act(() => closeComparison("close"));
    expect(opener).toHaveFocus();
    opener.remove();
  });

  it("DCR-FR-UKMP: where the Compare button is gone on close, the focus goes to the document column", () => {
    const opener = document.createElement("button");
    const column = document.createElement("div");
    document.body.append(column, opener);
    open(content(), opener, column);
    opener.remove();
    act(() => closeComparison("hunk-decided"));
    expect(column).toHaveFocus();
    column.remove();
  });

  it("EDT-FR-PQAS: opening and closing change no document and ask for no write", () => {
    const column = document.createElement("div");
    column.textContent = "unchanged";
    document.body.append(column);
    const before = document.body.innerHTML;
    open(content(), null, column);
    act(() => closeComparison("close"));
    expect(document.body.innerHTML.replace(/ tabindex="-1"/, "")).toBe(before);
    column.remove();
  });
});

describe("a floating overlay (EDT-FR-TOVY, EDT-FR-PQAS, PCR-FR-TNQA)", () => {
  it("EDT-FR-TOVY, SNV-FR-56: opening it runs the window's opening rule, which closes the other overlays", () => {
    const rule = vi.fn();
    setDiagramEditorOpening(rule);
    open(content());
    expect(rule).toHaveBeenCalledTimes(1);
  });

  it("EDT-FR-TOVY: a diagram overlay that opens closes it, and a window that closes closes it", () => {
    open(content());
    act(() => closeDiagramEditor("window"));
    expect(screen.queryByRole("dialog")).toBeNull();
    open(content());
    act(() => closeDiagramEditor("other-overlay"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("DCR-FR-GAFN: the owner closes its own pop-up, and no other review's", () => {
    open(content());
    act(() => closeComparisonOf("other:p9", "review-ended"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    act(() => closeComparisonOf("review:p1", "review-ended"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("PCR-FR-TNQA: while a decision runs, another overlay that opens does not close it, and the keys are left to the dialog above it", async () => {
    let settle: (decided: boolean) => void = () => {};
    open(content({ onAccept: vi.fn(() => new Promise<boolean>((resolve) => (settle = resolve))) }));
    fireEvent.click(within(dialog()).getByRole("button", { name: "Accept" }));
    // The confirmation of a write opens and closes the other overlays.
    act(() => closeDiagramEditor("other-overlay"));
    expect(screen.getByRole("dialog", { name: "Compare Mermaid diagrams" })).toBeInTheDocument();
    // A key in that dialog is not taken back into the pop-up.
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    fireEvent.keyDown(outside, { key: "Tab" });
    expect(outside).toHaveFocus();
    outside.remove();
    // The window itself still closes it.
    await act(async () => settle(false));
    act(() => closeDiagramEditor("other-overlay"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
