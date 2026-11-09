/**
 * The Flow sync group of the GitHub Project settings section
 * (`SET-project-settings.md` SET-FR-ZJRC, SET-FR-UFXD, SET-FR-PQWW, served by
 * `../../specifications/core/FTS-flow-tracker-sync.md` FTS-FR-LMBE).
 *
 * It holds the issue Type that each issue kind of a delivery plan is created
 * with on GitHub. Each change persists at once, outside the section dirty
 * state, so the group holds nothing to save.
 */
import { useCallback, useEffect, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import { githubPollingErrorMessage, refusalCode } from "../state/githubPolling";
import type { FlowTrackerSettings, MetadataList, PublicationIssueType } from "../types";
import { KIND_GLYPHS } from "./Flow/kindGlyphs";

/** SET-FR-ZJRC: the five issue kinds, in hierarchy order. */
const KINDS: { key: keyof FlowTrackerSettings; kind: "deliverable" | "epic" | "feature" | "task" | "defect"; label: string }[] = [
  { key: "deliverableType", kind: "deliverable", label: "Deliverable" },
  { key: "epicType", kind: "epic", label: "Epic" },
  { key: "featureType", kind: "feature", label: "Feature" },
  { key: "taskType", kind: "task", label: "Task" },
  { key: "defectType", kind: "defect", label: "Defect" },
];

type TypeRead = { kind: "loading" } | { kind: "loaded"; names: string[] } | { kind: "unavailable" };

const sameType = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

export function FlowSyncSettingsGroup() {
  const [settings, setSettings] = useState<FlowTrackerSettings | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [types, setTypes] = useState<TypeRead>({ kind: "loading" });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  /** SET-FR-UFXD: the stored values, and the Type list of SET-FR-XFNA. */
  const load = useCallback(async () => {
    try {
      setSettings(await api.getFlowTrackerSettings());
      setReadError(null);
    } catch (e) {
      logWarn(["frontend"], "flow tracker settings could not be read", { code: refusalCode(e) });
      setReadError(githubPollingErrorMessage(e));
    }
    try {
      const list: MetadataList<PublicationIssueType> | undefined = await api.listGithubIssueTypes();
      setTypes(
        !list || list.state === "failed"
          ? { kind: "unavailable" }
          : { kind: "loaded", names: list.items.map((t) => t.name) },
      );
    } catch (e) {
      logWarn(["frontend", "remote"], "github issue types could not be listed", { code: refusalCode(e) });
      setTypes({ kind: "unavailable" });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  /** SET-FR-PQWW: persist at once; a refused write keeps the previous value. */
  const save = async (next: FlowTrackerSettings) => {
    setSaving(true);
    setSaveError(null);
    try {
      setSettings(await api.setFlowTrackerSettings(next));
    } catch (e) {
      logWarn(["frontend"], "flow tracker settings were refused", { code: refusalCode(e) });
      setSaveError(githubPollingErrorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const offered = types.kind === "loaded" ? types.names : [];
  // SET-FR-LJYT, per SET-FR-UFXD: unavailable only when the list was read and
  // does not hold the stored Type.
  const unavailable = (name: string) =>
    types.kind === "loaded" && !types.names.some((n) => sameType(n, name));

  return (
    <div className="card" style={{ padding: "14px 16px", marginBottom: 16 }} data-testid="settings-flow-sync">
      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
        Syncing delivery plans
      </div>
      <div className="t-ui-sm t-muted" style={{ marginBottom: 12 }}>
        A synced Flow creates each item as a GitHub issue with the Type below.
        Milestones become GitHub milestones.
      </div>
      {settings === null && !readError && <div className="t-muted t-ui-sm">Loading…</div>}
      {readError && (
        <span className="picker-error" role="alert" style={{ display: "block" }}>
          ✗ {readError}
        </span>
      )}
      {settings && (
        <div style={{ display: "grid", gridTemplateColumns: "max-content 1fr", gap: "8px 12px", alignItems: "center" }}>
          {KINDS.map(({ key, kind, label }) => {
            const stored = settings[key];
            const choices = [stored, ...offered.filter((n) => !sameType(n, stored))];
            const id = `flow-sync-${kind}-type`;
            return (
              <div key={key} style={{ display: "contents" }}>
                <span className="t-ui-sm">
                  <span aria-hidden="true">{KIND_GLYPHS[kind]}</span>{" "}
                  <label htmlFor={id}>{label}</label>
                </span>
                <select
                  id={id}
                  className="input input--sm"
                  value={stored}
                  disabled={saving}
                  onChange={(e) => void save({ ...settings, [key]: e.target.value })}
                >
                  {choices.map((name) => (
                    <option key={name} value={name}>
                      {unavailable(name) ? `${name} (unavailable)` : name}
                    </option>
                  ))}
                </select>
              </div>
            );
          })}
        </div>
      )}
      {saveError && (
        <span className="picker-error" role="alert" data-testid="flow-sync-save-error" style={{ display: "block", marginTop: 10 }}>
          ✗ {saveError}
        </span>
      )}
    </div>
  );
}
