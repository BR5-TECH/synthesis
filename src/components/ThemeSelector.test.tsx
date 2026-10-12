import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { ThemeSelector } from "./ThemeSelector";
import type { ThemePreference } from "../types";

afterEach(cleanup);

function Harness({
  initial = "dark",
  onSelect,
  onOverlayOpening = () => {},
}: {
  initial?: ThemePreference;
  onSelect?: (pref: ThemePreference) => void;
  onOverlayOpening?: () => void;
}) {
  const [pref, setPref] = useState<ThemePreference>(initial);
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button type="button">outside</button>
      <ThemeSelector
        themePref={pref}
        onSelectTheme={(p) => {
          onSelect?.(p);
          setPref(p);
        }}
        open={open}
        onOpenChange={setOpen}
        onOverlayOpening={onOverlayOpening}
      />
    </div>
  );
}

const trigger = () => screen.getByTestId("chrome-theme-trigger");
const focusTrigger = () => act(() => trigger().focus());
const menu = () => screen.queryByTestId("chrome-theme-menu");

describe("ThemeSelector trigger (SNV-FR-15, SNV-FR-QXTH)", () => {
  it("SNV-FR-QXTH: shows only the trigger, named for the selected theme, declaring a closed popup", () => {
    render(<Harness initial="dark" />);

    expect(trigger()).toHaveAccessibleName("Theme: Dark");
    expect(trigger()).toHaveAttribute("aria-haspopup", "true");
    expect(trigger()).toHaveAttribute("aria-expanded", "false");
    expect(menu()).toBeNull();
    expect(screen.queryAllByRole("radio")).toHaveLength(0);
  });

  it("SNV-FR-QXTH: the trigger tooltip names the selected theme", () => {
    render(<Harness initial="light" />);
    expect(trigger()).toHaveTextContent("Light theme");
  });

  it("SNV-FR-QXTH: the trigger shows a different icon for each theme and keeps its footprint class", () => {
    const icons: string[] = [];
    for (const pref of ["light", "dark", "system"] as const) {
      const { unmount } = render(<Harness initial={pref} />);
      icons.push(trigger().querySelector("svg")!.innerHTML);
      expect(trigger().className).toContain("btn--icon");
      unmount();
    }
    expect(new Set(icons).size).toBe(3);
  });
});

describe("ThemeSelector dropdown (SNV-FR-QXTH)", () => {
  it("SNV-FR-QXTH, SNV-FR-56: a click opens a radiogroup of three icon-only rows in order and closes the other overlays first", async () => {
    const user = userEvent.setup();
    const onOverlayOpening = vi.fn();
    render(<Harness initial="dark" onOverlayOpening={onOverlayOpening} />);

    await user.click(trigger());

    expect(onOverlayOpening).toHaveBeenCalledTimes(1);
    expect(trigger()).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("radiogroup", { name: "Theme" })).toBeInTheDocument();
    const rows = screen.getAllByRole("radio");
    expect(rows.map((r) => r.getAttribute("aria-label"))).toEqual([
      "Light",
      "Dark",
      "System",
    ]);
    expect(rows.map((r) => r.getAttribute("aria-checked"))).toEqual([
      "false",
      "true",
      "false",
    ]);
    // Icon only: the visible text of a row is its tooltip, hidden from the
    // accessibility tree.
    for (const row of rows) {
      expect(row.querySelector("svg")).not.toBeNull();
      expect(row.querySelector("[aria-hidden='true']")).not.toBeNull();
    }
  });

  it("SNV-FR-QXTH: the system row does not use the gear icon of settings", async () => {
    const user = userEvent.setup();
    render(<Harness initial="system" />);
    await user.click(trigger());

    const system = screen.getByRole("radio", { name: "System" });
    // The display has a stand; the gear has none and is drawn with a circle.
    expect(system.querySelector("svg circle")).toBeNull();
    expect(system.querySelector("svg rect")).not.toBeNull();
  });

  it("SNV-FR-QXTH, SNV-FR-16: a click on a row selects it, closes the dropdown, and updates the trigger", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness initial="dark" onSelect={onSelect} />);

    await user.click(trigger());
    await user.click(screen.getByRole("radio", { name: "Light" }));

    expect(onSelect).toHaveBeenCalledWith("light");
    expect(menu()).toBeNull();
    expect(trigger()).toHaveAccessibleName("Theme: Light");
  });

  it("SNV-FR-QXTH: a second click on the trigger closes the dropdown with no change", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);

    await user.click(trigger());
    await user.click(trigger());

    expect(menu()).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("SNV-FR-QXTH: a pointer-down outside closes the dropdown with no change", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);

    await user.click(trigger());
    await user.click(screen.getByText("outside"));

    expect(menu()).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("SNV-FR-QXTH: Escape closes the dropdown after a pointer open and returns focus to the trigger", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);

    await user.click(trigger());
    await user.keyboard("{Escape}");

    expect(menu()).toBeNull();
    expect(trigger()).toHaveFocus();
    expect(onSelect).not.toHaveBeenCalled();
  });
});

describe("ThemeSelector keyboard (SNV-FR-QXTH)", () => {
  it.each(["{Enter}", " ", "{ArrowDown}"])(
    "SNV-FR-QXTH: %j on the trigger opens the dropdown and focuses the selected row",
    async (key) => {
      const user = userEvent.setup();
      render(<Harness initial="dark" />);

      focusTrigger();
      expect(trigger()).toHaveFocus();
      await user.keyboard(key);

      expect(menu()).not.toBeNull();
      expect(screen.getByRole("radio", { name: "Dark" })).toHaveFocus();
    },
  );

  it("SNV-FR-QXTH, SNV-FR-16: arrows move between rows and Enter selects the focused row", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness initial="dark" onSelect={onSelect} />);

    focusTrigger();
    await user.keyboard("{ArrowDown}");
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: "System" })).toHaveFocus();
    // The end of the list does not wrap.
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: "System" })).toHaveFocus();
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(screen.getByRole("radio", { name: "Light" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("radio", { name: "Light" })).toHaveFocus();
    await user.keyboard("{Enter}");

    expect(onSelect).toHaveBeenCalledWith("light");
    expect(menu()).toBeNull();
    expect(trigger()).toHaveFocus();
  });

  it("SNV-FR-QXTH: Escape from a row closes the dropdown, keeps the theme, and returns focus to the trigger", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness initial="dark" onSelect={onSelect} />);

    focusTrigger();
    await user.keyboard("{Enter}");
    await user.keyboard("{ArrowDown}");
    await user.keyboard("{Escape}");

    expect(menu()).toBeNull();
    expect(trigger()).toHaveFocus();
    expect(onSelect).not.toHaveBeenCalled();
    expect(trigger()).toHaveAccessibleName("Theme: Dark");
  });

  it("SNV-FR-QXTH: Tab from a row closes the dropdown", async () => {
    const user = userEvent.setup();
    render(<Harness initial="dark" />);

    focusTrigger();
    await user.keyboard("{Enter}");
    await user.tab();

    expect(menu()).toBeNull();
  });
});
