import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { inflateSync } from "node:zlib";

import mark32 from "../../src-tauri/icons/synthesis-32x32.png";
import mark64 from "../../src-tauri/icons/synthesis-64x64.png";
import { TopChrome } from "./Shell";
import { ProjectPicker } from "./ProjectPicker";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetAgentRegistry } from "../state/agentRegistry";
import { resetLogBufferForTest } from "../logging";

// The top chrome and the picker both reach the Tauri core, event, window, and
// dialog plugins at module load. These scenarios check only the app brand, so
// the stubs answer what the two surfaces read on mount and nothing else.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    isFullscreen: async () => false,
    setFullscreen: async () => {},
    onResized: async () => () => {},
  }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}));

beforeEach(() => {
  resetAppPreferencesCache();
  resetAgentRegistry();
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_app_preferences") {
      return { theme: "system", mainWindowFullscreen: false };
    }
    if (cmd === "list_recent_projects") return [];
    if (cmd === "list_project_agents") return [];
    if (cmd === "list_agent_turns") return [];
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});

afterEach(cleanup);

function renderTopChrome() {
  return render(
    <TopChrome
      projectName="acme"
      projectPath="~/dev/acme"
      onSwitchProject={() => {}}
      onBeforeSwitchProject={async () => true}
      onOpenAnother={() => {}}
      onRequestMergeCommit={async () => null}
      onOpenChanges={() => {}}
      onOpenRun={() => {}}
      onOverlayOpening={() => {}}
      activeWorktree={null}
      onSwitchWorktree={async () => ({ ok: true })}
      themePref="system"
      onSelectTheme={() => {}}
      searchOpen={false}
      setSearchOpen={() => {}}
      onOpenSearchResults={() => {}}
      onActivateSearchHit={() => {}}
      onRequestGithubToken={async () => false}
      onOpenGlobalSettings={() => {}}
      onOpenAgentInSettings={() => {}}
      onOpenProjectSettings={() => {}}
      agentsRosterOpen={false}
      setAgentsRosterOpen={() => {}}
    />,
  );
}

/**
 * The element that holds the app mark. Checks that the mark is the
 * transparent application mark in both sizes, that it is decorative, and that
 * the name "Synthesis" follows it.
 */
function brandOf(icon: HTMLElement, size: number): HTMLElement {
  expect(icon.tagName).toBe("IMG");
  expect(icon.getAttribute("src")).toBe(mark32);
  expect(icon.getAttribute("srcset")).toBe(`${mark32} 32w, ${mark64} 64w`);
  expect(icon.getAttribute("sizes")).toBe(`${size}px`);
  expect(icon.getAttribute("width")).toBe(String(size));
  expect(icon.getAttribute("height")).toBe(String(size));
  // Decorative: the name beside it is the brand's accessible text.
  expect(icon.getAttribute("alt")).toBe("");
  expect(screen.queryAllByRole("img")).not.toContain(icon);
  expect(icon.nextElementSibling?.textContent).toBe("Synthesis");
  const brand = icon.parentElement!;
  expect(brand.firstElementChild).toBe(icon);
  return brand;
}

/**
 * The size of an 8-bit RGBA PNG and the share of fully transparent pixels
 * inside the box around its opaque pixels. A mark with no tile behind it
 * leaves a large share of that box transparent; a tile, also a small tile in
 * a wide margin, fills its box.
 */
function readPngTransparency(path: string) {
  const bytes = readFileSync(path);
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  // IHDR: bit depth 8, colour type 6 (RGBA), no interlace. Without alpha
  // there is no transparent background.
  expect([bytes[24], bytes[25], bytes[28]]).toEqual([8, 6, 0]);
  const idat: Buffer[] = [];
  for (let at = 8; at < bytes.length; ) {
    const length = bytes.readUInt32BE(at);
    if (bytes.toString("ascii", at + 4, at + 8) === "IDAT") {
      idat.push(bytes.subarray(at + 8, at + 8 + length));
    }
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * 4;
  expect(raw.length).toBe(height * (stride + 1));
  let prev = new Uint8Array(stride);
  const alpha: number[][] = [];
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    expect(filter).toBeLessThanOrEqual(4);
    const line = Uint8Array.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let x = 0; x < stride; x++) {
      const left = x >= 4 ? line[x - 4] : 0;
      const up = prev[x];
      const upLeft = x >= 4 ? prev[x - 4] : 0;
      let predictor = 0;
      if (filter === 1) predictor = left;
      else if (filter === 2) predictor = up;
      else if (filter === 3) predictor = (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const [pa, pb, pc] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - upLeft)];
        predictor = pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
      }
      line[x] = (line[x] + predictor) & 0xff;
    }
    alpha.push(Array.from({ length: width }, (_, x) => line[x * 4 + 3]));
    prev = line;
  }
  let [left, top, right, bottom] = [width, height, -1, -1];
  alpha.forEach((row, y) =>
    row.forEach((a, x) => {
      if (a === 0) return;
      [left, top] = [Math.min(left, x), Math.min(top, y)];
      [right, bottom] = [Math.max(right, x), Math.max(bottom, y)];
    }),
  );
  let transparent = 0;
  for (let y = top; y <= bottom; y++) {
    for (let x = left; x <= right; x++) if (alpha[y][x] === 0) transparent++;
  }
  const box = (right - left + 1) * (bottom - top + 1);
  return { width, height, transparentShare: transparent / box };
}

describe("app brand", () => {
  it("SNV-FR-EZQB: the top chrome brand shows the application mark, then the name", async () => {
    const { container } = renderTopChrome();
    const brand = brandOf(screen.getByTestId("app-icon"), 16);

    expect(brand).toHaveClass("top-chrome__brand");
    // The legacy monochrome mark is gone, not drawn beside the new icon.
    expect(brand.querySelector("svg")).toBeNull();
    // The brand sits at the leading edge, before the project switcher.
    const leading = container.querySelector(".top-chrome__left")!;
    expect(leading.firstElementChild).toBe(brand);
    const switcher = screen.getByText("acme");
    expect(
      brand.compareDocumentPosition(switcher) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // Let the agents control finish its mount-time load inside this test.
    await waitFor(() =>
      expect(invokeMock.mock.calls.map(([cmd]) => cmd)).toContain("list_project_agents"),
    );
  });

  it.each([
    ["src-tauri/icons/synthesis-32x32.png", 32],
    ["src-tauri/icons/synthesis-64x64.png", 64],
  ])("SNV-FR-EZQB: %s has its size and no tile behind the mark", (path, size) => {
    const png = readPngTransparency(path);
    expect(png.width).toBe(size);
    expect(png.height).toBe(size);
    expect(png.transparentShare).toBeGreaterThan(0.1);
  });

  it("SNV-FR-EZQB: the tile check rejects the tiled bundle icon", () => {
    expect(readPngTransparency("src-tauri/icons/32x32.png").transparentShare).toBeLessThan(0.1);
  });

  it("PPK-FR-DYKZ: the picker header shows the same application mark, then the name", async () => {
    render(<ProjectPicker onOpen={vi.fn()} />);
    const head = brandOf(screen.getByTestId("app-icon"), 32);

    expect(head).toHaveClass("picker-head");
    expect(head.querySelector("svg")).toBeNull();
    // Let the picker finish its mount-time load inside this test.
    await screen.findByText(/no projects yet/i);
  });
});
