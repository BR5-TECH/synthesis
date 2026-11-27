// The links of a read-only version of a draft and of the draft preview
// (`NAW-new-artifact.md` NAW-FR-RUPR, `DCR-draft-change-review.md` DCR-FR-HFUZ).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { openUrl } from "@tauri-apps/plugin-opener";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(() => Promise.resolve()) }));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

import { DraftVersionReading } from "./DraftVersionReading";
import { draftLinkPlace } from "./NewArtifactWorkspace/DraftDocumentColumn";
import { RichLinkHostContext, type LinkHost } from "./linkFollow";
import { resetToasts, visibleToasts } from "../state/toasts";

const TEXT =
  "# Prompt\n\n[spec](docs/x.md) [later](#later) [web](https://example.com) [own](Untitled.md#later)\n\n## Later\n";
const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView");

const reading = () => screen.getByTestId("draft-version-reading");
const tiptap = () => (reading() as unknown as { editor: TiptapEditor }).editor;
const link = (text: string) =>
  Array.from(reading().querySelectorAll("a")).find((a) => a.textContent === text)!;

beforeEach(() => {
  resetToasts();
  vi.mocked(openUrl).mockClear();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: vi.fn(),
  });
});

afterEach(() => {
  cleanup();
  if (original) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", original);
  else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
});

describe("a read-only reading", () => {
  it("NAW-FR-RUPR, EDT-FR-YHKG: a plain click on a file link goes to the tab's host", async () => {
    const host: LinkHost = {
      base: { fileDir: "", roots: ["/p"] },
      selfPath: "Untitled.md",
      openFile: vi.fn(),
    };
    render(
      <RichLinkHostContext.Provider value={host}>
        <DraftVersionReading text={TEXT} mode="wysiwyg" label="Version 1" />
      </RichLinkHostContext.Provider>,
    );
    await screen.findByRole("heading", { name: "Prompt" });
    fireEvent.click(link("spec"));
    expect(host.openFile).toHaveBeenCalledWith("docs/x.md", null);
    // A link to the version's own name stays in the reading.
    fireEvent.click(link("own"));
    expect(host.openFile).toHaveBeenCalledTimes(1);
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Later");
  });

  it("NAW-FR-RUPR: the place of a prompt starts at the project root and names the prompt", () => {
    expect(draftLinkPlace("Untitled.md")).toEqual({ fileDir: "", selfPath: "Untitled.md" });
    expect(draftLinkPlace(null)).toEqual({ fileDir: "", selfPath: null });
  });

  it("DCR-FR-HFUZ: with no host, a fragment jumps and an external link opens, and a file link is refused", async () => {
    render(<DraftVersionReading text={TEXT} mode="wysiwyg" label="Preview" />);
    await screen.findByRole("heading", { name: "Prompt" });
    fireEvent.click(link("later"));
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Later");
    fireEvent.click(link("web"));
    await Promise.resolve();
    await Promise.resolve();
    expect(openUrl).toHaveBeenCalledWith("https://example.com");
    fireEvent.click(link("spec"));
    expect(visibleToasts()[0]).toMatchObject({
      key: "link-refused",
      body: "The app does not open this kind of link.",
    });
  });
});
