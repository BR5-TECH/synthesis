/**
 * The Delivery section of Project settings (`SET-project-settings.md`
 * SET-FR-WDYJ, SET-FR-XUKD, SET-FR-HFKK, SET-FR-RUPL, served by
 * `../../specifications/core/DSR-delivery-source.md`).
 *
 * It holds the source choice of the project's delivery and the Type mapping
 * between GitHub issue Types and plan kinds. Each change persists at once,
 * outside the section dirty state (SET-FR-08), so the section holds nothing to
 * save. A refused write keeps the previous value and shows the error beside the
 * control that made the change.
 */
import { useCallback, useEffect, useState } from "react";
import { listenProject } from "../projectEvents";

import * as api from "../api";
import { DELIVERY_SOURCE_CHANGED } from "../events";
import { logInfo, logWarn } from "../logging";
import { deliveryErrorCode, deliveryErrorMessage } from "../state/deliveryErrors";
import { githubPollingErrorMessage, refusalCode } from "../state/githubPolling";
import type {
  DeliveryReadEntry,
  DeliverySettings,
  DeliverySource,
  IssueKind,
  MetadataList,
  PublicationIssueType,
  TreeNode,
} from "../types";

type CreateKey = keyof DeliverySettings["createTypes"];

/** SET-FR-HFKK: the four issue kinds, in hierarchy order. */
const KINDS: { kind: IssueKind & CreateKey; label: string }[] = [
  { kind: "epic", label: "Epic" },
  { kind: "feature", label: "Feature" },
  { kind: "task", label: "Task" },
  { kind: "defect", label: "Defect" },
];

/** SET-FR-RUPL: the option value of Leave out, and of Not set. */
const EXCLUDE = "exclude";
const NOT_SET = "";

/** SET-FR-XFNA: what the Type list is doing. */
type TypeRead =
  | { kind: "loading" }
  | { kind: "loaded"; names: string[] }
  | { kind: "unavailable"; error: string };

/** The control a write came from, so its error shows beside it (SET-FR-XUKD). */
type Control = "source" | "create" | "read" | "untyped";

/** A Flow file the picker offers. */
interface FlowFile {
  id: string;
  path: string;
}

/** SET-FR-WDYJ: the project's `flow`-typed files, in tree order. */
export function flowFiles(tree: TreeNode | null | undefined): FlowFile[] {
  if (!tree) return [];
  const out: FlowFile[] = [];
  const walk = (node: TreeNode) => {
    if (node.nodeKind === "file" && node.artifactType === "flow") {
      out.push({ id: node.id, path: node.path });
    }
    node.children?.forEach(walk);
  };
  walk(tree);
  return out;
}

/** SET-FR-WDYJ: the resolved source, in words. */
export function describeSource(source: DeliverySource): string {
  switch (source.kind) {
    case "github":
      return `The delivery is the GitHub Project “${source.project.title}” (${source.project.ownerLogin} #${source.project.number}).`;
    case "file":
      return `The delivery is the Flow file ${source.path}.`;
    case "none":
      switch (source.reason) {
        case "github_project_unset":
          return "The project has no delivery: no GitHub Project is selected. Select one in GitHub Project settings.";
        case "delivery_file_missing":
          return "The project has no delivery: the chosen Flow file is missing, or it is not a Flow file.";
        default:
          return "The project has no delivery. Select a GitHub Project in GitHub Project settings, or choose a Flow file.";
      }
  }
}

/** DSR-FR-MKIG: Type names compare exactly. */
const holds = (names: string[], name: string) => names.includes(name);

/**
 * SET-FR-RUPL: the Type that Add gives a new read entry: the first offered Type
 * that is not a create Type and not in a read entry. Null when there is none.
 */
export function nextReadType(settings: DeliverySettings, offered: string[]): string | null {
  const used = [
    ...Object.values(settings.createTypes),
    ...settings.readEntries.map((e) => e.type),
  ];
  return offered.find((name) => !holds(used, name)) ?? null;
}

export function DeliverySettingsSection() {
  const [settings, setSettings] = useState<DeliverySettings | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [source, setSource] = useState<DeliverySource | null>(null);
  const [types, setTypes] = useState<TypeRead>({ kind: "loading" });
  const [files, setFiles] = useState<FlowFile[]>([]);
  // SET-FR-WDYJ: Flow file chosen while no file is stored shows the picker and
  // writes nothing until a file is picked.
  const [fileMode, setFileMode] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<{ control: Control; message: string } | null>(null);

  /** SET-FR-WDYJ: the resolved source. */
  const loadSource = useCallback(async () => {
    try {
      setSource(await api.getDeliverySource());
    } catch (e) {
      logWarn(["frontend"], "delivery source could not be resolved", { code: deliveryErrorCode(e) });
      setSource(null);
    }
  }, []);

  const loadSettings = useCallback(async () => {
    try {
      setSettings(await api.getDeliverySettings());
      setReadError(null);
    } catch (e) {
      logWarn(["frontend"], "delivery settings could not be read", { code: deliveryErrorCode(e) });
      setReadError(deliveryErrorMessage(e));
    }
  }, []);

  /** SET-FR-XFNA / SET-FR-LJYT: the Types the repository's owner offers. */
  const loadTypes = useCallback(async () => {
    setTypes({ kind: "loading" });
    try {
      const list: MetadataList<PublicationIssueType> | undefined = await api.listGithubIssueTypes();
      if (!list || list.state === "failed") {
        logWarn(["frontend", "remote"], "github issue types could not be read", {
          code: list?.errorCode ?? "no_answer",
        });
        setTypes({ kind: "unavailable", error: list?.error ?? "The issue Types could not be read." });
      } else {
        setTypes({ kind: "loaded", names: list.items.map((t) => t.name) });
      }
    } catch (e) {
      logWarn(["frontend", "remote"], "github issue types could not be listed", { code: refusalCode(e) });
      setTypes({ kind: "unavailable", error: githubPollingErrorMessage(e) });
    }
  }, []);

  /** SET-FR-WDYJ: the project's Flow files, for the picker. */
  const loadFiles = useCallback(async () => {
    try {
      setFiles(flowFiles(await api.loadProjectArtifactsTree()));
    } catch (e) {
      // A tree that cannot be read leaves the picker with the stored file only.
      logWarn(["frontend"], "project tree could not be read for the delivery file picker", {
        code: deliveryErrorCode(e),
      });
    }
  }, []);

  useEffect(() => {
    void loadSettings();
    void loadSource();
    void loadTypes();
    void loadFiles();
  }, [loadSettings, loadSource, loadTypes, loadFiles]);

  // DSR-FR-EGFF: a change made elsewhere (a publish, an export, the Map tab)
  // moves the statement of the resolved source and the stored values.
  useEffect(() => {
    const pending = listenProject(DELIVERY_SOURCE_CHANGED, () => {
      void loadSettings();
      void loadSource();
    });
    // The cleanup waits for a listen that has not answered yet, so the
    // unlisten runs once.
    return () => {
      void pending.then((un) => un()).catch(() => {});
    };
  }, [loadSettings, loadSource]);

  /** SET-FR-XUKD: persist at once; a refused write keeps the previous value. */
  const save = async (next: DeliverySettings, control: Control) => {
    setSaving(true);
    setSaveError(null);
    try {
      const stored = await api.setDeliverySettings(next);
      setSettings(stored);
      if (stored.source.kind !== "file") setFileMode(false);
      logInfo(["frontend"], "delivery settings saved", { control, source: stored.source.kind });
      void loadSource();
    } catch (e) {
      logWarn(["frontend"], "delivery settings were refused", { control, code: deliveryErrorCode(e) });
      setSaveError({ control, message: deliveryErrorMessage(e) });
    } finally {
      setSaving(false);
    }
  };

  const offered = types.kind === "loaded" ? types.names : [];
  // SET-FR-LJYT: a stored Type is unavailable only when the list was read and
  // does not hold it. A list that failed says nothing either way.
  const unavailable = (name: string) => types.kind === "loaded" && !holds(types.names, name);
  const typeChoices = (stored: string) => (holds(offered, stored) ? offered : [stored, ...offered]);
  const typeOption = (name: string) => (
    <option key={name} value={name}>
      {unavailable(name) ? `${name} (unavailable)` : name}
    </option>
  );
  const errorBeside = (control: Control) =>
    saveError?.control === control ? (
      <span className="picker-error t-ui-sm" role="alert" data-testid={`delivery-save-error-${control}`}>
        ✗ {saveError.message}
      </span>
    ) : null;

  const storedKind = settings?.source.kind ?? "automatic";
  const shownKind = fileMode && storedKind !== "file" ? "file" : storedKind;
  const storedFlowId = settings?.source.kind === "file" ? settings.source.flowId : null;
  const fileChoices =
    storedFlowId && !files.some((f) => f.id === storedFlowId)
      ? [{ id: storedFlowId, path: storedFlowId }, ...files]
      : files;

  const chooseSource = (kind: "automatic" | "github" | "file") => {
    if (!settings) return;
    if (kind === "file") {
      setFileMode(true);
      setSaveError(null);
      return;
    }
    setFileMode(false);
    void save({ ...settings, source: { kind } }, "source");
  };

  const setEntry = (index: number, entry: DeliveryReadEntry) => {
    if (!settings) return;
    const readEntries = settings.readEntries.map((e, i) => (i === index ? entry : e));
    void save({ ...settings, readEntries }, "read");
  };
  const addType = settings ? nextReadType(settings, offered) : null;

  return (
    <div data-testid="settings-delivery">
      {settings === null && !readError && <div className="t-muted t-ui-sm">Loading…</div>}
      {readError && (
        <span className="picker-error" role="alert" style={{ display: "block" }}>
          ✗ {readError}
        </span>
      )}

      {settings && (
        <div className="card" style={{ padding: "14px 16px", marginBottom: 16 }} data-testid="delivery-source-group">
          <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }} id="delivery-source-label">
            Source
          </div>
          <div className="t-ui-sm t-muted" style={{ marginBottom: 10 }}>
            Where the project&rsquo;s delivery plan lives. The Map tab shows it, and agents read it.
          </div>
          <div role="radiogroup" aria-labelledby="delivery-source-label" style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {(
              [
                ["automatic", "Automatic", "The selected GitHub Project, when there is one."],
                ["github", "GitHub Project", "The GitHub Project selected in GitHub Project settings."],
                ["file", "Flow file", "One Flow file of this project."],
              ] as const
            ).map(([kind, label, hint]) => (
              <label key={kind} className="t-ui-sm" style={{ display: "flex", gap: 8, alignItems: "baseline" }}>
                <input
                  type="radio"
                  name="delivery-source"
                  value={kind}
                  checked={shownKind === kind}
                  disabled={saving}
                  onChange={() => chooseSource(kind)}
                />
                <span>
                  {label}
                  <span className="t-ui-xs t-muted" style={{ display: "block" }}>
                    {hint}
                  </span>
                </span>
              </label>
            ))}
          </div>
          {shownKind === "file" && (
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 10, marginLeft: 22 }}>
              <label className="t-ui-sm" htmlFor="delivery-flow-file">
                Flow file
              </label>
              <select
                id="delivery-flow-file"
                className="input input--sm"
                value={storedFlowId ?? ""}
                disabled={saving}
                onChange={(e) =>
                  e.target.value !== "" &&
                  void save({ ...settings, source: { kind: "file", flowId: e.target.value } }, "source")
                }
              >
                {storedFlowId === null && (
                  <option value="" disabled>
                    {files.length === 0 ? "No Flow file in this project" : "Choose a Flow file…"}
                  </option>
                )}
                {fileChoices.map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.path}
                  </option>
                ))}
              </select>
            </div>
          )}
          {errorBeside("source")}
          {source && (
            <p className="t-ui-sm" role="status" data-testid="delivery-resolved-source" style={{ marginTop: 10, marginBottom: 0 }}>
              {describeSource(source)}
            </p>
          )}
        </div>
      )}

      {settings && (
        <div className="card" style={{ padding: "14px 16px", marginBottom: 16 }} data-testid="delivery-type-mapping">
          <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>Type mapping</div>
          <div className="t-ui-sm t-muted" style={{ marginBottom: 12 }}>
            Which GitHub issue Type each plan kind is created with, and which kind an issue of another
            Type takes when the plan is read.
          </div>

          {types.kind === "loading" && (
            <p className="t-ui-xs t-muted" role="status" data-testid="delivery-types-loading">
              Loading issue Types…
            </p>
          )}
          {types.kind === "unavailable" && (
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }} data-testid="delivery-types-error">
              <span className="picker-error t-ui-sm" role="alert">
                ✗ {types.error} Your saved choices are kept.
              </span>
              <button type="button" className="btn btn--ghost btn--sm" onClick={() => void loadTypes()}>
                Retry
              </button>
            </div>
          )}
          {types.kind === "loaded" && types.names.length === 0 && (
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }} data-testid="delivery-types-empty">
              <span className="t-ui-sm t-muted" role="status">
                GitHub lists no issue Type for this repository&rsquo;s owner. Your saved choices are kept.
              </span>
              <button type="button" className="btn btn--ghost btn--sm" onClick={() => void loadTypes()}>
                Retry
              </button>
            </div>
          )}

          <div className="t-ui-xs t-muted" style={{ fontWeight: 600, marginBottom: 6 }}>
            Create Types
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "max-content 1fr", gap: "8px 12px", alignItems: "center", marginBottom: 6 }}>
            {KINDS.map(({ kind, label }) => {
              const stored = settings.createTypes[kind];
              const id = `delivery-create-${kind}`;
              return (
                <div key={kind} style={{ display: "contents" }}>
                  <label className="t-ui-sm" htmlFor={id}>
                    {label}
                  </label>
                  <select
                    id={id}
                    className="input input--sm"
                    value={stored}
                    disabled={saving}
                    onChange={(e) =>
                      void save({ ...settings, createTypes: { ...settings.createTypes, [kind]: e.target.value } }, "create")
                    }
                  >
                    {typeChoices(stored).map(typeOption)}
                  </select>
                </div>
              );
            })}
          </div>
          {errorBeside("create")}

          <div className="t-ui-xs t-muted" style={{ fontWeight: 600, margin: "14px 0 6px" }}>
            Read entries
          </div>
          {settings.readEntries.length === 0 && (
            <p className="t-ui-sm t-muted" style={{ margin: "0 0 8px" }}>
              No other Type is read into the plan.
            </p>
          )}
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }} data-testid="delivery-read-entries">
            {settings.readEntries.map((entry, index) => (
              <div key={`${entry.type}-${index}`} style={{ display: "flex", alignItems: "center", gap: 8 }} data-testid="delivery-read-entry">
                <select
                  aria-label={`Type of read entry ${index + 1}`}
                  className="input input--sm"
                  value={entry.type}
                  disabled={saving}
                  onChange={(e) => setEntry(index, { ...entry, type: e.target.value })}
                >
                  {typeChoices(entry.type).map(typeOption)}
                </select>
                <span className="t-ui-sm t-muted" aria-hidden="true">
                  →
                </span>
                <select
                  aria-label={`Kind of read entry ${index + 1}`}
                  className="input input--sm"
                  value={entry.kind}
                  disabled={saving}
                  onChange={(e) => setEntry(index, { ...entry, kind: e.target.value as DeliveryReadEntry["kind"] })}
                >
                  {KINDS.map(({ kind, label }) => (
                    <option key={kind} value={kind}>
                      {label}
                    </option>
                  ))}
                  <option value={EXCLUDE}>Leave out</option>
                </select>
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  aria-label={`Remove read entry ${index + 1}`}
                  disabled={saving}
                  onClick={() =>
                    void save({ ...settings, readEntries: settings.readEntries.filter((_, i) => i !== index) }, "read")
                  }
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            style={{ marginTop: 6 }}
            disabled={saving || addType === null}
            title={
              addType !== null
                ? undefined
                : types.kind === "loaded" && types.names.length > 0
                  ? "Every Type GitHub lists is mapped already."
                  : "Add needs the list of issue Types from GitHub."
            }
            onClick={() =>
              addType !== null &&
              void save({ ...settings, readEntries: [...settings.readEntries, { type: addType, kind: "task" }] }, "read")
            }
          >
            Add
          </button>
          {errorBeside("read")}

          <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 14 }}>
            <label className="t-ui-sm" htmlFor="delivery-untyped">
              Issues with no Type
            </label>
            <select
              id="delivery-untyped"
              className="input input--sm"
              value={settings.untyped ?? NOT_SET}
              disabled={saving}
              onChange={(e) =>
                void save(
                  {
                    ...settings,
                    untyped: e.target.value === NOT_SET ? null : (e.target.value as DeliverySettings["untyped"]),
                  },
                  "untyped",
                )
              }
            >
              <option value={NOT_SET}>Not set</option>
              {KINDS.map(({ kind, label }) => (
                <option key={kind} value={kind}>
                  {label}
                </option>
              ))}
              <option value={EXCLUDE}>Leave out</option>
            </select>
          </div>
          {errorBeside("untyped")}
        </div>
      )}
    </div>
  );
}
