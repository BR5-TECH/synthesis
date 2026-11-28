/**
 * Global settings → **Documents** (GLS-FR-IXAQ, GLS-FR-NOWF, GLS-FR-KWVN,
 * GLS-FR-PBXM, GLS-FR-MRGQ, GLS-FR-DCMT).
 *
 * The **Page width** of the document page (`EPW-editor-page-width.md`), which
 * applies at once to every open page, and the pages of a DOCX export: the **Landscape pages for wide diagrams**
 * switch, the **Landscape below** threshold, and the four **Page margins**.
 * The export reads them when it starts (`DXP-document-export.md`
 * DXP-FR-PGMR, DXP-FR-LQWZ).
 *
 * The section holds no dirty state. The switch saves at once, and each field
 * commits on Enter and on blur. It writes through `patchAppPreferences`, which
 * carries every other field of the record through unchanged (GSS-FR-20).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  loadAppPreferences,
  patchAppPreferences,
  updateAppPreferences,
} from "../state/appPreferences";
import { logWarn } from "../logging";
import { useEditorPageWidth, useEditorPageWidthSync } from "../hooks/useEditorPageWidth";
import {
  PAGE_WIDTH_DEFAULT,
  PAGE_WIDTH_MAX,
  PAGE_WIDTH_MIN,
  commitEditorPageWidth,
  getEditorPageWidth,
  restoreEditorPageWidth,
} from "../state/editorPageWidth";
import type { DocxMarginsMm } from "../types";
import { limitToCommit } from "./AiApiPercentLimitRow";

/** GSS-FR-TLHZ: the threshold range and its default. */
export const THRESHOLD_MIN = 10;
export const THRESHOLD_MAX = 100;
export const THRESHOLD_DEFAULT = 75;
/** GSS-FR-MGNV: the margin range and the default margins. */
export const MARGIN_MIN = 5;
export const MARGIN_MAX = 60;
export const MARGINS_DEFAULT: DocxMarginsMm = { top: 35, bottom: 30, left: 30, right: 30 };

type Side = keyof DocxMarginsMm;

/** The page outline of the margins preview: A4, 210 by 297 mm. */
const PAGE_W = 70;
const PAGE_H = 99;
const MM_TO_PX = PAGE_W / 210;

/** A stored margin, or its default where the record has none in range. */
function marginsOf(stored: Partial<DocxMarginsMm> | undefined): DocxMarginsMm {
  const side = (s: Side) => {
    const v = stored?.[s];
    return typeof v === "number" && v >= MARGIN_MIN && v <= MARGIN_MAX ? v : MARGINS_DEFAULT[s];
  };
  return { top: side("top"), bottom: side("bottom"), left: side("left"), right: side("right") };
}

interface WholeFieldProps {
  id: string;
  label: string;
  suffix: string;
  /** The stored value, or `null` until the first read answers. */
  stored: number | null;
  min: number;
  max: number;
  disabled?: boolean;
  /** The value the field shows while the author types, for the preview. */
  onDraft?: (value: number | undefined) => void;
  /** Saves `value`. A rejected promise restores the stored value. */
  save: (value: number) => Promise<void>;
  /**
   * Receives the message of the field, or `null`, where the parent shows it
   * in a place of its own. Without it the field shows its message below it.
   */
  onMessage?: (message: string | null) => void;
}

/**
 * GLS-FR-DCMT: a whole-number field that commits on Enter and on blur. A
 * value that is empty, not whole, or out of range shows an inline message and
 * commits nothing. A refused save restores the stored value and shows the
 * refusal inline.
 */
function WholeField({
  id,
  label,
  suffix,
  stored,
  min,
  max,
  disabled,
  onDraft,
  save,
  onMessage,
}: WholeFieldProps) {
  /** The typed text, or `null` while the field shows the stored value. */
  const [edit, setEdit] = useState<string | null>(null);
  const [error, setError] = useState("");
  /** A commit on its way, so Enter and the blur after it send one save. */
  const inFlight = useRef(false);
  const shown = stored === null ? "" : String(stored);
  const text = edit ?? shown;

  const unchanged = text.trim() === shown;
  const toCommit = limitToCommit(text, min, max);
  const invalid = !unchanged && toCommit === undefined;
  const message = invalid ? `Enter a whole number from ${min} to ${max}.` : error ? `✗ ${error}` : null;

  useEffect(() => {
    onMessage?.(message);
  }, [message, onMessage]);

  const commit = async () => {
    if (inFlight.current || stored === null) return;
    if (unchanged || toCommit === undefined || toCommit === stored) {
      setEdit(null);
      onDraft?.(undefined);
      return;
    }
    setError("");
    inFlight.current = true;
    try {
      await save(toCommit);
      setEdit(null);
    } catch (e) {
      setEdit(null);
      setError(`The value could not be saved: ${e instanceof Error ? e.message : String(e)}`);
      logWarn(["frontend"], `document setting ${label.toLowerCase()} not saved`, {
        value: toCommit,
        error: String(e),
      });
    } finally {
      inFlight.current = false;
      onDraft?.(undefined);
    }
  };

  return (
    <div className="document-settings__field">
      <label className="picker-field__label" htmlFor={id}>
        {label}
      </label>
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        <input
          id={id}
          className="input"
          type="text"
          inputMode="numeric"
          autoComplete="off"
          data-testid={id}
          aria-invalid={invalid || undefined}
          aria-describedby={message ? `${id}-message` : undefined}
          disabled={stored === null || disabled}
          style={{ width: 56, borderColor: invalid ? "var(--danger)" : undefined }}
          value={text}
          onChange={(e) => {
            setEdit(e.target.value);
            onDraft?.(limitToCommit(e.target.value, min, max));
          }}
          onBlur={() => void commit()}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !invalid) void commit();
          }}
        />
        <span className="t-ui-xs t-muted">{suffix}</span>
      </div>
      {message && !onMessage && (
        <span id={`${id}-message`} className="picker-error" style={{ display: "block", marginTop: 4 }}>
          {message}
        </span>
      )}
    </div>
  );
}

/**
 * GLS-FR-MRGQ: an A4 outline with the text area of the margins inside it, so
 * the author sees which side each field sets and how much room is left.
 */
function MarginsPreview({ margins }: { margins: DocxMarginsMm }) {
  return (
    <div
      className="document-settings__page"
      aria-hidden="true"
      style={{ width: PAGE_W, height: PAGE_H }}
    >
      <div
        className="document-settings__text-area"
        style={{
          top: margins.top * MM_TO_PX,
          bottom: margins.bottom * MM_TO_PX,
          left: margins.left * MM_TO_PX,
          right: margins.right * MM_TO_PX,
        }}
      />
    </div>
  );
}

export function DocumentSettings() {
  // Null until the stored record has landed, so no control asserts a value
  // nobody chose.
  // GLS-FR-IXAQ / EPW-FR-ZCBT: the field shows the width of the page width
  // store of this window, which reads the stored width and follows the saves
  // of the other windows. Null until the first read answers.
  useEditorPageWidthSync();
  const storeWidth = useEditorPageWidth();
  const [widthLoaded, setWidthLoaded] = useState(false);
  const pageWidth = widthLoaded ? storeWidth : null;
  const [pageWidthError, setPageWidthError] = useState<string | null>(null);
  const [landscape, setLandscape] = useState<boolean | null>(null);
  const [threshold, setThreshold] = useState<number | null>(null);
  const [margins, setMargins] = useState<DocxMarginsMm | null>(null);

  const [drafts, setDrafts] = useState<Partial<DocxMarginsMm>>({});
  const [switchError, setSwitchError] = useState<string | null>(null);
  /**
   * The messages of the margin fields, shown below the margins so that a
   * message does not move the fields around the page outline.
   */
  const [marginMessages, setMarginMessages] = useState<Partial<Record<Side, string>>>({});
  const reportMessage = useMemo(() => {
    const of = (side: Side) => (message: string | null) =>
      setMarginMessages((m) => {
        if ((m[side] ?? null) === message) return m;
        const next = { ...m };
        if (message) next[side] = message;
        else delete next[side];
        return next;
      });
    return { top: of("top"), bottom: of("bottom"), left: of("left"), right: of("right") };
  }, []);

  // GLS-FR-KWVN, GLS-FR-PBXM, GLS-FR-MRGQ: initialised from "load app
  // preferences" on mount.
  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (cancelled) return;
      // GLS-FR-IXAQ: the sync hook adopted the same record, which is cached.
      setWidthLoaded(true);
      // GSS-FR-JTZE: absent means never chosen, which is off.
      setLandscape(prefs.docxLandscapeDiagrams === true);
      const t = prefs.docxLandscapeThresholdPercent;
      setThreshold(
        typeof t === "number" && t >= THRESHOLD_MIN && t <= THRESHOLD_MAX ? t : THRESHOLD_DEFAULT,
      );
      setMargins(marginsOf(prefs.docxMarginsMm));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // GLS-FR-KWVN: the switch saves at once and holds no dirty state.
  const onToggle = useCallback(async (next: boolean) => {
    setSwitchError(null);
    setLandscape(next);
    try {
      await patchAppPreferences({ docxLandscapeDiagrams: next });
    } catch (e) {
      setLandscape(!next);
      const message = e instanceof Error ? e.message : "could not save";
      setSwitchError(message);
      logWarn(["frontend"], "document export landscape switch not saved", {
        requested: next,
        error: message,
      });
    }
  }, []);

  // GLS-FR-IXAQ / GLS-FR-DCMT / EPW-FR-XOMR: the width goes through the page
  // width store, so the pages of this window show it at once and the other
  // windows hear of it with the save.
  // A refused save restores the stored width (GLS-FR-DCMT).
  const savePageWidth = async (value: number) => {
    const before = getEditorPageWidth();
    try {
      await commitEditorPageWidth(value);
    } catch (e) {
      restoreEditorPageWidth(before);
      throw e;
    }
  };

  // GLS-FR-NOWF: Reset sets 820 and saves at once.
  const resetPageWidth = useCallback(async () => {
    setPageWidthError(null);
    const before = getEditorPageWidth();
    try {
      await commitEditorPageWidth(PAGE_WIDTH_DEFAULT);
    } catch (e) {
      restoreEditorPageWidth(before);
      setPageWidthError(
        `The value could not be saved: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }, []);

  const saveThreshold = async (value: number) => {
    await patchAppPreferences({ docxLandscapeThresholdPercent: value });
    setThreshold(value);
  };

  // GSS-FR-20, GLS-FR-DCMT: a save changes its own side of the margins that
  // are on disk when it runs. Two fields can commit back to back, and a refused
  // save of one never reaches disk through the save of the other.
  const saveMargin = (side: Side) => async (value: number) => {
    await updateAppPreferences((current) => ({
      docxMarginsMm: { ...marginsOf(current.docxMarginsMm), [side]: value },
    }));
    setMargins((m) => (m ? { ...m, [side]: value } : m));
  };

  const draft = (side: Side) => (value: number | undefined) =>
    setDrafts((d) => ({ ...d, [side]: value }));

  const preview: DocxMarginsMm = {
    ...(margins ?? MARGINS_DEFAULT),
    ...Object.fromEntries(Object.entries(drafts).filter(([, v]) => v !== undefined)),
  };

  const marginField = (side: Side, label: string) => (
    <WholeField
      id={`document-margin-${side}`}
      label={label}
      suffix="mm"
      stored={margins ? margins[side] : null}
      min={MARGIN_MIN}
      max={MARGIN_MAX}
      onDraft={draft(side)}
      save={saveMargin(side)}
      onMessage={reportMessage[side]}
    />
  );

  const marginLabels: [Side, string][] = [
    ["top", "Top"],
    ["left", "Left"],
    ["right", "Right"],
    ["bottom", "Bottom"],
  ];

  return (
    <div data-testid="document-settings">
      <h3 className="t-ui-sm" style={{ marginTop: 0, marginBottom: 12, fontWeight: 600 }}>
        Document page
      </h3>
      <div className="document-settings__page-width">
        <WholeField
          id="document-page-width"
          label="Page width"
          suffix="px"
          stored={pageWidth}
          min={PAGE_WIDTH_MIN}
          max={PAGE_WIDTH_MAX}
          save={savePageWidth}
        />
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          data-testid="document-page-width-reset"
          disabled={pageWidth === null || pageWidth === PAGE_WIDTH_DEFAULT}
          onClick={() => void resetPageWidth()}
        >
          Reset
        </button>
      </div>
      {pageWidthError && (
        <p className="picker-error" style={{ marginTop: 4 }} role="alert">
          ✗ {pageWidthError}
        </p>
      )}
      <p className="t-ui-xs t-muted" style={{ marginTop: 12, maxWidth: 520 }}>
        The largest width of the text on the document page at 100% zoom. You can also drag the
        ruler below the editor to set it.
      </p>

      <h3 className="t-ui-sm" style={{ marginTop: 28, marginBottom: 12, fontWeight: 600 }}>
        DOCX export
      </h3>
      <p className="t-p" style={{ marginBottom: 20 }}>
        Set the pages of a DOCX export, so that the diagrams in it stay readable.
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <input
          type="checkbox"
          checked={landscape ?? false}
          disabled={landscape === null}
          onChange={(e) => void onToggle(e.target.checked)}
        />
        <span>Landscape pages for wide diagrams</span>
      </label>
      <p className="t-ui-xs t-muted" style={{ marginTop: 12, maxWidth: 520 }}>
        A diagram that a portrait page shows at less than the share below of its size goes on a
        landscape page of its own. A tall diagram stays on a portrait page.
      </p>
      {switchError && (
        <p className="t-ui-xs" style={{ marginTop: 12, color: "var(--danger)" }} role="alert">
          {switchError}
        </p>
      )}

      <div style={{ marginTop: 16 }}>
        <WholeField
          id="document-landscape-below"
          label="Landscape below"
          suffix="% of the diagram's size"
          stored={threshold}
          min={THRESHOLD_MIN}
          max={THRESHOLD_MAX}
          disabled={landscape !== true}
          save={saveThreshold}
        />
      </div>

      <h3 className="t-ui-sm" style={{ marginTop: 28, marginBottom: 12, fontWeight: 600 }}>
        Page margins
      </h3>
      <div className="document-settings__margins">
        <div style={{ gridArea: "top" }}>{marginField("top", "Top")}</div>
        <div style={{ gridArea: "left" }}>{marginField("left", "Left")}</div>
        <div style={{ gridArea: "page" }}>
          <MarginsPreview margins={preview} />
        </div>
        <div style={{ gridArea: "right" }}>{marginField("right", "Right")}</div>
        <div style={{ gridArea: "bottom" }}>{marginField("bottom", "Bottom")}</div>
      </div>
      {marginLabels.map(
        ([side, label]) =>
          marginMessages[side] && (
            <span
              key={side}
              id={`document-margin-${side}-message`}
              className="picker-error"
              style={{ display: "block", marginTop: 8 }}
            >
              {label}: {marginMessages[side]}
            </span>
          ),
      )}
      <p className="t-ui-xs t-muted" style={{ marginTop: 12, maxWidth: 520 }}>
        The margins of every page of a DOCX export, portrait and landscape. Smaller margins leave
        more room, so pictures show larger. A PDF export keeps its own margins.
      </p>
    </div>
  );
}
