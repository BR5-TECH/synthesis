import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { emitLineEndingsChanged, onLineEndingsChanged } from "../events";
import type { LineEndings } from "../types";

export const LINE_ENDING_CHOICES: [LineEndings, string][] = [
  ["lf", "LF"],
  ["crlf", "CRLF"],
];

/**
 * Owns the single shared project-wide line-ending convention (PSS-FR-17).
 *
 * Two equal entry points edit one value (STB-FR-19 / SET-FR-11): the status
 * bar's control (STB-FR-16) and the Project settings window's Project section
 * (SET-FR-10). Both go through `selectLineEndings` here, which is what makes
 * each reflect a change made from the other without a reload — the same shape
 * the shared theme preference uses (SNV-FR-15).
 *
 * `onChanged` fires after a selection is persisted, so the caller can mark every
 * open Editor tab dirty (EDT-FR-41 / STB-FR-18). It deliberately does *not* fire
 * on the initial load: opening a project must not dirty anything.
 *
 * `null` means no project is open (or the config could not be read), in which
 * case neither control renders a convention.
 */
export function useLineEndings(
  projectPath: string,
  onChanged?: () => void,
) {
  const [lineEndings, setLineEndings] = useState<LineEndings | null>(null);
  // A latest-ref so the cross-window subscription below is established once and
  // still calls the current callback — re-subscribing per render would drop the
  // announcement that arrived in the gap.
  const onChangedRef = useRef(onChanged);
  onChangedRef.current = onChanged;
  // The applied value, read by that subscription rather than closed over: the
  // listener is established once and would otherwise compare against whatever
  // the value was on first render.
  const lineEndingsRef = useRef<LineEndings | null>(lineEndings);
  lineEndingsRef.current = lineEndings;

  useEffect(() => {
    if (!projectPath) {
      setLineEndings(null);
      return;
    }
    let cancelled = false;
    void api
      .loadProjectConfig()
      .then((config) => {
        if (cancelled) return;
        const value = config?.lineEndings;
        setLineEndings(value === "lf" || value === "crlf" ? value : null);
      })
      .catch(() => {
        // PSS-FR-10: a damaged `project.toml` is a typed error. The Project
        // settings window is where that gets surfaced (SET-FR-09); the status bar
        // is a strip of ambient facts, so it simply shows no convention rather
        // than an error message.
        if (!cancelled) setLineEndings(null);
      });
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  /**
   * STB-FR-17 / SET-FR-10: persist the selection immediately rather than
   * waiting for a section save, so both entry points behave identically and the
   * control sits outside the Project section's dirty state (SET-FR-08).
   *
   * On a persistence failure the applied value is rolled back, so neither
   * control shows a convention that will not survive a relaunch — and, more
   * importantly, no tab is marked dirty for a conversion that will not happen.
   */
  const selectLineEndings = (next: LineEndings) => {
    const previous = lineEndings;
    if (next === previous) return;
    setLineEndings(next);
    // Advanced here rather than left to the next render: the announcement below
    // reaches this window too, and it goes out on the write's own promise —
    // before React has committed the state change. A ref still reading the old
    // value would make this window treat its own announcement as the other
    // window's and mark every open tab dirty a second time.
    lineEndingsRef.current = next;
    void api
      .saveProjectConfig({ lineEndings: next })
      .then(() => {
        onChanged?.();
        // SET-FR-11 / SWN-FR-01: the two controls that edit this one value are
        // in two different windows now — the status bar in the main window, the
        // Project section in the Project settings child window. Each reflects a
        // change made from the other "without a reload", which since the split
        // means hearing about it.
        void emitLineEndingsChanged(next);
      })
      .catch(() => {
        setLineEndings(previous);
        lineEndingsRef.current = previous;
      });
  };

  /**
   * SET-FR-11 / STB-FR-18: adopt a convention the other window persisted.
   *
   * `onChanged` fires here too, deliberately: a change made in the Project
   * settings window marks the artifact of every open Editor tab dirty exactly
   * as one made from the status bar does, and those tabs are in this window.
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onLineEndingsChanged((value) => {
      // The announcement reaches every window including the one that made it,
      // where it is already applied — so the window that wrote it does nothing
      // and, in particular, does not mark its own tabs dirty a second time.
      if (cancelled || lineEndingsRef.current === value) return;
      lineEndingsRef.current = value;
      setLineEndings(value);
      onChangedRef.current?.();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return { lineEndings, selectLineEndings };
}
