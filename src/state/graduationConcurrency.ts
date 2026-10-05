/**
 * The Graduation section's pending setting
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-QKKQ through
 * SET-FR-IHQE).
 *
 * The project-wide limit of graduation runs is a **pending change**: a choice
 * marks the section pending and nothing is written until the window saves, with
 * its other pending changes or when it closes (SET-FR-HQKK). The value lives
 * with the window rather than with the section on screen, because a section the
 * author has left is not mounted and the save-before-close sweep still has to
 * reach what it holds (SET-FR-IHQE).
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import { registerSettingsSection } from "./settingsSweep";
import type { GraduationConcurrencyLimit } from "../types";

/** The key of the section in its window (SWN-FR-11). */
export const GRADUATION_SECTION = "graduation";

/** SET-FR-QKKQ: the numbers the drop-down lists, before **Unlimited**. */
export const LISTED_LIMITS = [1, 2, 4, 8] as const;

/** PSS-FR-JRWC: the limit a project that stores none has. */
export const DEFAULT_LIMIT: GraduationConcurrencyLimit = 1;

/** One entry of the drop-down. */
export interface LimitChoice {
  value: GraduationConcurrencyLimit;
  label: string;
}

/** The select's value for a limit. `unlimited` is a word, never a number. */
export function limitKey(limit: GraduationConcurrencyLimit): string {
  return limit === "unlimited" ? "unlimited" : String(limit);
}

/** The inverse of {@link limitKey}. */
export function limitOfKey(key: string): GraduationConcurrencyLimit {
  return key === "unlimited" ? "unlimited" : Number(key);
}

/**
 * SET-FR-QKKQ / SET-FR-ZNXA: the choices, with a stored positive integer the
 * list does not hold added in numeric order, so it can be selected and is not
 * normalised to a value the author did not pick.
 */
export function limitChoices(
  stored: GraduationConcurrencyLimit | null,
): LimitChoice[] {
  const numbers: number[] = [...LISTED_LIMITS];
  if (typeof stored === "number" && !numbers.includes(stored)) {
    numbers.push(stored);
    numbers.sort((a, b) => a - b);
  }
  return [
    ...numbers.map((value) => ({ value, label: String(value) })),
    { value: "unlimited" as const, label: "Unlimited" },
  ];
}

/** A read that names a limit this build cannot use reads as the default. */
function readLimit(value: unknown): GraduationConcurrencyLimit {
  if (value === "unlimited") return "unlimited";
  if (typeof value === "number" && Number.isInteger(value) && value >= 1) {
    return value;
  }
  return DEFAULT_LIMIT;
}

function messageOf(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

/** What the section renders from (SET-FR-TFNG). */
export interface GraduationConcurrencySetting {
  /** `loading` until the read lands, `error` where it failed. */
  load: "loading" | "ready" | "error";
  loadError: string | null;
  /** The value last read or written. The drop-down lists it (SET-FR-ZNXA). */
  stored: GraduationConcurrencyLimit | null;
  /** What the drop-down shows. */
  selected: GraduationConcurrencyLimit | null;
  saving: boolean;
  saveError: string | null;
  /** Whether the author chose a value that is not written yet. */
  pending: boolean;
  select: (value: GraduationConcurrencyLimit) => void;
  /** Write the pending choice. Resolves `true` once it landed. */
  save: () => Promise<boolean>;
}

/**
 * Own the Graduation section's value for the life of the window, and publish it
 * to the save-before-close sweep (SET-FR-HQKK, SWN-FR-08, SWN-FR-09).
 *
 * `contentRoot` keys the read, so a window opened against another project or
 * worktree never shows or writes the previous one's value.
 */
export function useGraduationConcurrencySetting(
  contentRoot: string,
): GraduationConcurrencySetting {
  const [load, setLoad] = useState<"loading" | "ready" | "error">("loading");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [stored, setStored] = useState<GraduationConcurrencyLimit | null>(null);
  const [selected, setSelected] = useState<GraduationConcurrencyLimit | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // The sweep reads these at the moment it asks, so they are refs: a closure
  // over render state would answer for the render the section registered in.
  const storedRef = useRef(stored);
  storedRef.current = stored;
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  const inFlight = useRef<Promise<boolean> | null>(null);

  // SET-FR-ZNXA / SET-FR-TFNG: the read, with a loading state until it lands.
  // A read that fails offers no choice, so a damaged store is never shown as a
  // limit of one and never overwritten by it.
  useEffect(() => {
    let cancelled = false;
    setLoad("loading");
    setLoadError(null);
    void Promise.resolve()
      .then(() => api.loadProjectConfig())
      .then((config) => {
        if (cancelled) return;
        if (!config) throw new Error("the project configuration could not be read");
        const limit = readLimit(config.graduationConcurrencyLimit);
        setStored(limit);
        setSelected(limit);
        setLoad("ready");
      })
      .catch((reason) => {
        if (cancelled) return;
        logWarn(["frontend"], "the graduation concurrency limit could not be read", {});
        setLoadError(messageOf(reason));
        setLoad("error");
      });
    return () => {
      cancelled = true;
    };
  }, [contentRoot]);

  const isPending = () =>
    inFlight.current !== null ||
    (selectedRef.current !== null && selectedRef.current !== storedRef.current);

  const save = useCallback((): Promise<boolean> => {
    // SWN-FR-09: a write already in flight is awaited, never started again.
    if (inFlight.current) return inFlight.current;
    const wanted = selectedRef.current;
    if (wanted === null || wanted === storedRef.current) return Promise.resolve(true);
    setSaving(true);
    setSaveError(null);
    const write = api
      // SET-FR-IHQE: the write names the limit alone, so it carries every other
      // project-public section through unchanged (PSS-FR-ZVSD).
      .saveProjectConfig({ graduationConcurrencyLimit: wanted })
      .then(() => {
        storedRef.current = wanted;
        setStored(wanted);
        logInfo(["frontend"], "the graduation concurrency limit was saved", {
          unlimited: wanted === "unlimited",
          limit: wanted === "unlimited" ? null : wanted,
        });
        return true;
      })
      .catch((reason) => {
        // SET-FR-TFNG: the author's selection stays, and the section stays
        // pending so a retry writes it.
        logWarn(["frontend"], "the graduation concurrency limit could not be saved", {
          unlimited: wanted === "unlimited",
        });
        setSaveError(messageOf(reason));
        return false;
      })
      .finally(() => {
        inFlight.current = null;
        setSaving(false);
      });
    inFlight.current = write;
    return write;
  }, []);

  // SWN-FR-08: published for the life of the window, not of the section on
  // screen, so a pending choice is written on close from any section.
  useEffect(
    () =>
      registerSettingsSection({
        section: GRADUATION_SECTION,
        pending: isPending,
        save,
      }),
    [save],
  );

  const select = useCallback((value: GraduationConcurrencyLimit) => {
    selectedRef.current = value;
    setSelected(value);
    setSaveError(null);
  }, []);

  return {
    load,
    loadError,
    stored,
    selected,
    saving,
    saveError,
    pending: selected !== null && selected !== stored,
    select,
    save,
  };
}
