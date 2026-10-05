// The Document text viewer (`specifications/ui/DTV-document-text-viewer.md`):
// the phases, the Rich and Source switch, the source view, the read-only
// surface, and the log records. The backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetViewStateForTest } from "../../state/documentViewState";
import { DocumentTextViewer } from ".";
import {
  ID,
  documentText,
  invokeMock,
  logErrorMock,
  logInfoMock,
  logWarnMock,
  readCalls,
  readRejects,
  readReturns,
  resetFixtures,
} from "./viewerFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./viewerFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./viewerFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("./viewerFixtures")).loggingMock);

beforeEach(() => {
  resetFixtures();
  resetViewStateForTest();
});
afterEach(cleanup);

const viewer = () => <DocumentTextViewer documentId={ID} />;

describe("DTV-FR-SQNK: loading and failure", () => {
  it("DTV-FR-SQNK, DTV-FR-HQTN: shows Loading document… in text while the read is pending", async () => {
    invokeMock.mockImplementation(() => new Promise(() => {}));
    render(viewer());
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("Loading document…");
  });

  it("DTV-FR-SQNK, DTV-FR-BPXG: a read failure shows The document could not be read. and logs it without the path", async () => {
    readRejects("io_error: /secret/path/notes.md");
    render(viewer());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The document could not be read.",
    );
    expect(screen.queryByRole("region")).toBeNull();
    expect(logErrorMock).toHaveBeenCalledTimes(1);
    expect(logErrorMock).toHaveBeenCalledWith(
      ["frontend"],
      expect.any(String),
      { reason: "read" },
    );
    expect(JSON.stringify(logErrorMock.mock.calls)).not.toContain("/secret");
  });

  it("DTV-FR-SQNK: a document of another format than Markdown or text is a failed read", async () => {
    readReturns(documentText("x", "pdf"));
    render(viewer());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The document could not be read.",
    );
  });

  it("DTV-FR-UGVG, DTV-FR-SQNK: reads with the document id and invokes nothing else", async () => {
    readReturns(documentText("hello"));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("read_document", { id: ID });
  });
});

describe("DTV-FR-BEQL: unavailable", () => {
  it.each(["unavailable", "unknown_document"])(
    "DTV-FR-BEQL, DTV-FR-BPXG: a read that fails as %s shows the unavailable state and logs a warning",
    async (code) => {
      readRejects(code);
      render(viewer());
      expect(
        await screen.findByText("This document is unavailable."),
      ).toBeInTheDocument();
      expect(screen.getByText(/may have been moved, deleted/)).toBeInTheDocument();
      expect(screen.queryByRole("region")).toBeNull();
      expect(logWarnMock).toHaveBeenCalledWith(
        ["frontend"],
        expect.any(String),
        { reason: "unavailable" },
      );
    },
  );
});

describe("DTV-FR-VZNE: the Rich and Source switch", () => {
  it("DTV-FR-VZNE: a Markdown document opens in the rich view with a Rich and Source radio group", async () => {
    readReturns(documentText("# Title\n\nBody text."));
    render(viewer());
    const group = await screen.findByRole("radiogroup", { name: "View" });
    const rich = within(group).getByRole("radio", { name: "Rich" });
    const source = within(group).getByRole("radio", { name: "Source" });
    expect(rich).toHaveAttribute("aria-checked", "true");
    expect(source).toHaveAttribute("aria-checked", "false");
    expect(rich).toHaveAttribute("data-active", "true");
    const region = screen.getByRole("region", { name: "Document text" });
    expect(within(region).getByRole("heading", { level: 1 })).toHaveTextContent(
      "Title",
    );
  });

  it("DTV-FR-VZNE: choosing Source shows the exact text and choosing Rich returns", async () => {
    const text = "# Title\n\n*x*\n";
    readReturns(documentText(text));
    const user = userEvent.setup();
    render(viewer());
    await user.click(await screen.findByRole("radio", { name: "Source" }));
    const region = screen.getByRole("region", { name: "Document text" });
    expect(region.textContent).toBe(text);
    expect(within(region).queryByRole("heading")).toBeNull();
    expect(screen.getByRole("radio", { name: "Source" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await user.click(screen.getByRole("radio", { name: "Rich" }));
    expect(
      within(screen.getByRole("region", { name: "Document text" })).getByRole(
        "heading",
        { level: 1 },
      ),
    ).toBeInTheDocument();
  });

  it("DTV-FR-VZNE, DTV-FR-HQTN: the switch is reached with Tab, and the arrow keys move and select", async () => {
    readReturns(documentText("# Title"));
    const user = userEvent.setup();
    render(viewer());
    const rich = await screen.findByRole("radio", { name: "Rich" });
    const source = screen.getByRole("radio", { name: "Source" });
    // One tab stop: the active choice.
    expect(rich).toHaveAttribute("tabindex", "0");
    expect(source).toHaveAttribute("tabindex", "-1");
    await user.tab();
    expect(rich).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(source).toHaveFocus();
    expect(source).toHaveAttribute("aria-checked", "true");
    expect(rich).toHaveAttribute("aria-checked", "false");
    expect(source).toHaveAttribute("tabindex", "0");
    await user.keyboard("{ArrowRight}");
    expect(rich).toHaveFocus();
    expect(rich).toHaveAttribute("aria-checked", "true");
    await user.keyboard("{ArrowLeft}");
    expect(source).toHaveFocus();
    await user.keyboard("{Home}");
    expect(rich).toHaveAttribute("aria-checked", "true");
    await user.keyboard("{End}");
    expect(source).toHaveAttribute("aria-checked", "true");
  });
});

describe("DTV-FR-CIWP: the source view", () => {
  it("DTV-FR-CIWP: a text document opens in the source view and shows no switch", async () => {
    readReturns(documentText("plain\ntext", "text"));
    render(viewer());
    const region = await screen.findByRole("region", { name: "Document text" });
    expect(region.textContent).toBe("plain\ntext");
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByRole("radio")).toBeNull();
  });

  it("DTV-FR-CIWP: the source text is kept exactly, with line breaks and spaces, as one text node", async () => {
    const text = "  # not a heading  \n\n\tindent\r\n<b>raw</b> *x* [a](b)\n\n\n  end  ";
    readReturns(documentText(text, "text"));
    render(viewer());
    const region = await screen.findByRole("region", { name: "Document text" });
    expect(region.textContent).toBe(text);
    const pre = region.querySelector("pre");
    expect(pre).not.toBeNull();
    expect(pre!.childNodes).toHaveLength(1);
    expect(pre!.firstChild!.nodeType).toBe(Node.TEXT_NODE);
    expect(pre!.querySelector("b, em, strong, a, h1")).toBeNull();
  });

  it("DTV-FR-CIWP: the source view of a Markdown document is the exact stored text", async () => {
    const text = "---\ntitle: x\n---\n\n| a | b |\n|---|---|\n";
    readReturns(documentText(text));
    const user = userEvent.setup();
    render(viewer());
    await user.click(await screen.findByRole("radio", { name: "Source" }));
    expect(screen.getByRole("region", { name: "Document text" }).textContent).toBe(
      text,
    );
  });
});

describe("DTV-FR-THTI: no way to change the document", () => {
  it("DTV-FR-THTI: no editable surface, toolbar, Save, or dirty indicator, in either view", async () => {
    readReturns(documentText("# Title\n\ntext"));
    const user = userEvent.setup();
    const { container } = render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    const check = () => {
      expect(
        container.querySelector(
          "textarea, input, [contenteditable], [contenteditable='true'], [role='toolbar'], [role='textbox']",
        ),
      ).toBeNull();
      expect(screen.queryByRole("button", { name: /save/i })).toBeNull();
      expect(screen.queryByText(/unsaved|modified|dirty/i)).toBeNull();
      expect(container.querySelector("[data-dirty]")).toBeNull();
    };
    check();
    await user.click(screen.getByRole("radio", { name: "Source" }));
    check();
    expect(invokeMock.mock.calls.every((c) => c[0] === "read_document")).toBe(
      true,
    );
  });

  it("DTV-FR-THTI: the text can be selected and copied, because nothing blocks selection", async () => {
    readReturns(documentText("copy me", "text"));
    const { container } = render(viewer());
    const region = await screen.findByRole("region", { name: "Document text" });
    expect(region.getAttribute("style") ?? "").not.toMatch(/user-select/);
    expect(container.querySelector("[style*='user-select']")).toBeNull();
    const range = document.createRange();
    range.selectNodeContents(region);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    expect(selection.toString()).toBe("copy me");
  });
});

describe("DTV-FR-HQTN: focus stop and names", () => {
  it("DTV-FR-HQTN: the text region is a focus stop with an accessible name, after the switch in Tab order", async () => {
    readReturns(documentText("# Title"));
    const user = userEvent.setup();
    render(viewer());
    const region = await screen.findByRole("region", { name: "Document text" });
    expect(region).toHaveAttribute("tabindex", "0");
    await user.tab();
    await user.tab();
    expect(region).toHaveFocus();
  });

  it("DTV-FR-HQTN: a text document has the region as its first and only focus stop", async () => {
    readReturns(documentText("plain", "text"));
    const user = userEvent.setup();
    render(viewer());
    const region = await screen.findByRole("region", { name: "Document text" });
    await user.tab();
    expect(region).toHaveFocus();
  });
});

describe("DTV-FR-BPXG: logging", () => {
  it("DTV-FR-BPXG: an INFO record carries the format and the UTF-8 byte length and no text", async () => {
    const text = "Secret héllo wörld";
    readReturns(documentText(text));
    render(viewer());
    await screen.findByRole("region", { name: "Document text" });
    expect(logInfoMock).toHaveBeenCalledTimes(1);
    const [domains, message, fields] = logInfoMock.mock.calls[0];
    expect(domains).toEqual(["frontend"]);
    expect(fields).toEqual({
      format: "markdown",
      bytes: new TextEncoder().encode(text).length,
    });
    expect(fields.bytes).toBeGreaterThan(text.length);
    const everything = JSON.stringify([...logInfoMock.mock.calls, ...logWarnMock.mock.calls, ...logErrorMock.mock.calls]);
    expect(everything).not.toContain("Secret");
    expect(everything).not.toContain("notes.md");
    expect(everything).not.toContain("/refs");
    expect(String(message)).not.toContain("Secret");
  });
});

describe("DTV-FR-XMRL: the choice belongs to the tab", () => {
  it("DTV-FR-XMRL: the choice survives an unmount and a mount of the same tab, and a new tab starts rich", async () => {
    readReturns(documentText("# Title"));
    const user = userEvent.setup();
    const first = render(viewer());
    await user.click(await screen.findByRole("radio", { name: "Source" }));
    first.unmount();
    const second = render(viewer());
    expect(await screen.findByRole("radio", { name: "Source" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    second.unmount();
    // The shell forgets the state when the tab closes.
    resetViewStateForTest();
    render(viewer());
    expect(await screen.findByRole("radio", { name: "Rich" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("DTV-FR-XMRL: the choice is memory only and writes only to the view state store", async () => {
    readReturns(documentText("# Title"));
    const user = userEvent.setup();
    render(viewer());
    await user.click(await screen.findByRole("radio", { name: "Source" }));
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
    expect(readCalls()).toHaveLength(1);
  });
});

describe("DTV-FR-SSQI: size", () => {
  it("DTV-FR-SSQI: a document of 300 KB renders in both views", async () => {
    const paragraph = "Some *emphasis* and `code` with [a link](x) in a line of prose.\n\n";
    const heading = "## Heading\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n";
    let text = "";
    while (text.length < 300 * 1024) text += heading + paragraph;
    readReturns(documentText(text));
    const user = userEvent.setup();
    render(viewer());
    const region = await screen.findByRole("region", { name: "Document text" });
    expect(region.querySelectorAll("h2").length).toBeGreaterThan(1000);
    await user.click(screen.getByRole("radio", { name: "Source" }));
    expect(screen.getByRole("region", { name: "Document text" }).textContent).toBe(
      text,
    );
  }, 20000);
});
