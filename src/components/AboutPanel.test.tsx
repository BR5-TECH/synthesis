import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AboutPanel } from "./AboutPanel";

const openUrlMock = vi.fn(async (_url: string) => {});
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrlMock(url),
}));

afterEach(() => {
  cleanup();
  openUrlMock.mockReset();
  openUrlMock.mockResolvedValue(undefined);
});

describe("AboutPanel", () => {
  it("ABT-FR-RUSV: the dialog, the link, and the Close control carry their accessible names", () => {
    render(<AboutPanel onClose={() => {}} />);

    const dialog = screen.getByRole("dialog", { name: "About Synthesis" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("link", { name: "Synthesis on GitHub" })).toHaveTextContent(
      "Synthesis on GitHub",
    );
    expect(screen.getByRole("button", { name: "Close" })).toBeVisible();
  });

  it("ABT-FR-TNRB: the panel shows the exact description and no other control", () => {
    render(<AboutPanel onClose={() => {}} />);

    expect(
      screen.getByText("AI-powered IDE for spec-driven development."),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getAllByRole("link")).toHaveLength(1);
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("ABT-FR-DXGC: the Close control and Escape each call onClose", async () => {
    const onClose = vi.fn();
    render(<AboutPanel onClose={onClose} />);

    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);

    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("ABT-FR-HYFE: focus starts inside the panel and Tab cycles within it in both directions", async () => {
    render(<AboutPanel onClose={() => {}} />);
    const close = screen.getByRole("button", { name: "Close" });
    const link = screen.getByRole("link", { name: "Synthesis on GitHub" });
    expect(close).toHaveFocus();

    await userEvent.tab();
    expect(link).toHaveFocus();
    await userEvent.tab();
    expect(close).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(link).toHaveFocus();
  });

  it("ABT-FR-HYFE: focus that left the panel is pulled back in by Tab", async () => {
    render(
      <>
        <button>outside</button>
        <AboutPanel onClose={() => {}} />
      </>,
    );
    screen.getByRole("button", { name: "outside" }).focus();

    await userEvent.tab();

    expect(screen.getByRole("dialog")).toContainElement(
      document.activeElement as HTMLElement,
    );
  });

  it("ABT-FR-HYFE: closing returns focus to the element that held it before the panel opened", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    const { unmount } = render(<AboutPanel onClose={() => {}} />);
    expect(opener).not.toHaveFocus();
    unmount();

    expect(opener).toHaveFocus();
    opener.remove();
  });

  it("ABT-FR-WPLJ: the link opens the repository address through the opener, once per activation, and does not navigate the window", async () => {
    const onClose = vi.fn();
    render(<AboutPanel onClose={onClose} />);
    const link = screen.getByRole("link", { name: "Synthesis on GitHub" });
    expect(link).toHaveAttribute("href", "https://github.com/BR5-TECH/synthesis");

    await userEvent.click(link);

    expect(openUrlMock).toHaveBeenCalledTimes(1);
    expect(openUrlMock).toHaveBeenCalledWith("https://github.com/BR5-TECH/synthesis");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("ABT-FR-WPLJ: a primary and a middle click both have the default navigation cancelled", () => {
    render(<AboutPanel onClose={() => {}} />);
    const link = screen.getByRole("link", { name: "Synthesis on GitHub" });

    expect(fireEvent.click(link)).toBe(false);
    expect(fireEvent(link, new MouseEvent("auxclick", { bubbles: true, cancelable: true }))).toBe(
      false,
    );
  });

  it("ABT-FR-WPLJ: a failed opening leaves the panel usable", async () => {
    openUrlMock.mockRejectedValueOnce(new Error("no handler"));
    const onClose = vi.fn();
    render(<AboutPanel onClose={onClose} />);

    await userEvent.click(screen.getByRole("link", { name: "Synthesis on GitHub" }));

    expect(screen.getByRole("dialog", { name: "About Synthesis" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
