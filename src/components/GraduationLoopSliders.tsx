/**
 * The graduation loop sliders of the Project settings Graduation section
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-PSLD through
 * SET-FR-OVTW).
 *
 * Three sliders set the pass budget, the stage time limit and the total time
 * limit of `PSS-project-settings-storage.md` (PSS-FR-QBNE, PSS-FR-HVSM,
 * PSS-FR-RKTW). A change persists at once when the author releases the slider
 * or the key, so the section holds no dirty state for them and they take no
 * part in the save-before-close sweep (SET-FR-WTOS). Writes go one after
 * another, so quick changes are all kept.
 *
 * A run that stopped on a time limit routes here with the slider that sets the
 * limit named (GRU-FR-QLRQ, GRU-FR-TTLR). That slider receives the focus once
 * the stored values are read.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import {
  GRADUATION_LOOP_SETTINGS,
  loopPatch,
  loopRefusalCode,
  loopRefusalMessage,
  storedLoopValue,
  type GraduationLoopKey,
  type GraduationLoopSetting,
} from "../state/graduationLoopSettings";

/** SWN-FR-HZWD: a route asked for one slider to receive the focus. */
export interface GraduationFocusRequest {
  /** The address item that names the slider, for example `total-time-limit`. */
  item: string;
  nonce: number;
}

type PerKey<T> = Record<GraduationLoopKey, T>;

function perKey<T>(value: T): PerKey<T> {
  return {
    graduationPassBudget: value,
    graduationStageTimeLimitMs: value,
    graduationTotalTimeLimitMs: value,
  };
}

const STAGE = GRADUATION_LOOP_SETTINGS[1];
const TOTAL = GRADUATION_LOOP_SETTINGS[2];

export interface GraduationLoopSlidersProps {
  focusRequest?: GraduationFocusRequest | null;
  /** Called once the focus request has been honoured. */
  onFocusHandled?: () => void;
}

export function GraduationLoopSliders({
  focusRequest = null,
  onFocusHandled,
}: GraduationLoopSlidersProps) {
  /** What the store holds: `undefined` while the read is outstanding. */
  const [stored, setStored] = useState<PerKey<number | null> | undefined>(undefined);
  const [loadError, setLoadError] = useState("");
  /** The value while a slider moves, so a drag does not write each step. */
  const [drafts, setDrafts] = useState<PerKey<number | null>>(() => perKey(null));
  /**
   * The value of the last write that did not settle yet. `null` inside means a
   * reset to default. The control shows it, so it does not jump back while the
   * write is on its way.
   */
  const [pending, setPending] = useState<PerKey<number | null | undefined>>(() =>
    perKey(undefined),
  );
  const [errors, setErrors] = useState<PerKey<string>>(() => perKey(""));
  const mounted = useRef(true);
  /** SET-FR-WTOS: the writes, one after another. */
  const chain = useRef<Promise<unknown>>(Promise.resolve());
  /** The sequence number of the last write of each setting. */
  const latest = useRef<PerKey<number>>(perKey(0));
  const inputs = useRef(new Map<string, HTMLInputElement>());

  useEffect(() => {
    mounted.current = true;
    api
      .loadProjectConfig()
      .then((config) => {
        if (!mounted.current) return;
        setStored({
          graduationPassBudget: storedLoopValue(config, "graduationPassBudget"),
          graduationStageTimeLimitMs: storedLoopValue(config, "graduationStageTimeLimitMs"),
          graduationTotalTimeLimitMs: storedLoopValue(config, "graduationTotalTimeLimitMs"),
        });
      })
      .catch((e) => {
        if (mounted.current) setLoadError(String(e));
        logWarn(["frontend"], "the graduation loop settings could not be read", {
          reason: loopRefusalCode(e),
        });
      });
    return () => {
      mounted.current = false;
    };
  }, []);

  // GRU-FR-QLRQ / GRU-FR-TTLR: the slider a route named receives the focus
  // once the stored values are on screen.
  useEffect(() => {
    if (!focusRequest || stored === undefined) return;
    const input = inputs.current.get(focusRequest.item);
    if (input) {
      input.focus();
      input.scrollIntoView?.({ block: "nearest" });
    }
    onFocusHandled?.();
  }, [focusRequest, stored, onFocusHandled]);

  /**
   * SET-FR-WTOS / SET-FR-KAXB: persist one setting. `null` clears it to unset.
   * Each write reads the stored line-ending convention first, because the save
   * writes it every time (PSS-FR-17).
   */
  const persist = useCallback((setting: GraduationLoopSetting, value: number | null) => {
    const { key } = setting;
    const seq = ++latest.current[key];
    setPending((prev) => ({ ...prev, [key]: value }));
    setErrors((prev) => ({ ...prev, [key]: "" }));
    const settle = () => {
      if (latest.current[key] === seq) {
        setPending((prev) => ({ ...prev, [key]: undefined }));
      }
    };
    chain.current = chain.current.then(async () => {
      try {
        const current = await api.loadProjectConfig();
        await api.saveProjectConfig({
          lineEndings: current.lineEndings,
          ...loopPatch(key, value),
        });
        logInfo(["frontend"], "a graduation loop setting was saved", {
          setting: key,
          value,
        });
        if (!mounted.current) return;
        setStored((prev) => (prev ? { ...prev, [key]: value } : prev));
        settle();
      } catch (e) {
        logWarn(["frontend"], "a graduation loop setting was not saved", {
          setting: key,
          reason: loopRefusalCode(e),
        });
        if (!mounted.current) return;
        settle();
        setErrors((prev) => ({ ...prev, [key]: loopRefusalMessage(setting, e) }));
      }
    });
  }, []);

  if (loadError) {
    return (
      <p
        className="picker-error"
        style={{ display: "block", marginBottom: 18 }}
        data-testid="graduation-loop-load-error"
      >
        ✗ The pass and time limits could not be read: {loadError}
      </p>
    );
  }

  /** The value a slider shows: its drag, its write on the way, the store, or the default. */
  const shown = (setting: GraduationLoopSetting): number => {
    const draft = drafts[setting.key];
    if (draft !== null) return draft;
    const inFlight = pending[setting.key];
    if (inFlight !== undefined) return inFlight ?? setting.defaultValue;
    return stored?.[setting.key] ?? setting.defaultValue;
  };

  /** SET-FR-WTOS: a slider writes once, when the author lets it go. */
  const commit = (setting: GraduationLoopSetting) => {
    const draft = drafts[setting.key];
    if (draft === null) return;
    setDrafts((prev) => ({ ...prev, [setting.key]: null }));
    persist(setting, draft);
  };

  const loading = stored === undefined;
  const stageOverTotal = !loading && shown(STAGE) > shown(TOTAL);

  return (
    <div data-testid="graduation-loop-settings">
      {GRADUATION_LOOP_SETTINGS.map((setting) => {
        const value = shown(setting);
        const text = setting.format(value);
        const id = `graduation-${setting.item}`;
        const unset =
          pending[setting.key] === undefined
            ? stored?.[setting.key] === null
            : pending[setting.key] === null;
        return (
          <div key={setting.key} style={{ marginBottom: 20, maxWidth: 520 }}>
            <label className="t-label" htmlFor={id}>
              {setting.label} — {loading ? "…" : text}
            </label>
            <input
              id={id}
              ref={(el) => {
                if (el) inputs.current.set(setting.item, el);
                else inputs.current.delete(setting.item);
              }}
              type="range"
              data-testid={id}
              min={setting.min}
              max={setting.max}
              step={setting.step}
              value={value}
              disabled={loading}
              aria-valuetext={text}
              aria-describedby={`${id}-note`}
              onChange={(e) => {
                const next = Number(e.target.value);
                setDrafts((prev) => ({ ...prev, [setting.key]: next }));
              }}
              onPointerUp={() => commit(setting)}
              onKeyUp={() => commit(setting)}
              onBlur={() => commit(setting)}
              style={{
                display: "block",
                width: "100%",
                marginTop: 8,
                accentColor: "var(--accent)",
              }}
            />
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span className="t-ui-xs t-muted" style={{ flex: 1 }}>
                {setting.format(setting.min)} – {setting.format(setting.max)}
              </span>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                data-testid={`${id}-reset`}
                aria-label={`Reset ${setting.label} to default`}
                disabled={loading || unset}
                onClick={() => persist(setting, null)}
              >
                Reset to default
              </button>
            </div>
            <p
              id={`${id}-note`}
              className="t-ui-xs t-muted"
              data-testid={`${id}-note`}
              style={{ marginTop: 6 }}
            >
              {setting.helper} The default is {setting.format(setting.defaultValue)}.
            </p>
            {errors[setting.key] && (
              <p
                className="t-ui-xs"
                role="alert"
                data-testid={`${id}-error`}
                style={{ marginTop: 6, color: "var(--danger)" }}
              >
                {errors[setting.key]}
              </p>
            )}
          </div>
        );
      })}
      {/* SET-FR-OVTW: both values stay as they are; the section only says
          which limit ends the run first. */}
      {stageOverTotal && (
        <p
          className="t-ui-xs"
          role="status"
          data-testid="graduation-stage-over-total"
          style={{ marginBottom: 20, maxWidth: 520, color: "var(--warn)" }}
        >
          The time per turn is longer than the total time per run. The total time
          limit ends the run first.
        </p>
      )}
    </div>
  );
}
