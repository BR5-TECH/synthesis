/**
 * The Graduation section of Project settings
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-QKKQ through
 * SET-FR-IHQE).
 *
 * One control: the project-wide limit of graduation runs. A choice is a pending
 * change that the window writes with its other pending changes, so the section
 * has no Save control of its own and writes nothing when a choice is made.
 */
import {
  limitChoices,
  limitKey,
  limitOfKey,
  type GraduationConcurrencySetting,
} from "../state/graduationConcurrency";

export function GraduationSettings({
  setting,
}: {
  setting: GraduationConcurrencySetting;
}) {
  const { load, loadError, stored, selected, saving, saveError, pending } = setting;

  // SET-FR-TFNG: a failed read states it and offers no choice, so the section
  // never shows a limit of one that the store does not hold.
  if (load === "error") {
    return (
      <div className="card" style={{ padding: "12px 14px" }} role="alert">
        <div className="t-ui-sm" style={{ color: "var(--danger, #e5484d)" }}>
          The limit of graduation runs could not be read.
        </div>
        <p className="t-ui-xs t-muted" style={{ margin: "6px 0 0" }}>
          {loadError}
        </p>
        <p className="t-ui-xs t-muted" style={{ margin: "6px 0 0" }}>
          Nothing has been written from here.
        </p>
      </div>
    );
  }

  if (load === "loading" || selected === null) {
    return (
      <div className="t-ui-sm t-muted" role="status">
        Loading the limit of graduation runs…
      </div>
    );
  }

  return (
    <div
      className="card"
      style={{ padding: "14px 16px", marginBottom: 16 }}
      data-testid="settings-graduation"
    >
      <label
        htmlFor="graduation-concurrency"
        style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}
      >
        Concurrent graduation runs
      </label>
      <div className="t-ui-sm t-muted" style={{ marginBottom: 10 }}>
        The most runs this project works at the same time. Stream runs and
        direct runs count together. Each stream or worktree still works one
        run at a time.
      </div>
      <select
        id="graduation-concurrency"
        className="select"
        value={limitKey(selected)}
        disabled={saving}
        onChange={(event) => setting.select(limitOfKey(event.target.value))}
      >
        {limitChoices(stored).map((choice) => (
          <option key={limitKey(choice.value)} value={limitKey(choice.value)}>
            {choice.label}
          </option>
        ))}
      </select>
      <p className="t-ui-xs t-muted" style={{ margin: "8px 0 0" }}>
        A lower limit stops no run that works now. It starts no new run until
        fewer runs work than the limit.
      </p>

      {saving && (
        <p className="t-ui-xs t-muted" role="status" style={{ margin: "8px 0 0" }}>
          Saving…
        </p>
      )}
      {!saving && pending && !saveError && (
        <p
          className="t-ui-xs t-muted"
          data-testid="graduation-pending"
          style={{ margin: "8px 0 0" }}
        >
          This change is not saved yet. It is saved with the other changes when
          you close this window.
        </p>
      )}
      {saveError && (
        <div role="alert" style={{ marginTop: 8 }}>
          <span className="picker-error" style={{ display: "block" }}>
            ✗ The limit could not be saved. {saveError}
          </span>
          <button
            type="button"
            className="btn btn--default btn--sm"
            style={{ marginTop: 6 }}
            onClick={() => void setting.save()}
          >
            Retry
          </button>
        </div>
      )}
    </div>
  );
}
