/**
 * What a graduation run makes while OpenSpec mode is active
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-DCLQ,
 * GSD-FR-CKSV, GSD-FR-DCBH, GSD-FR-AGPL).
 *
 * The dialog reads `"get openspec status"` when it opens. While the mode is
 * active, it asks **Graduate as**: **Propose change** or **Implement**.
 * **Propose change** asks a change name, checked as the author types.
 */
import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, RefObject } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { CHANGE_NAME_RULE, isValidChangeName } from "../../state/openspec/changeName";

/** GSD-FR-DCLQ: what the run makes. */
export type GraduateAs = "propose" | "implement";

/** GSD-FR-AGPL: why Work directly is not offered for a proposed change. */
export const DIRECT_PROPOSE_REASON =
  "A proposed change lives on a work stream, so a propose run cannot work directly in the worktree.";

/** GSD-FR-DCBH: the refusals of a propose start that render against the field. */
export const CHANGE_NAME_REFUSALS = ["invalid_change_name", "openspec_change_exists"];

/** GSD-FR-DCBH: what a refusal of the change name says. */
export function changeNameRefusalText(code: string): string {
  return code === "openspec_change_exists"
    ? "A change of this name is already on that stream, or a run is proposing it. Choose another name."
    : CHANGE_NAME_RULE;
}

export interface OpenspecModeRead {
  /** The read answered or failed. The dialog starts nothing before that. */
  settled: boolean;
  /** GSD-FR-DCLQ: the mode is active. A status that cannot be read is not. */
  active: boolean;
}

/** GSD-FR-DCLQ: read the OpenSpec status once, when the dialog opens. */
export function useOpenspecMode(draftId: string): OpenspecModeRead {
  const [read, setRead] = useState<OpenspecModeRead>({ settled: false, active: false });
  useEffect(() => {
    let live = true;
    api
      .getOpenspecStatus()
      .then((status) => {
        if (!live) return;
        const active = status?.active === true;
        setRead({ settled: true, active });
        logInfo(["frontend"], "graduation start read the openspec status", { draftId, active });
      })
      .catch((e) => {
        if (!live) return;
        setRead({ settled: true, active: false });
        logWarn(["frontend"], "graduation start could not read the openspec status", {
          draftId,
          error: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      live = false;
    };
  }, [draftId]);
  return read;
}

const CHOICES: Array<{ value: GraduateAs; label: string }> = [
  { value: "propose", label: "Propose change" },
  { value: "implement", label: "Implement" },
];

/** GSD-FR-DCLQ: the **Graduate as** toggle, a radio group with a roving tab stop. */
export function GraduateAsChoice({
  value,
  disabled,
  onChange,
  selectedRef,
}: {
  value: GraduateAs;
  disabled: boolean;
  onChange: (value: GraduateAs) => void;
  /** GSD-FR-WMTD: the selected segment, which the dialog focuses first. */
  selectedRef?: { current: HTMLButtonElement | null };
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const onKeyDown = (event: KeyboardEvent, index: number) => {
    const keys = ["ArrowRight", "ArrowDown", "ArrowLeft", "ArrowUp", "Home", "End"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const next =
      event.key === "Home" ? 0 : event.key === "End" ? CHOICES.length - 1 : (index + 1) % 2;
    onChange(CHOICES[next].value);
    refs.current[next]?.focus();
  };
  return (
    <fieldset className="graduation-start__destination" data-testid="graduation-graduate-as">
      <legend className="picker-field__label">Graduate as</legend>
      <div className="graduation-start__destination-row" role="radiogroup" aria-label="Graduate as">
        {CHOICES.map((choice, index) => {
          const active = value === choice.value;
          return (
            <button
              key={choice.value}
              ref={(element) => {
                refs.current[index] = element;
                if (active && selectedRef) selectedRef.current = element;
              }}
              type="button"
              role="radio"
              value={choice.value}
              className="graduation-start__destination-option"
              aria-checked={active}
              data-active={active ? "true" : "false"}
              tabIndex={active ? 0 : -1}
              disabled={disabled}
              onClick={() => {
                if (!active) onChange(choice.value);
              }}
              onKeyDown={(event) => onKeyDown(event, index)}
            >
              {choice.label}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

/**
 * GSD-FR-CKSV / GSD-FR-DCBH: the **Change name** field. A name that is not
 * valid states the rule under the field. A refusal of the start renders here.
 */
export function ChangeNameField({
  value,
  onChange,
  refusal,
  disabled,
  inputRef,
}: {
  value: string;
  onChange: (value: string) => void;
  refusal: string | null;
  disabled: boolean;
  inputRef?: RefObject<HTMLInputElement | null>;
}) {
  const invalid = !isValidChangeName(value);
  const message = refusal ?? (invalid ? CHANGE_NAME_RULE : null);
  return (
    <div className="picker-field">
      <label className="picker-field__label" htmlFor="graduation-change-name">
        Change name
      </label>
      <input
        id="graduation-change-name"
        className="input"
        ref={inputRef}
        value={value}
        disabled={disabled}
        spellCheck={false}
        autoComplete="off"
        aria-invalid={message !== null || undefined}
        aria-describedby={message ? "graduation-change-name-error" : undefined}
        onChange={(event) => onChange(event.target.value)}
      />
      {message && (
        <p
          className="stream-select__error"
          id="graduation-change-name-error"
          role={refusal ? "alert" : undefined}
          data-testid="graduation-change-name-error"
        >
          {message}
        </p>
      )}
    </div>
  );
}
