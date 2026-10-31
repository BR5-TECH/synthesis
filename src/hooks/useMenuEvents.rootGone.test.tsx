/** The native menu while the worktree gone dialog is open (WTS-FR-ZHLG). */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

import { useMenuEvents, type MenuHandlers } from "./useMenuEvents";
import { MENU_ABOUT, MENU_CLOSE_PROJECT, MENU_EXIT_REQUESTED, MENU_NEW_FILE } from "../events";
import { applyActiveRootState, resetActiveRootHealth } from "../state/activeRootHealth";

type EventHandler = (ev: { payload: unknown }) => void;
let handlers: Record<string, EventHandler> = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers[name] = handler;
    return Promise.resolve(() => {});
  },
  emit: () => Promise.resolve(),
}));

const menuHandlers = (): MenuHandlers => ({
  newFile: vi.fn(),
  newFolder: vi.fn(),
  newArtifact: vi.fn(),
  save: vi.fn(),
  saveAll: vi.fn(),
  closeProject: vi.fn(),
  exit: vi.fn(),
  find: vi.fn(),
  findReplace: vi.fn(),
  about: vi.fn(),
});

async function mounted() {
  const menu = menuHandlers();
  renderHook(() => useMenuEvents(menu));
  await waitFor(() => expect(handlers[MENU_ABOUT]).toBeDefined());
  return menu;
}

const fire = (name: string) => handlers[name]({ payload: undefined });

beforeEach(() => {
  handlers = {};
  resetActiveRootHealth();
});
afterEach(cleanup);

describe("the menu while the root is gone", () => {
  it("WTS-FR-ZHLG: every menu item acts while the root is alive", async () => {
    const menu = await mounted();

    fire(MENU_ABOUT);
    fire(MENU_NEW_FILE);

    expect(menu.about).toHaveBeenCalledTimes(1);
    expect(menu.newFile).toHaveBeenCalledTimes(1);
  });

  it("WTS-FR-ZHLG: no item acts while the dialog is open, so no overlay opens behind it", async () => {
    const menu = await mounted();
    applyActiveRootState({ state: "gone", primaryPath: "~/dev/acme" });

    fire(MENU_ABOUT);
    fire(MENU_NEW_FILE);

    expect(menu.about).not.toHaveBeenCalled();
    expect(menu.newFile).not.toHaveBeenCalled();
  });

  it("WTS-FR-ZHLG: Close Project and Exit still act", async () => {
    const menu = await mounted();
    applyActiveRootState({ state: "gone", primaryPath: "~/dev/acme" });

    fire(MENU_CLOSE_PROJECT);
    fire(MENU_EXIT_REQUESTED);

    expect(menu.closeProject).toHaveBeenCalledTimes(1);
    expect(menu.exit).toHaveBeenCalledTimes(1);
  });
});
