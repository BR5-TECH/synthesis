import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ActivityBar } from "./Shell";

// SNV-shell-navigation.md SNV-FR-03 / SNV-FR-44 / SNV-FR-48: the strip's two
// clusters, what each toggle drives, and when the History toggle is live.

afterEach(cleanup);

function renderBar(overrides: Partial<Parameters<typeof ActivityBar>[0]> = {}) {
  const props = {
    panelSurface: "library" as const,
    panelHidden: false,
    onSelectPanelSurface: vi.fn(),
    bottomSurface: "runs" as const,
    bottomVisible: false,
    onSelectBottomSurface: vi.fn(),
    historyEnabled: true,
    side: "left" as const,
    ...overrides,
  };
  render(<ActivityBar {...props} />);
  return props;
}

const clusters = () =>
  Array.from(document.querySelectorAll(".activity-bar__cluster"));

const namesIn = (el: Element) =>
  within(el as HTMLElement)
    .getAllByRole("button")
    .map((b) => b.getAttribute("aria-label"));

describe("the Documents toggle (DPN-FR-KYSK, SNV-FR-UCJH, SNV-FR-49)", () => {
  it("DPN-FR-KYSK, SNV-FR-UCJH: Documents is the second toggle of the leading cluster, right after Project and before Notes", () => {
    renderBar();
    const [leading] = clusters();
    const names = namesIn(leading);
    expect(names.indexOf("Documents")).toBe(names.indexOf("Project") + 1);
    expect(names.indexOf("Notes")).toBe(names.indexOf("Documents") + 1);
    // It is a vertical-panel toggle, so it is not in the bottom cluster.
    const [, trailing] = clusters();
    expect(namesIn(trailing)).not.toContain("Documents");
  });

  it("DPN-FR-KYSK, SNV-FR-UCJH: activating Documents asks for the documents surface on the terms of the other toggles", async () => {
    const props = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Documents" }));
    expect(props.onSelectPanelSurface).toHaveBeenCalledTimes(1);
    expect(props.onSelectPanelSurface).toHaveBeenCalledWith("documents");
    expect(props.onSelectBottomSurface).not.toHaveBeenCalled();
  });

  it("DPN-FR-KYSK, SNV-FR-49: the toggle carries the tooltip and the accessible name Documents, shown on keyboard focus", async () => {
    renderBar();
    const toggle = screen.getByRole("button", { name: "Documents" });
    expect(toggle).toHaveAttribute("aria-label", "Documents");
    await act(async () => toggle.focus());
    expect(await screen.findByRole("tooltip")).toHaveTextContent("Documents");
  });

  it("DPN-FR-KYSK, SNV-FR-45: the toggle reads as active while Documents is the shown surface, and none does while the panel is hidden", () => {
    renderBar({ panelSurface: "documents" });
    expect(screen.getByRole("button", { name: "Documents" })).toHaveAttribute("data-active", "true");
    expect(screen.getByRole("button", { name: "Project" })).toHaveAttribute("data-active", "false");
    cleanup();
    renderBar({ panelSurface: "documents", panelHidden: true });
    expect(document.querySelectorAll('[data-active="true"]')).toHaveLength(0);
  });
});

describe("the activity bar's two clusters (SNV-FR-44)", () => {
  it("groups vertical-panel toggles at the top and bottom-panel toggles at the foot", () => {
    renderBar();
    const [leading, trailing] = clusters();
    // CMP-FR-01: Comments sits immediately below Notes. SNV-FR-44: Changes
    // closes the cluster, meeting the Git toggle across the gap.
    // DPN-FR-KYSK / SNV-FR-UCJH: Documents sits immediately after Project.
    expect(namesIn(leading)).toEqual([
      "Project",
      "Documents",
      "Notes",
      "Comments",
      "Drafts",
      "Changes",
    ]);
    // SNV-FR-44: Logs sits beside Runs — the two surfaces that stream lines as
    // they are produced — and Git closes the cluster, at the very foot of the
    // strip beside the status bar that reports its state.
    expect(namesIn(trailing)).toEqual(["Runs", "Logs", "History", "Git"]);
  });

  it("holds only panel toggles — nothing else (SNV-FR-03 / SNV-FR-42)", () => {
    renderBar();
    const bar = document.querySelector(".activity-bar") as HTMLElement;
    expect(within(bar).getAllByRole("button")).toHaveLength(10);
  });

  it("routes each toggle to its own cluster's callback", async () => {
    const props = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));
    expect(props.onSelectPanelSurface).toHaveBeenCalledWith("notes");
    expect(props.onSelectBottomSurface).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Git" }));
    expect(props.onSelectBottomSurface).toHaveBeenCalledWith("git");
  });

  it("routes every vertical-panel toggle to its own surface", async () => {
    // One assertion per toggle: a copy-paste in the strip that pointed Comments
    // at "notes" would otherwise pass the enumeration test above untouched.
    for (const [name, surface] of [
      ["Project", "library"],
      ["Documents", "documents"],
      ["Notes", "notes"],
      ["Comments", "comments"],
      ["Drafts", "drafts"],
      ["Changes", "changes"],
    ] as const) {
      const props = renderBar();
      await userEvent.click(screen.getByRole("button", { name }));
      expect(props.onSelectPanelSurface).toHaveBeenCalledWith(surface);
      cleanup();
    }
  });

  it("routes every bottom-panel toggle to its own surface", async () => {
    for (const [name, surface] of [
      ["Runs", "runs"],
      ["Logs", "logs"],
      ["History", "history"],
      ["Git", "git"],
    ] as const) {
      const props = renderBar();
      await userEvent.click(screen.getByRole("button", { name }));
      expect(props.onSelectBottomSurface).toHaveBeenCalledWith(surface);
      cleanup();
    }
  });

  it("keeps Logs live while History is greyed out (SNV-FR-44, SNV-FR-49, SNV-FR-46, SNV-FR-48 / LOG-FR-01)", async () => {
    // SNV-FR-48 disables History with no artifact-bound tab active. Logs sits
    // beside it and must NOT inherit that gating: a diagnostic record is about
    // the application, not about the open artifact, so the one moment a user
    // most wants the log is the moment no artifact is open.
    const props = renderBar({ historyEnabled: false });
    const logs = screen.getByRole("button", { name: "Logs" });
    expect(logs).not.toBeDisabled();
    expect(screen.getByRole("button", { name: /^History/ })).toBeDisabled();

    await userEvent.click(logs);
    expect(props.onSelectBottomSurface).toHaveBeenCalledWith("logs");
  });
});

describe("which toggle reads as active (SNV-FR-45 / SNV-FR-46)", () => {
  const activeNames = () =>
    Array.from(document.querySelectorAll('[data-active="true"]')).map((b) =>
      b.getAttribute("aria-label"),
    );

  it("marks the surface each panel is showing", () => {
    renderBar({
      panelSurface: "changes",
      bottomVisible: true,
      bottomSurface: "git",
    });
    expect(activeNames()).toEqual(["Changes", "Git"]);
  });

  it("marks Comments when the panel is showing it", () => {
    renderBar({ panelSurface: "comments" });
    expect(activeNames()).toEqual(["Comments"]);
  });

  it("marks Drafts when the panel is showing it (SNV-FR-44, SNV-FR-49, SNV-FR-45, DRP-FR-01)", () => {
    renderBar({ panelSurface: "drafts" });
    expect(activeNames()).toEqual(["Drafts"]);
  });

  it("marks Logs when the bottom panel is showing it", () => {
    renderBar({ bottomVisible: true, bottomSurface: "logs" });
    expect(activeNames()).toEqual(["Project", "Logs"]);
  });

  it("marks nothing in a cluster whose panel is hidden", () => {
    renderBar({ panelHidden: true, bottomVisible: false });
    expect(activeNames()).toEqual([]);
  });

  it("marks the two clusters independently", () => {
    // Hiding the vertical panel says nothing about the bottom one.
    renderBar({
      panelHidden: true,
      bottomVisible: true,
      bottomSurface: "runs",
    });
    expect(activeNames()).toEqual(["Runs"]);
  });
});

describe("the History toggle's enablement (SNV-FR-48 / HVW-FR-01)", () => {
  it("is enabled while an artifact-bound tab is active", () => {
    renderBar({ historyEnabled: true });
    expect(screen.getByRole("button", { name: "History" })).toBeEnabled();
  });

  it("is disabled, and says why, with no artifact-bound tab active", () => {
    renderBar({ historyEnabled: false });
    // The label carries the reason, so the greyed-out control explains itself
    // rather than leaving the user to guess (SNV-FR-49).
    expect(
      screen.getByRole("button", {
        name: "History — requires an open artifact",
      }),
    ).toBeDisabled();
    // …and it is the only toggle affected.
    for (const name of ["Project", "Notes", "Changes", "Runs", "Git"]) {
      expect(screen.getByRole("button", { name })).toBeEnabled();
    }
  });

  it("stays marked active while disabled when the panel is on History (SNV-FR-48)", () => {
    // The strip says where the panel *is*; greyed out only says you cannot
    // steer it there right now.
    renderBar({
      historyEnabled: false,
      bottomVisible: true,
      bottomSurface: "history",
    });
    const toggle = screen.getByRole("button", {
      name: "History — requires an open artifact",
    });
    expect(toggle).toBeDisabled();
    expect(toggle).toHaveAttribute("data-active", "true");
  });
});

describe("the Runs toggle's needs-attention mark (SNV-FR-70 / SNV-FR-46, SNV-FR-54, NTF-FR-39)", () => {
  const runsToggle = () =>
    screen.getByRole("button", { name: /^Runs/ }) as HTMLButtonElement;

  it("carries a mark distinct from the active marking, with the panel hidden", () => {
    // SNV-FR-70, SNV-FR-46, SNV-FR-54, NTF-FR-39: the bottom panel is hidden and a graduation run has raised.
    renderBar({ runsAttention: "on", bottomVisible: false });
    const toggle = runsToggle();
    expect(toggle).toHaveAttribute("data-attention", "on");
    expect(toggle).toHaveAttribute("data-active", "false");
    // NTF-FR-36: and a screen reader is told, in words.
    expect(toggle.getAttribute("aria-label")).toMatch(/needs attention/);
    // No other toggle in the trailing cluster is marked active or attentive.
    for (const name of ["Logs", "Git"]) {
      const other = screen.getByRole("button", { name });
      expect(other).toHaveAttribute("data-active", "false");
      expect(other).not.toHaveAttribute("data-attention");
    }
  });

  it("moves no toggle in the strip, marked or not", () => {
    // SNV-FR-70: the mark occupies space the cluster reserves whether it is
    // showing or not.
    renderBar();
    const unmarked = namesIn(clusters()[1]);
    const withoutSlot = document.querySelectorAll(
      ".activity-btn__attention",
    ).length;
    cleanup();
    renderBar({ runsAttention: "on" });
    expect(namesIn(clusters()[1]).map((name) => name?.split(" — ")[0])).toEqual(
      unmarked.map((name) => name?.split(" — ")[0]),
    );
    // The mark's element is in the tree on every toggle either way, so nothing
    // is added or removed when one takes or loses it.
    expect(document.querySelectorAll(".activity-btn__attention").length).toBe(
      withoutSlot,
    );
  });

  it("activates exactly as an unmarked one does", async () => {
    // SNV-FR-70: it is presentation and nothing else.
    const props = renderBar({ runsAttention: "on", bottomVisible: false });
    await userEvent.click(runsToggle());
    expect(props.onSelectBottomSurface).toHaveBeenCalledWith("runs");
  });

  it("stands whether or not the toggle is active", () => {
    // A hidden bottom panel and an open one showing another surface each say
    // the same thing, and so does an open one showing Runs.
    renderBar({ runsAttention: "on", bottomVisible: true, bottomSurface: "runs" });
    const toggle = runsToggle();
    expect(toggle).toHaveAttribute("data-active", "true");
    expect(toggle).toHaveAttribute("data-attention", "on");
  });

  it("renders no mark when the facility holds none", () => {
    renderBar();
    expect(runsToggle()).not.toHaveAttribute("data-attention");
    expect(runsToggle().getAttribute("aria-label")).toBe("Runs");
  });
});
