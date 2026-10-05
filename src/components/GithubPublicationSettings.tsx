/**
 * The Publication group of the GitHub Project settings section
 * (`SET-project-settings.md` SET-FR-BLQN … SET-FR-LJYT, served by
 * `../../specifications/core/GHP-github-publication.md` GHP-FR-KVRH).
 *
 * It holds the Types that may be a parent, the Type of every sub-issue, and
 * the sub-issue milestone policy. Each change persists at once, outside the
 * section dirty state (SET-FR-MQUE), so the group holds nothing to save.
 *
 * GitHub is consulted for one thing, the list of Types, and only to say which
 * stored Type is still offered. A list that cannot be read changes no stored
 * value (SET-FR-LJYT): the group states what it could not check and leaves the
 * author's choices alone.
 */
import { useCallback, useEffect, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import { githubPollingErrorMessage, refusalCode } from "../state/githubPolling";
import type {
  GithubPublicationSettings,
  MetadataList,
  MilestonePolicy,
  PublicationIssueType,
} from "../types";

const POLICIES: { value: MilestonePolicy; label: string; hint: string }[] = [
  {
    value: "inherit_parent",
    label: "Inherit parent milestone",
    hint: "The sub-issue takes the milestone of its parent, or none if the parent has none.",
  },
  {
    value: "no_milestone",
    label: "No milestone",
    hint: "The sub-issue is published without a milestone.",
  },
  {
    value: "author_selected",
    label: "Author-selectable milestone",
    hint: "The publication chooser offers the open milestones of the repository.",
  },
];

/** SET-FR-XFNA: what the Type list is doing. */
type TypeRead =
  | { kind: "loading" }
  | { kind: "loaded"; names: string[] }
  | { kind: "unavailable"; error: string };

function sameType(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

export function GithubPublicationSettingsGroup() {
  const [settings, setSettings] = useState<GithubPublicationSettings | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [types, setTypes] = useState<TypeRead>({ kind: "loading" });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const loadSettings = useCallback(async () => {
    try {
      const stored = await api.getGithubPublicationSettings();
      if (!stored) throw new Error("invalid_publication_settings");
      setSettings(stored);
      setReadError(null);
    } catch (e) {
      logWarn(["frontend"], "github publication settings could not be read", {
        code: refusalCode(e),
      });
      setReadError(githubPollingErrorMessage(e));
    }
  }, []);

  /** SET-FR-XFNA / SET-FR-LJYT: the Types the repository's owner offers. */
  const loadTypes = useCallback(async () => {
    setTypes({ kind: "loading" });
    try {
      const list: MetadataList<PublicationIssueType> | undefined =
        await api.listGithubIssueTypes();
      if (!list || list.state === "failed") {
        setTypes({
          kind: "unavailable",
          error: list?.error ?? "The issue Types could not be read.",
        });
      } else {
        setTypes({ kind: "loaded", names: list.items.map((t) => t.name) });
      }
    } catch (e) {
      logWarn(["frontend", "remote"], "github issue types could not be listed", {
        code: refusalCode(e),
      });
      setTypes({ kind: "unavailable", error: githubPollingErrorMessage(e) });
    }
  }, []);

  useEffect(() => {
    void loadSettings();
    void loadTypes();
  }, [loadSettings, loadTypes]);

  /**
   * SET-FR-MQUE: persist at once. The controls render the stored values, so a
   * refused write leaves them showing the previous ones.
   */
  const save = async (next: GithubPublicationSettings) => {
    setSaving(true);
    setSaveError(null);
    try {
      setSettings(await api.setGithubPublicationSettings(next));
    } catch (e) {
      logWarn(["frontend"], "github publication settings were refused", {
        code: refusalCode(e),
      });
      setSaveError(githubPollingErrorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const offered = types.kind === "loaded" ? types.names : [];
  // SET-FR-LJYT: a stored Type is unavailable only when the list was read and
  // does not hold it. A list that failed says nothing either way.
  const unavailable = (name: string) =>
    types.kind === "loaded" && !types.names.some((n) => sameType(n, name));

  const parentChoices = settings
    ? [
        ...settings.parentIssueTypes,
        ...offered.filter(
          (n) => !settings.parentIssueTypes.some((stored) => sameType(stored, n)),
        ),
      ]
    : [];
  const subChoices = settings
    ? [
        ...(offered.some((n) => sameType(n, settings.subIssueType))
          ? []
          : [settings.subIssueType]),
        ...offered,
      ]
    : [];

  const toggleParent = (name: string, checked: boolean) => {
    if (!settings) return;
    const next = checked
      ? [...settings.parentIssueTypes, name]
      : settings.parentIssueTypes.filter((stored) => !sameType(stored, name));
    void save({ ...settings, parentIssueTypes: next });
  };

  return (
    <div
      className="card"
      style={{ padding: "14px 16px", marginBottom: 16 }}
      data-testid="settings-github-publication"
    >
      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
        Publishing drafts as issues
      </div>
      <div className="t-ui-sm t-muted" style={{ marginBottom: 12 }}>
        Publish to GitHub can create a root issue, or a sub-issue of an open
        issue in the same repository. These settings decide which issues can be
        a parent and how a sub-issue is typed and scheduled.
      </div>

      {settings === null && !readError && <div className="t-muted t-ui-sm">Loading…</div>}
      {readError && (
        <span className="picker-error" role="alert" style={{ display: "block" }}>
          ✗ {readError}
        </span>
      )}

      {types.kind === "loading" && settings && (
        <p className="t-ui-xs t-muted" role="status" data-testid="github-types-loading">
          Loading issue Types…
        </p>
      )}
      {types.kind === "unavailable" && (
        <div
          style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}
          data-testid="github-types-error"
        >
          <span className="picker-error t-ui-sm" role="alert">
            ✗ {types.error} Your saved choices are kept.
          </span>
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => void loadTypes()}>
            Retry
          </button>
        </div>
      )}
      {types.kind === "loaded" && types.names.length === 0 && (
        <div
          style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}
          data-testid="github-types-empty"
        >
          <span className="t-ui-sm t-muted" role="status">
            GitHub lists no issue Type for this repository’s owner. Your saved choices are kept.
          </span>
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => void loadTypes()}>
            Retry
          </button>
        </div>
      )}

      {settings && (
        <div style={{ display: "grid", gridTemplateColumns: "180px 1fr", gap: "12px 12px", alignItems: "start" }}>
          <span className="t-ui-sm" id="publication-parent-types-label">
            Parent issue Types
          </span>
          <div
            role="group"
            aria-labelledby="publication-parent-types-label"
            style={{ display: "flex", flexDirection: "column", gap: 4 }}
          >
            {parentChoices.map((name) => {
              const checked = settings.parentIssueTypes.some((stored) => sameType(stored, name));
              const last = checked && settings.parentIssueTypes.length === 1;
              return (
                <label key={name} className="t-ui-sm" style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={saving || last}
                    title={last ? "At least one Type must stay selected." : undefined}
                    onChange={(e) => toggleParent(name, e.target.checked)}
                  />
                  <span>{name}</span>
                  {checked && unavailable(name) && (
                    <span className="t-ui-xs t-muted" data-testid="github-type-unavailable">
                      (unavailable)
                    </span>
                  )}
                </label>
              );
            })}
          </div>

          <label className="t-ui-sm" htmlFor="publication-sub-issue-type">
            Sub-issue Type
          </label>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <select
              id="publication-sub-issue-type"
              className="input input--sm"
              value={settings.subIssueType}
              disabled={saving}
              onChange={(e) => void save({ ...settings, subIssueType: e.target.value })}
            >
              {subChoices.map((name) => (
                <option key={name} value={name}>
                  {unavailable(name) ? `${name} (unavailable)` : name}
                </option>
              ))}
            </select>
          </div>

          <span className="t-ui-sm" id="publication-milestone-policy-label">
            Sub-issue milestone
          </span>
          <div
            role="radiogroup"
            aria-labelledby="publication-milestone-policy-label"
            style={{ display: "flex", flexDirection: "column", gap: 6 }}
          >
            {POLICIES.map((policy) => (
              <label key={policy.value} className="t-ui-sm" style={{ display: "flex", gap: 8, alignItems: "baseline" }}>
                <input
                  type="radio"
                  name="publication-milestone-policy"
                  value={policy.value}
                  checked={settings.subIssueMilestonePolicy === policy.value}
                  disabled={saving}
                  onChange={() => void save({ ...settings, subIssueMilestonePolicy: policy.value })}
                />
                <span>
                  {policy.label}
                  <span className="t-ui-xs t-muted" style={{ display: "block" }}>
                    {policy.hint}
                  </span>
                </span>
              </label>
            ))}
          </div>
        </div>
      )}

      {saveError && (
        <span
          className="picker-error"
          role="alert"
          data-testid="github-publication-save-error"
          style={{ display: "block", marginTop: 10 }}
        >
          ✗ {saveError}
        </span>
      )}
    </div>
  );
}
