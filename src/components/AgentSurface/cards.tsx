/**
 * The four cards of the discussion catalog (`ASF-agent-surfaces.md`
 * ASF-FR-EKZW, ASF-FR-IBTY, ASF-FR-EOLY, ASF-FR-MXGW, ASF-FR-MLKG).
 *
 * A card is a whole panel in one component. It keeps its own selection, and
 * its buttons send the context its kind declares (`AUI-agent-ui-surfaces.md`
 * AUI-FR-XCXV). A submitted card shows what was sent.
 */
import { useId, useMemo, type ReactNode } from "react";

import { Icon } from "../icons";
import { useRender } from "./context";
import type { Scope } from "./functions";
import { useCardState } from "./model";
import type { SurfaceComponent } from "../../types";

type Props = { component: SurfaceComponent; scope: Scope };

interface Option {
  value: string;
  label: string;
  description?: string;
}

const NONE: readonly string[] = [];

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function optionsOf(list: unknown): Option[] {
  return (Array.isArray(list) ? list : []).map((o) => {
    const option = (o ?? {}) as Record<string, unknown>;
    return {
      value: str(option.value),
      label: str(option.label),
      description: typeof option.description === "string" ? option.description : undefined,
    };
  });
}

function nameOf(action: unknown): string {
  return str((action as { name?: unknown } | undefined)?.name);
}

/** ASF-FR-YCTE / ASF-FR-MLKG: what this card sent, where it is the one that sent. */
function useSent(component: SurfaceComponent) {
  const { submission } = useRender();
  return submission && submission.sourceComponentId === component.id ? submission : undefined;
}

function SendButton({
  label,
  disabled,
  pressed,
  onPress,
  danger,
  primary = true,
}: {
  label: string;
  disabled: boolean;
  pressed: boolean;
  onPress: () => void;
  danger?: boolean;
  primary?: boolean;
}) {
  const variant = danger ? "btn--danger" : primary ? "btn--primary" : "";
  return (
    <button
      type="button"
      className={`btn btn--sm ${variant}`.trim()}
      disabled={disabled}
      data-pressed={pressed ? "true" : undefined}
      onClick={onPress}
    >
      {pressed && <Icon.Check size={12} />}
      <span>{label}</span>
    </button>
  );
}

/** ASF-FR-EKZW: a question with options, and an own answer where allowed. */
function ChoiceCardView({ component, scope }: Props) {
  const { surfaceId, readOnly, press } = useRender();
  const sent = useSent(component);
  const name = useId();
  const options = optionsOf(component.options);
  const multiple = component.multiple === true;
  const allowOther = component.allowOther === true;
  const [selected, setSelected] = useCardState<readonly string[]>(surfaceId, `${component.id}:selected`, NONE);
  const [other, setOther] = useCardState(surfaceId, `${component.id}:other`, "");
  const shownSelected = sent ? ((sent.context.selected as string[] | undefined) ?? NONE) : selected;
  const shownOther = sent ? str(sent.context.other) : other;
  const toggle = (value: string) =>
    setSelected(
      multiple
        ? selected.includes(value)
          ? selected.filter((v) => v !== value)
          : [...selected, value]
        : [value],
    );
  const ready = selected.length > 0 || (allowOther && other.trim() !== "");
  const send = () => {
    const context: Record<string, unknown> = { selected: [...selected] };
    if (allowOther) context.other = other;
    press(component.id, scope, { name: nameOf(component.action), context });
  };
  return (
    <div className="agent-surface__card-kind agent-surface__choice-card">
      <fieldset className="agent-surface__field">
        <legend className="agent-surface__question">{str(component.question)}</legend>
        <div className="agent-surface__options">
          {options.map((option, index) => (
            <label key={index} className="agent-surface__choice agent-surface__choice--rich">
              <input
                type={multiple ? "checkbox" : "radio"}
                name={name}
                checked={shownSelected.includes(option.value)}
                disabled={readOnly}
                onChange={() => toggle(option.value)}
              />
              <span className="agent-surface__choice-text">
                <span>{option.label}</span>
                {option.description && (
                  <span className="agent-surface__description">{option.description}</span>
                )}
              </span>
            </label>
          ))}
        </div>
        {allowOther && (
          <label className="agent-surface__field">
            <span className="agent-surface__label">Own answer</span>
            <input
              className="input input--sm"
              type="text"
              value={shownOther}
              disabled={readOnly}
              onChange={(e) => setOther(e.target.value)}
            />
          </label>
        )}
      </fieldset>
      <SendButton
        label={str(component.sendLabel) || "Send"}
        disabled={readOnly || !ready}
        pressed={sent !== undefined}
        onPress={send}
      />
    </div>
  );
}

/** ASF-FR-IBTY: a decision with a confirm button and an optional decline. */
function ConfirmCardView({ component, scope }: Props) {
  const { readOnly, press } = useRender();
  const sent = useSent(component);
  const confirm = (component.confirm ?? {}) as { label?: unknown; name?: unknown };
  const decline = component.decline as { label?: unknown; name?: unknown } | undefined;
  const danger = component.tone === "danger";
  return (
    <div className="agent-surface__card-kind agent-surface__confirm-card">
      <p className="agent-surface__question">{str(component.title)}</p>
      {typeof component.body === "string" && <p className="agent-surface__text">{component.body}</p>}
      <div className="agent-surface__stack agent-surface__stack--row">
        <SendButton
          label={str(confirm.label)}
          disabled={readOnly}
          pressed={sent?.name === str(confirm.name)}
          danger={danger}
          onPress={() => press(component.id, scope, { name: str(confirm.name), context: {} })}
        />
        {decline && (
          <SendButton
            label={str(decline.label)}
            disabled={readOnly}
            pressed={sent?.name === str(decline.name)}
            primary={false}
            onPress={() => press(component.id, scope, { name: str(decline.name), context: {} })}
          />
        )}
      </div>
    </div>
  );
}

/** ASF-FR-EOLY: options against criteria, and a pick where the card has one. */
function ComparisonCardView({ component, scope }: Props) {
  const { surfaceId, readOnly, press } = useRender();
  const sent = useSent(component);
  const name = useId();
  const criteria = (Array.isArray(component.criteria) ? component.criteria : []) as Array<{
    key?: unknown;
    title?: unknown;
  }>;
  const options = (Array.isArray(component.options) ? component.options : []) as Array<{
    value?: unknown;
    label?: unknown;
    values?: Record<string, unknown>;
    recommended?: unknown;
  }>;
  const pick = component.pick as { name?: unknown; label?: unknown } | undefined;
  const [picked, setPicked] = useCardState(surfaceId, `${component.id}:picked`, "");
  const shownPicked = sent ? str(sent.context.picked) : picked;
  const cell = (value: unknown) =>
    typeof value === "string" || typeof value === "number" ? String(value) : typeof value === "boolean" ? (value ? "Yes" : "No") : "";
  return (
    <div className="agent-surface__card-kind agent-surface__comparison-card">
      {typeof component.title === "string" && <p className="agent-surface__question">{component.title}</p>}
      <div className="agent-surface__table-wrap">
        <table className="agent-surface__table">
          <thead>
            <tr>
              {pick && (
                <th scope="col">
                  <span className="agent-surface__visually-hidden">Pick</span>
                </th>
              )}
              <th scope="col">Option</th>
              {criteria.map((c, i) => (
                <th key={i} scope="col">
                  {str(c.title)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {options.map((option, r) => {
              const value = str(option.value);
              return (
                <tr key={r} data-recommended={option.recommended === true ? "true" : undefined}>
                  {pick && (
                    <td>
                      <input
                        type="radio"
                        name={name}
                        aria-label={str(option.label)}
                        checked={shownPicked === value}
                        disabled={readOnly}
                        onChange={() => setPicked(value)}
                      />
                    </td>
                  )}
                  <th scope="row">
                    {str(option.label)}
                    {option.recommended === true && (
                      <span className="agent-surface__badge">Recommended</span>
                    )}
                  </th>
                  {criteria.map((c, i) => (
                    <td key={i}>{cell(option.values?.[str(c.key)])}</td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {pick && (
        <SendButton
          label={str(pick.label) || "Pick"}
          disabled={readOnly || picked === ""}
          pressed={sent !== undefined}
          onPress={() => press(component.id, scope, { name: str(pick.name), context: { picked } })}
        />
      )}
    </div>
  );
}

/** ASF-FR-MXGW: items with checkboxes, sent with none checked as well. */
function ChecklistCardView({ component, scope }: Props) {
  const { surfaceId, readOnly, press } = useRender();
  const sent = useSent(component);
  const items = useMemo(
    () =>
      (Array.isArray(component.items) ? component.items : []).map((i) => {
        const item = (i ?? {}) as Record<string, unknown>;
        return {
          value: str(item.value),
          label: str(item.label),
          description: typeof item.description === "string" ? item.description : undefined,
          checked: item.checked === true,
        };
      }),
    [component.items],
  );
  const initial = useMemo(() => items.filter((i) => i.checked).map((i) => i.value), [items]);
  const [checked, setChecked] = useCardState<readonly string[]>(surfaceId, `${component.id}:checked`, initial);
  const shown = sent ? ((sent.context.checked as string[] | undefined) ?? NONE) : checked;
  const toggle = (value: string) =>
    setChecked(checked.includes(value) ? checked.filter((v) => v !== value) : [...checked, value]);
  return (
    <div className="agent-surface__card-kind agent-surface__checklist-card">
      {typeof component.title === "string" && <p className="agent-surface__question">{component.title}</p>}
      <div className="agent-surface__options">
        {items.map((item, index) => (
          <label key={index} className="agent-surface__choice agent-surface__choice--rich">
            <input
              type="checkbox"
              checked={shown.includes(item.value)}
              disabled={readOnly}
              onChange={() => toggle(item.value)}
            />
            <span className="agent-surface__choice-text">
              <span>{item.label}</span>
              {item.description && <span className="agent-surface__description">{item.description}</span>}
            </span>
          </label>
        ))}
      </div>
      <SendButton
        label={str(component.sendLabel) || "Send"}
        disabled={readOnly}
        pressed={sent !== undefined}
        onPress={() =>
          press(component.id, scope, {
            name: nameOf(component.action),
            context: { checked: items.map((i) => i.value).filter((v) => checked.includes(v)) },
          })
        }
      />
    </div>
  );
}

/** ASF-FR-HRQL: the drawing of each card. */
export const CARD_RENDERERS: Record<string, (props: Props) => ReactNode> = {
  ChoiceCard: ChoiceCardView,
  ConfirmCard: ConfirmCardView,
  ComparisonCard: ComparisonCardView,
  ChecklistCard: ChecklistCardView,
};
