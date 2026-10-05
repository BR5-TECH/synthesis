import { useEffect, useRef } from "react";
import {
  MENU_ABOUT,
  MENU_CLOSE_PROJECT,
  MENU_EXIT_REQUESTED,
  MENU_FIND,
  MENU_FIND_REPLACE,
  MENU_NEW_ARTIFACT,
  MENU_NEW_FILE,
  MENU_NEW_FOLDER,
  MENU_SAVE,
  MENU_SAVE_ALL,
  onMenuEvent,
  type MenuEvent,
} from "../events";

export interface MenuHandlers {
  /** SNV-FR-24: open the New File modal (NFI-new-file.md). */
  newFile: () => void;
  newFolder: () => void;
  newArtifact: () => void;
  /** SNV-FR-29: write the active tab's content. */
  save: () => void;
  /** SNV-FR-31: write everything holding unsaved changes. */
  saveAll: () => void;
  closeProject: () => void;
  /** SNV-FR-26: the app is quitting and is held until the frontend releases it. */
  exit: () => void;
  /** SNV-FR-43 / EFR-FR-AYNZ: toggle the active Editor's Find panel. */
  find: () => void;
  /** SNV-FR-43 / EFR-FR-BJUY: toggle its Find & Replace panel. */
  findReplace: () => void;
  /** ABT-FR-KMVD: open the About panel. Relayed whether or not a project is open. */
  about: () => void;
}

/**
 * SNV-FR-23..31: subscribe once to the native File-menu events the Rust shell
 * relays. A latest-ref holds the handlers so the listeners subscribe exactly
 * once and always invoke the current closures (which depend on per-render state)
 * without re-subscribing. The unlisten fns are captured via a cancel flag so a
 * listener that resolves after unmount is detached immediately.
 *
 * The Save and Find channels arrive only while their menu items are enabled — a
 * disabled native item swallows both the click and the accelerator (SNV-FR-28 /
 * SNV-FR-30 / SNV-FR-43) — so nothing here re-checks enablement.
 */
export function useMenuEvents(handlers: MenuHandlers): void {
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const reg = (event: MenuEvent, run: () => void) => {
      void onMenuEvent(event, run).then((fn) => {
        if (cancelled) fn();
        else unlisten.push(fn);
      });
    };
    reg(MENU_NEW_FILE, () => handlersRef.current.newFile());
    reg(MENU_NEW_FOLDER, () => handlersRef.current.newFolder());
    reg(MENU_NEW_ARTIFACT, () => handlersRef.current.newArtifact());
    reg(MENU_SAVE, () => handlersRef.current.save());
    reg(MENU_SAVE_ALL, () => handlersRef.current.saveAll());
    reg(MENU_CLOSE_PROJECT, () => handlersRef.current.closeProject());
    reg(MENU_EXIT_REQUESTED, () => handlersRef.current.exit());
    reg(MENU_FIND, () => handlersRef.current.find());
    reg(MENU_FIND_REPLACE, () => handlersRef.current.findReplace());
    reg(MENU_ABOUT, () => handlersRef.current.about());
    return () => {
      cancelled = true;
      unlisten.forEach((u) => u());
    };
  }, []);
}
