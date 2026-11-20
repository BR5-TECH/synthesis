/**
 * The native application menu channels. The id of each item is also its event
 * name, and these match the MENU_* constants of `src-tauri/src/menu/`
 * byte-for-byte.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Native File-menu activations (SNV-FR-24 / SNV-FR-25). The id doubles as the
// event name; these match `menu.rs`'s MENU_* constants byte-for-byte.
export const MENU_NEW_FILE = "menu:new-file";
export const MENU_NEW_FOLDER = "menu:new-folder";
export const MENU_NEW_ARTIFACT = "menu:new-artifact";
/** SNV-FR-28 / SNV-FR-29: write the active tab's content. */
export const MENU_SAVE = "menu:save";
/** SNV-FR-30 / SNV-FR-31: write everything holding unsaved changes. */
export const MENU_SAVE_ALL = "menu:save-all";
export const MENU_CLOSE_PROJECT = "menu:close-project";
/** PJT-FR-PILG: open the Project picker as a modal over this window. */
export const MENU_NEW_TAB = "menu:new-tab";
/**
 * SNV-FR-26: the application is quitting. The backend holds the exit while the
 * frontend writes pending Editor changes (EDT-FR-33) and then either releases it
 * and then answers with `finish_exit` — proceeding, or cancelling the quit when
 * a write was blocked (EDT-FR-32).
 */
export const MENU_EXIT_REQUESTED = "menu:exit-requested";
/** SNV-FR-43 / EFR-FR-AYNZ: open, switch to, or toggle the Editor's Find panel. */
export const MENU_FIND = "menu:find";
/** SNV-FR-43 / EFR-FR-BJUY: the same for the Find & Replace panel. */
export const MENU_FIND_REPLACE = "menu:find-replace";
/** SNV-FR-23 / ABT-FR-KMVD: open the About panel in the window that is showing. */
export const MENU_ABOUT = "menu:about";

/** SNV-FR-LVDP / EZM-FR-DMLT: Window -> Zoom -> Zoom In. */
export const MENU_ZOOM_IN = "menu:zoom-in";
/** SNV-FR-LVDP / EZM-FR-DMLT: Window -> Zoom -> Zoom Out. */
export const MENU_ZOOM_OUT = "menu:zoom-out";
/** SNV-FR-LVDP / EZM-FR-DMLT: Window -> Zoom -> Actual Size. */
export const MENU_ZOOM_RESET = "menu:zoom-reset";
/** SNV-FR-LVDP: a step item of Window -> Zoom, with its step in percent. */
export const MENU_ZOOM_SET = "menu:zoom-set";

/** A payload-less native File-menu channel (SNV-FR-23..31). */
export type MenuEvent =
  | typeof MENU_NEW_FILE
  | typeof MENU_NEW_FOLDER
  | typeof MENU_NEW_ARTIFACT
  | typeof MENU_SAVE
  | typeof MENU_SAVE_ALL
  | typeof MENU_CLOSE_PROJECT
  | typeof MENU_NEW_TAB
  | typeof MENU_EXIT_REQUESTED
  | typeof MENU_FIND
  | typeof MENU_FIND_REPLACE
  | typeof MENU_ABOUT
  | typeof MENU_ZOOM_IN
  | typeof MENU_ZOOM_OUT
  | typeof MENU_ZOOM_RESET;

/** Subscribe to a payload-less native File-menu event (SNV-FR-23..31). */
export const onMenuEvent = (
  event: MenuEvent,
  run: () => void,
): Promise<UnlistenFn> => listen(event, () => run());

/** SNV-FR-LVDP: the payload of a step item of the Zoom submenu. */
export interface MenuZoomSetPayload {
  percent: number;
}

/** Subscribe to the step items of the Zoom submenu (SNV-FR-LVDP). */
export const onMenuZoomSet = (
  run: (percent: number) => void,
): Promise<UnlistenFn> =>
  listen<MenuZoomSetPayload>(MENU_ZOOM_SET, (event) => run(event.payload.percent));
