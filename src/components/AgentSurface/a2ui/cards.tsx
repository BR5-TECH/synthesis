/**
 * The four cards of the discussion catalog (`ASF-agent-surfaces.md`
 * ASF-FR-EKZW, ASF-FR-IBTY, ASF-FR-EOLY, ASF-FR-MXGW, ASF-FR-MLKG).
 *
 * A card is a whole panel in one component. It keeps its own selection, and
 * its buttons send the context its kind declares (`AUI-agent-ui-surfaces.md`
 * AUI-FR-XCXV). A submitted card shows what was sent.
 */
import { useId, useMemo } from "react";
import { createComponentImplementation } from "@a2ui/react";
import { z } from "zod";

import { Icon } from "../../icons";
import { useSurfaceHost } from "./host";
import { useCardState } from "../model";

/** The resolved props of a card and its id. */
type Props = { component: Record<string, unknown> & { id: string } };

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
function useSent(component: { id: string }) {
  const { submission } = useSurfaceHost();
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
function ChoiceCardView({ component }: Props) {
  const { surfaceId, readOnly, sendCard } = useSurfaceHost();
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
    sendCard(component.id, { name: nameOf(component.action), context });
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
function ConfirmCardView({ component }: Props) {
  const { readOnly, sendCard } = useSurfaceHost();
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
          onPress={() => sendCard(component.id, { name: str(confirm.name), context: {} })}
        />
        {decline && (
          <SendButton
            label={str(decline.label)}
            disabled={readOnly}
            pressed={sent?.name === str(decline.name)}
            primary={false}
            onPress={() => sendCard(component.id, { name: str(decline.name), context: {} })}
          />
        )}
      </div>
    </div>
  );
}

/** ASF-FR-EOLY: options against criteria, and a pick where the card has one. */
function ComparisonCardView({ component }: Props) {
  const { surfaceId, readOnly, sendCard } = useSurfaceHost();
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
          onPress={() => sendCard(component.id, { name: str(pick.name), context: { picked } })}
        />
      )}
    </div>
  );
}

/** ASF-FR-MXGW: items with checkboxes, sent with none checked as well. */
function ChecklistCardView({ component }: Props) {
  const { surfaceId, readOnly, sendCard } = useSurfaceHost();
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
          sendCard(component.id, {
            name: nameOf(component.action),
            context: { checked: items.map((i) => i.value).filter((v) => checked.includes(v)) },
          })
        }
      />
    </div>
  );
}

const action = z.object({ name: z.string() });
const option = z.object({ value: z.string(), label: z.string(), description: z.string().optional() });

/** AUI contract surface: the extension cards. */
export const ChoiceCardApi = {
  name: "ChoiceCard",
  schema: z.object({
    question: z.string(),
    options: z.array(option),
    multiple: z.boolean().optional(),
    allowOther: z.boolean().optional(),
    sendLabel: z.string().optional(),
    action,
    weight: z.number().optional(),
  }),
};

export const ConfirmCardApi = {
  name: "ConfirmCard",
  schema: z.object({
    title: z.string(),
    body: z.string().optional(),
    confirm: z.object({ label: z.string(), name: z.string() }),
    decline: z.object({ label: z.string(), name: z.string() }).optional(),
    tone: z.enum(["default", "danger"]).optional(),
    weight: z.number().optional(),
  }),
};

export const ComparisonCardApi = {
  name: "ComparisonCard",
  schema: z.object({
    title: z.string().optional(),
    criteria: z.array(z.object({ key: z.string(), title: z.string() })),
    options: z.array(
      z.object({
        value: z.string(),
        label: z.string(),
        values: z.record(z.union([z.string(), z.number(), z.boolean()])),
        recommended: z.boolean().optional(),
      }),
    ),
    pick: z.object({ name: z.string(), label: z.string().optional() }).optional(),
    weight: z.number().optional(),
  }),
};

export const ChecklistCardApi = {
  name: "ChecklistCard",
  schema: z.object({
    title: z.string().optional(),
    items: z.array(option.extend({ checked: z.boolean().optional() })),
    sendLabel: z.string().optional(),
    action,
    weight: z.number().optional(),
  }),
};

/** A card implementation over one of the views above. */
function card<Api extends { name: string; schema: z.ZodTypeAny }>(
  api: Api,
  View: (props: Props) => React.ReactNode,
) {
  return createComponentImplementation(api, ({ props, context }) => (
    <View component={{ ...(props as Record<string, unknown>), id: context.componentModel.id }} />
  ));
}

/** ASF-FR-HRQL: the four cards. */
export const CARD_PARTS = [
  card(ChoiceCardApi, ChoiceCardView),
  card(ConfirmCardApi, ConfirmCardView),
  card(ComparisonCardApi, ComparisonCardView),
  card(ChecklistCardApi, ChecklistCardView),
];
