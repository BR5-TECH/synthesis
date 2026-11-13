/**
 * The add and edit dialog of an MCP server (`MSS-mcp-servers-settings.md`
 * MSS-FR-MWAV through MSS-FR-GHDO).
 *
 * On add, the author chooses who uses the server (MSS-FR-SCDK). A team record
 * has its default `enabled` as a check box (MSS-FR-ENMX).
 *
 * The dialog holds the unsaved edits. `Cancel` and Escape drop them
 * (MSS-FR-GMHC). `Save` writes the record, then each typed secret, then each
 * cleared secret (MSS-FR-UWZQ). A refusal keeps the dialog open with every
 * field as the author typed it (MSS-FR-STXZ).
 */
import { useEffect, useMemo, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import { AUTH_LABEL, errorCode, logCode, refusalText, SCOPE_LABEL } from "../state/mcpServerText";
import {
  buildRecord,
  emptyForm,
  formFromRecord,
  refusalField,
  secretSteps,
  slugify,
  validateForm,
  type McpFormProblems,
  type McpListRow,
  type McpServerForm,
} from "../state/mcpValidation";
import type { McpAuthMode, McpScope, McpSecretPresence, McpServerEntry } from "../types";
import { Modal } from "./DraftsPanelParts";
import { Field, KeyValueList, ProblemLine, SecretField } from "./McpDialogFields";

export interface McpServerDialogProps {
  /** The row of `Edit` or `Set secrets`. Null for `Add server`. */
  entry: McpServerEntry | null;
  /** The IDs of every server of the project (MCS-FR-LLQD). */
  knownIds: string[];
  /** MSS-FR-PDVK: put the focus on the first secret that has no value. */
  focusMissingSecret?: boolean;
  returnFocus?: HTMLElement | null;
  onClose: () => void;
  /** The entry of the server after the last step of a save. */
  onSaved: (entry: McpServerEntry) => void;
}

const SCOPE_CHOICES: McpScope[] = ["team", "personal_project", "personal_all"];

const AUTH_CHOICES: [McpAuthMode, string][] = (["none", "bearer", "oauth2"] as const).map(
  (mode) => [mode, AUTH_LABEL[mode]],
);

function initialForm(entry: McpServerEntry | null): McpServerForm {
  if (!entry) return emptyForm();
  if (entry.record) return formFromRecord(entry.record);
  // MCS-FR-HBOD: an invalid record comes with no fields. A valid record saved
  // over it with the same ID replaces it.
  return { ...emptyForm(), id: entry.id, idEdited: true };
}

export function McpServerDialog({
  entry,
  knownIds,
  focusMissingSecret = false,
  returnFocus,
  onClose,
  onSaved,
}: McpServerDialogProps) {
  const [form, setForm] = useState<McpServerForm>(() => initialForm(entry));
  /** The ID that a save of this dialog stored. From then on, the ID is fixed. */
  const [savedId, setSavedId] = useState<string | null>(entry ? entry.id : null);
  const [presence, setPresence] = useState<McpSecretPresence[]>(entry?.secrets ?? []);
  /** MSS-FR-SCDK: the team first on add; the scope of the record on edit. */
  const [scope, setScope] = useState<McpScope>(entry?.scope ?? "team");
  /** MSS-FR-ENMX: the default of a team record, on for a new one. */
  const [teamDefault, setTeamDefault] = useState<boolean>(entry?.record?.enabled ?? true);
  const [touched, setTouched] = useState<Set<string>>(() => new Set());
  const [refusals, setRefusals] = useState<McpFormProblems>({});
  const [topError, setTopError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const alive = useRef(true);

  const isNew = savedId === null;
  const otherIds = useMemo(
    () => knownIds.filter((id) => id !== savedId),
    [knownIds, savedId],
  );
  const problems = useMemo(
    () => validateForm(form, { otherIds, isNew }),
    [form, otherIds, isNew],
  );
  const hasProblem = Object.keys(problems).length > 0;

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // MSS-FR-PDVK: this effect runs after the modal put the focus on its first
  // control, so it wins.
  useEffect(() => {
    if (!focusMissingSecret) return;
    const missing = (entry?.secrets ?? []).find((s) => !s.present);
    if (!missing) return;
    const inputs = body.current?.querySelectorAll<HTMLInputElement>("[data-secret-slot]") ?? [];
    Array.from(inputs)
      .find((input) => input.dataset.secretSlot === missing.slot)
      ?.focus();
    // Mount-only: the request is for the moment the dialog opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const touch = (field: string) =>
    setTouched((prev) => (prev.has(field) ? prev : new Set(prev).add(field)));

  /** A change of a field drops the refusal that names it. */
  const change = (patch: Partial<McpServerForm>, changed: string[]) => {
    let fields = changed;
    setForm((prev) => {
      const next = { ...prev, ...patch };
      // MSS-FR-DVDX: on add, ID follows Name until the author edits ID.
      if ("name" in patch && isNew && !next.idEdited) next.id = slugify(next.name);
      return next;
    });
    // MSS-FR-BEQD: an ID that follows the name shows its problem with the name,
    // so a Save that an ID disables always says why.
    if (fields.includes("name") && isNew && !form.idEdited) fields = [...fields, "id"];
    fields.forEach(touch);
    setRefusals((prev) => {
      const next = { ...prev };
      for (const field of fields) delete next[field];
      return next;
    });
  };

  /** A problem shows once the author worked on the field. A refusal always shows. */
  const shown = (field: string): string | null =>
    refusals[field] ?? (touched.has(field) ? (problems[field] ?? null) : null);

  const listProblems = (prefix: "env" | "header"): Record<string, string> => {
    const out: Record<string, string> = {};
    for (const [field, text] of Object.entries({ ...problems, ...refusals })) {
      if (!field.startsWith(`${prefix}:`)) continue;
      if (refusals[field] || touched.has(field)) out[field] = refusals[field] ?? text;
    }
    return out;
  };

  const present = (slot: string) => presence.some((p) => p.slot === slot && p.present);

  /** MSS-FR-UWZQ: the record, then each typed secret, then each cleared secret. */
  const save = async () => {
    if (hasProblem || saving) return;
    setSaving(true);
    setRefusals({});
    setTopError(null);
    const built = buildRecord(form, entry?.record ?? null);
    // MSS-FR-ENMX: a personal record keeps its `enabled`, and a new one is on.
    const record = scope === "team" ? { ...built, enabled: teamDefault } : built;
    const action = isNew ? "add" : "edit";
    logInfo(["frontend"], "mcp server save started", {
      id: record.id,
      action,
      scope,
      transport: record.transport.kind,
    });
    let latest: McpServerEntry;
    try {
      latest = await api.saveMcpServer(record, scope);
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "mcp server save refused", { id: record.id, action, error: logCode(code) });
      if (!alive.current) return;
      // MSS-FR-STXZ: every field stays as the author typed it.
      const field = refusalField(code, form);
      if (field) setRefusals({ [field]: refusalText(code) });
      else setTopError(refusalText(code));
      setSaving(false);
      return;
    }
    if (alive.current) {
      setSavedId(record.id);
      setPresence(latest.secrets);
    }

    const failed: McpFormProblems = {};
    const done: string[] = [];
    for (const step of secretSteps(form, latest.secrets)) {
      logInfo(["frontend"], `mcp server secret ${step.kind} started`, { id: record.id, slot: step.slot });
      try {
        latest =
          step.kind === "set"
            ? await api.setMcpServerSecret(record.id, step.slot, step.value)
            : await api.clearMcpServerSecret(record.id, step.slot);
        // A step that is done is not done again on the next Save.
        done.push(step.field);
      } catch (e) {
        const code = errorCode(e);
        logWarn(["frontend"], `mcp server secret ${step.kind} refused`, {
          id: record.id,
          slot: step.slot,
          error: logCode(code),
        });
        failed[step.field] =
          step.kind === "set"
            ? `This secret was not stored: ${refusalText(code)}`
            : `This secret was not cleared: ${refusalText(code)}`;
      }
    }
    // The section gets the entry also when the dialog closed during the save.
    onSaved(latest);
    if (Object.keys(failed).length > 0) {
      if (!alive.current) return;
      // MSS-FR-STXZ: the record is saved, and the dialog says which secret is not.
      setPresence(latest.secrets);
      setForm((prev) => done.reduce(withSecretDone, prev));
      setRefusals(failed);
      setTopError("The server is saved. A secret was not stored. Correct it and save again.");
      setSaving(false);
      return;
    }
    logInfo(["frontend"], "mcp server saved", { id: record.id, action });
    if (alive.current) onClose();
  };

  const title = entry ? "Edit MCP server" : "Add MCP server";
  const isStdio = form.transport === "stdio";

  return (
    <Modal label={title} onClose={onClose} returnFocus={returnFocus} closeOnScrim={false} className="modal--wide mcp-dialog">
      <div className="modal__head">
        <div className="modal__title">{title}</div>
      </div>
      <div className="modal__body" ref={body} data-testid="mcp-server-dialog">
        {topError && (
          <p className="mcp-dialog__problem mcp-dialog__top" role="alert">
            {topError}
          </p>
        )}

        <Field label="Name" problem={shown("name")}>
          {({ id, describedBy, invalid }) => (
            <input
              id={id}
              className="input"
              value={form.name}
              aria-invalid={invalid}
              aria-describedby={describedBy}
              onChange={(event) => change({ name: event.target.value }, ["name"])}
              onBlur={() => touch("name")}
            />
          )}
        </Field>

        <Field
          label="ID"
          hint={isNew ? "Agents see it in the names of the tools. You cannot change it later." : undefined}
          problem={shown("id")}
        >
          {({ id, describedBy, invalid }) => (
            <input
              id={id}
              className="input input--mono"
              value={form.id}
              readOnly={!isNew}
              aria-readonly={!isNew}
              aria-invalid={invalid}
              aria-describedby={describedBy}
              onChange={(event) => change({ id: event.target.value, idEdited: true }, ["id"])}
              onBlur={() => touch("id")}
            />
          )}
        </Field>

        {isNew ? (
          <fieldset className="mcp-dialog__choice">
            <legend className="mcp-dialog__label">Who uses it</legend>
            <div className="mcp-dialog__radios mcp-dialog__radios--column">
              {SCOPE_CHOICES.map((value) => (
                <label key={value} className="mcp-dialog__radio">
                  <input
                    type="radio"
                    name="mcp-scope"
                    value={value}
                    checked={scope === value}
                    onChange={() => setScope(value)}
                  />
                  {SCOPE_LABEL[value]}
                </label>
              ))}
            </div>
          </fieldset>
        ) : (
          <div className="mcp-dialog__field">
            <span className="mcp-dialog__label">Who uses it</span>
            <span className="t-ui-sm mcp-dialog__value" data-testid="mcp-dialog-scope">
              {SCOPE_LABEL[scope]}
            </span>
          </div>
        )}

        {scope === "team" && (
          <label className="mcp-dialog__check t-ui-sm">
            <input
              type="checkbox"
              checked={teamDefault}
              onChange={(event) => setTeamDefault(event.target.checked)}
            />
            On for the team by default
          </label>
        )}

        <fieldset className="mcp-dialog__choice">
          <legend className="mcp-dialog__label">Transport</legend>
          <div className="mcp-dialog__radios">
            {(
              [
                ["stdio", "Local command"],
                ["http", "Remote (HTTP)"],
              ] as const
            ).map(([value, label]) => (
              <label key={value} className="mcp-dialog__radio">
                <input
                  type="radio"
                  name="mcp-transport"
                  value={value}
                  checked={form.transport === value}
                  onChange={() => change({ transport: value }, [])}
                />
                {label}
              </label>
            ))}
          </div>
        </fieldset>

        {isStdio ? (
          <>
            <Field label="Command" problem={shown("command")}>
              {({ id, describedBy, invalid }) => (
                <input
                  id={id}
                  className="input input--mono"
                  value={form.command}
                  aria-invalid={invalid}
                  aria-describedby={describedBy}
                  onChange={(event) => change({ command: event.target.value }, ["command"])}
                  onBlur={() => touch("command")}
                />
              )}
            </Field>
            <Field label="Arguments" hint="One argument on each line.">
              {({ id }) => (
                <textarea
                  id={id}
                  className="textarea input--mono mcp-dialog__args"
                  rows={3}
                  value={form.argsText}
                  onChange={(event) => change({ argsText: event.target.value }, [])}
                />
              )}
            </Field>
            <Field
              label="Working folder"
              hint="Relative to the project. Empty runs the command in the project root."
              problem={shown("cwd")}
            >
              {({ id, describedBy, invalid }) => (
                <input
                  id={id}
                  className="input input--mono"
                  value={form.cwd}
                  aria-invalid={invalid}
                  aria-describedby={describedBy}
                  onChange={(event) => change({ cwd: event.target.value }, ["cwd"])}
                  onBlur={() => touch("cwd")}
                />
              )}
            </Field>
            <KeyValueList
              title="Environment"
              noun="variable"
              slotPrefix="env"
              rows={form.env}
              problems={listProblems("env")}
              present={present}
              onChange={(env) => change({ env }, changedRows("env", form.env, env))}
              onTouch={touch}
            />
          </>
        ) : (
          <>
            <Field label="URL" problem={shown("url")}>
              {({ id, describedBy, invalid }) => (
                <input
                  id={id}
                  className="input input--mono"
                  type="url"
                  value={form.url}
                  aria-invalid={invalid}
                  aria-describedby={describedBy}
                  onChange={(event) => change({ url: event.target.value }, ["url"])}
                  onBlur={() => touch("url")}
                />
              )}
            </Field>
            <fieldset className="mcp-dialog__choice">
              <legend className="mcp-dialog__label">Authentication</legend>
              <div className="mcp-dialog__radios">
                {AUTH_CHOICES.map(([value, label]) => (
                  <label key={value} className="mcp-dialog__radio">
                    <input
                      type="radio"
                      name="mcp-auth"
                      value={value}
                      checked={form.auth === value}
                      onChange={() => change({ auth: value }, ["auth"])}
                    />
                    {label}
                  </label>
                ))}
              </div>
              <ProblemLine id="mcp-auth-problem" text={refusals.auth} />
            </fieldset>
            {form.auth === "bearer" && (
              <SecretField
                label="Token"
                slot="bearer"
                present={present("bearer")}
                input={form.bearer}
                problem={refusals.bearer ?? problems.bearer}
                onChange={(bearer) => change({ bearer }, ["bearer"])}
              />
            )}
            {form.auth === "oauth2" && (
              <>
                <Field label="Client ID" hint="Optional. Without it, the server registers a client.">
                  {({ id }) => (
                    <input
                      id={id}
                      className="input input--mono"
                      value={form.clientId}
                      onChange={(event) => change({ clientId: event.target.value }, [])}
                    />
                  )}
                </Field>
                {form.clientId.trim() !== "" && (
                  <SecretField
                    label="Client secret"
                    slot="oauth_client_secret"
                    present={present("oauth_client_secret")}
                    input={form.clientSecret}
                    problem={refusals.clientSecret}
                    onChange={(clientSecret) => change({ clientSecret }, ["clientSecret"])}
                  />
                )}
                <Field label="Scopes" hint="Optional. Separate the scopes with spaces.">
                  {({ id }) => (
                    <input
                      id={id}
                      className="input input--mono"
                      value={form.scopesText}
                      onChange={(event) => change({ scopesText: event.target.value }, [])}
                    />
                  )}
                </Field>
                <Field label="Callback port" hint="Optional." problem={shown("callbackPort")}>
                  {({ id, describedBy, invalid }) => (
                    <input
                      id={id}
                      className="input input--mono mcp-dialog__port"
                      inputMode="numeric"
                      value={form.callbackPortText}
                      aria-invalid={invalid}
                      aria-describedby={describedBy}
                      onChange={(event) =>
                        change({ callbackPortText: event.target.value }, ["callbackPort"])
                      }
                      onBlur={() => touch("callbackPort")}
                    />
                  )}
                </Field>
              </>
            )}
            <KeyValueList
              title="Headers"
              noun="header"
              slotPrefix="header"
              rows={form.headers}
              problems={listProblems("header")}
              present={present}
              onChange={(headers) => change({ headers }, changedRows("header", form.headers, headers))}
              onTouch={touch}
            />
          </>
        )}
      </div>
      <div className="modal__actions">
        <button type="button" className="btn btn--ghost" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="btn btn--primary"
          disabled={hasProblem || saving}
          onClick={() => void save()}
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  );
}

/** The list fields whose row the author changed, added, or removed. */
function changedRows(prefix: "env" | "header", before: McpListRow[], after: McpListRow[]): string[] {
  const keys = new Set<string>();
  for (const row of after) if (!before.includes(row)) keys.add(row.key);
  for (const row of before) if (!after.some((r) => r.key === row.key)) keys.add(row.key);
  return [...keys].map((key) => `${prefix}:${key}`);
}

/** The typed value and the clear of one secret field are done. */
function withSecretDone(form: McpServerForm, field: string): McpServerForm {
  const done = { value: "", clear: false };
  if (field === "bearer") return { ...form, bearer: done };
  if (field === "clientSecret") return { ...form, clientSecret: done };
  const [prefix, key] = field.split(":");
  const rows = prefix === "env" ? form.env : form.headers;
  const updated = rows.map((row) => (row.key === key ? { ...row, ...done } : row));
  return prefix === "env" ? { ...form, env: updated } : { ...form, headers: updated };
}
