/**
 * The drawing of each component of the discussion catalog
 * (`ASF-agent-surfaces.md` ASF-FR-HRQL to ASF-FR-GKYM, ASF-FR-TAVD).
 *
 * Every component draws with the application's own controls. A value is read
 * through `resolveValue`, an input writes its binding into the local model,
 * and a button hands its id to the panel, which resolves and sends the action.
 */
import {
  useId,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import { Icon } from "../icons";
import { CARD_RENDERERS } from "./cards";
import { asText, resolveValue, type Scope } from "./functions";
import { absolute, itemPointer } from "./pointer";
import type { SurfaceComponent } from "../../types";

import { useRender, type SurfaceRenderContext } from "./context";

export { SurfaceRenderProvider } from "./context";
export type { SurfaceRenderContext };

type Props = { component: SurfaceComponent; scope: Scope };

/** A string prop, resolved. */
function useText(value: unknown, scope: Scope): string {
  return asText(resolveValue(value, scope));
}

/** The binding pointer of a value, where it is one. */
function bindingOf(value: unknown, scope: Scope): string | undefined {
  if (value && typeof value === "object" && typeof (value as { "@path"?: unknown })["@path"] === "string") {
    return absolute((value as { "@path": string })["@path"], scope.base);
  }
  return undefined;
}

/** ASF-FR-GKYM: the messages of the false checks of a component. */
function failedChecks(component: SurfaceComponent, scope: Scope): string[] {
  const checks = component["@checks"];
  if (!Array.isArray(checks)) return [];
  const messages: string[] = [];
  for (const check of checks) {
    const rule = check as { condition?: unknown; message?: unknown };
    if (resolveValue(rule.condition, scope) !== true) {
      messages.push(typeof rule.message === "string" ? rule.message : "");
    }
  }
  return messages;
}

function CheckMessages({ messages }: { messages: string[] }) {
  const shown = messages.filter((m) => m.trim() !== "");
  if (shown.length === 0) return null;
  return (
    <ul className="agent-surface__checks">
      {shown.map((message, i) => (
        <li key={i}>{message}</li>
      ))}
    </ul>
  );
}

/** ASF-FR-TURS: the children of a container, static or a template. */
function Children({ list, scope }: { list: unknown; scope: Scope }) {
  const { model } = useRender();
  if (Array.isArray(list)) {
    return (
      <>
        {list.map((id, i) =>
          typeof id === "string" ? <Node key={`${id}-${i}`} id={id} scope={scope} /> : null,
        )}
      </>
    );
  }
  if (list && typeof list === "object") {
    const template = list as { componentId?: unknown; path?: unknown };
    if (typeof template.componentId !== "string" || typeof template.path !== "string") return null;
    const pointer = absolute(template.path, scope.base);
    const items = resolveValue({ "@path": pointer }, { ...scope, model });
    if (!Array.isArray(items)) return null;
    return (
      <>
        {items.map((_, index) => (
          <Node
            key={`${template.componentId}-${index}`}
            id={template.componentId as string}
            scope={{ model, base: itemPointer(pointer, index), index }}
          />
        ))}
      </>
    );
  }
  return null;
}

function flexOf(component: SurfaceComponent): CSSProperties | undefined {
  const weight = component.weight;
  return typeof weight === "number" ? { flexGrow: weight, flexBasis: 0, minWidth: 0 } : undefined;
}

const JUSTIFY: Record<string, string> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  spaceBetween: "space-between",
  spaceAround: "space-around",
  spaceEvenly: "space-evenly",
  stretch: "stretch",
};
const ALIGN: Record<string, string> = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  stretch: "stretch",
};

function Stack({ component, scope, direction }: Props & { direction: "row" | "column" }) {
  return (
    <div
      className={`agent-surface__stack agent-surface__stack--${direction}`}
      style={{
        justifyContent: JUSTIFY[String(component.justify)] ?? undefined,
        alignItems: ALIGN[String(component.align)] ?? undefined,
        ...flexOf(component),
      }}
    >
      <Children list={component.children} scope={scope} />
    </div>
  );
}

function ListView({ component, scope }: Props) {
  const direction = component.direction === "horizontal" ? "row" : "column";
  return (
    <div
      className={`agent-surface__stack agent-surface__stack--${direction} agent-surface__list`}
      role="list"
      style={{ alignItems: ALIGN[String(component.align)] ?? undefined, ...flexOf(component) }}
    >
      <Children list={component.children} scope={scope} />
    </div>
  );
}

function CardView({ component, scope }: Props) {
  return (
    <div className="agent-surface__card" style={flexOf(component)}>
      {typeof component.child === "string" && <Node id={component.child} scope={scope} />}
    </div>
  );
}

/** ASF-FR-TAVD: the WAI-ARIA tabs pattern. */
function TabsView({ component, scope }: Props) {
  const tabs = Array.isArray(component.tabs)
    ? (component.tabs as Array<{ title?: unknown; child?: unknown }>)
    : [];
  const [selected, setSelected] = useState(0);
  const base = useId();
  const current = Math.min(selected, Math.max(tabs.length - 1, 0));
  const titles = tabs.map((tab) => asText(resolveValue(tab.title, scope)));
  const onKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (step === 0 || tabs.length === 0) return;
    event.preventDefault();
    const next = (current + step + tabs.length) % tabs.length;
    setSelected(next);
    document.getElementById(`${base}-tab-${next}`)?.focus();
  };
  const child = tabs[current]?.child;
  return (
    <div className="agent-surface__tabs" style={flexOf(component)}>
      <div className="agent-surface__tablist" role="tablist">
        {titles.map((title, i) => (
          <button
            key={i}
            id={`${base}-tab-${i}`}
            type="button"
            role="tab"
            className="agent-surface__tab"
            aria-selected={i === current}
            aria-controls={`${base}-panel`}
            tabIndex={i === current ? 0 : -1}
            onClick={() => setSelected(i)}
            onKeyDown={onKey}
          >
            {title}
          </button>
        ))}
      </div>
      <div
        id={`${base}-panel`}
        role="tabpanel"
        aria-labelledby={`${base}-tab-${current}`}
        className="agent-surface__tabpanel"
      >
        {typeof child === "string" && <Node id={child} scope={scope} />}
      </div>
    </div>
  );
}

function DividerView({ component }: Props) {
  return (
    <hr
      className={`agent-surface__divider agent-surface__divider--${
        component.axis === "vertical" ? "vertical" : "horizontal"
      }`}
      aria-orientation={component.axis === "vertical" ? "vertical" : "horizontal"}
    />
  );
}

function TextView({ component, scope }: Props) {
  const text = useText(component.text, scope);
  const caption = component.variant === "caption";
  return (
    <p
      className={`agent-surface__text${caption ? " agent-surface__text--caption" : ""}`}
      style={flexOf(component)}
    >
      {text}
    </p>
  );
}

const ICONS: Record<string, (p: { size?: number }) => ReactNode> = {
  check: Icon.Check,
  close: Icon.X,
  add: Icon.Plus,
  delete: Icon.Trash,
  search: Icon.Search,
  settings: Icon.Settings,
  home: Icon.Home,
  info: Icon.Info,
  warning: Icon.Warning,
  error: Icon.ErrorCircle,
  lock: Icon.Lock,
  folder: Icon.Folder,
  refresh: Icon.Refresh,
  play: Icon.Play,
  stop: Icon.Stop,
  pause: Icon.Pause,
  menu: Icon.Menu,
  download: Icon.PullDown,
  upload: Icon.PushUp,
};

function IconView({ component }: Props) {
  const name = String(component.name);
  const Glyph = ICONS[name];
  return (
    <span className="agent-surface__icon" aria-hidden="true" data-icon={name}>
      {Glyph ? <Glyph size={14} /> : <span className="agent-surface__icon-dot" />}
    </span>
  );
}

function ButtonView({ component, scope }: Props) {
  const { readOnly, press, submission } = useRender();
  const pressedId = submission?.sourceComponentId;
  const failed = failedChecks(component, scope);
  const variant = component.variant === "primary" ? "btn--primary" : component.variant === "borderless" ? "btn--ghost" : "";
  const pressed = pressedId === component.id;
  return (
    <div className="agent-surface__button" style={flexOf(component)}>
      <button
        type="button"
        className={`btn btn--sm ${variant}`.trim()}
        disabled={readOnly || failed.length > 0}
        data-pressed={pressed ? "true" : undefined}
        aria-pressed={readOnly ? pressed : undefined}
        onClick={() => press(component.id, scope)}
      >
        {pressed && <Icon.Check size={12} />}
        {typeof component.child === "string" && <Node id={component.child} scope={scope} inline />}
      </button>
      <CheckMessages messages={failed} />
    </div>
  );
}

/** A label of an input: its `label`, or its accessibility label. */
function useLabel(component: SurfaceComponent, scope: Scope): string {
  const label = useText(component.label, scope);
  const accessible = useText(
    (component["@accessibility"] as { label?: unknown } | undefined)?.label,
    scope,
  );
  return label || accessible;
}

function TextFieldView({ component, scope }: Props) {
  const { readOnly, write } = useRender();
  const label = useLabel(component, scope);
  const binding = bindingOf(component.value, scope);
  const value = useText(component.value, scope);
  const [own, setOwn] = useState(value);
  const shown = binding ? value : own;
  const placeholder = useText(component.placeholder, scope);
  const onChange = (next: string) => (binding ? write(binding, next) : setOwn(next));
  const failed = failedChecks(component, scope);
  const long = component.variant === "longText";
  const type = component.variant === "number" ? "number" : "text";
  return (
    <label className="agent-surface__field" style={flexOf(component)}>
      <span className="agent-surface__label">{label}</span>
      {long ? (
        <textarea
          className="textarea input--sm"
          value={shown}
          placeholder={placeholder || undefined}
          disabled={readOnly}
          rows={3}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <input
          className="input input--sm"
          type={type}
          value={shown}
          placeholder={placeholder || undefined}
          disabled={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
      <CheckMessages messages={failed} />
    </label>
  );
}

function CheckBoxView({ component, scope }: Props) {
  const { readOnly, write } = useRender();
  const label = useLabel(component, scope);
  const binding = bindingOf(component.value, scope);
  const value = resolveValue(component.value, scope) === true;
  const [own, setOwn] = useState(value);
  const checked = binding ? value : own;
  return (
    <div className="agent-surface__field" style={flexOf(component)}>
      <label className="agent-surface__choice">
        <input
          type="checkbox"
          checked={checked}
          disabled={readOnly}
          onChange={(e) => (binding ? write(binding, e.target.checked) : setOwn(e.target.checked))}
        />
        <span>{label}</span>
      </label>
      <CheckMessages messages={failedChecks(component, scope)} />
    </div>
  );
}

function ChoicePickerView({ component, scope }: Props) {
  const { readOnly, write } = useRender();
  const label = useLabel(component, scope);
  const name = useId();
  const binding = bindingOf(component.value, scope);
  const resolved = resolveValue(component.value, scope);
  const [own, setOwn] = useState<string[]>(Array.isArray(resolved) ? resolved.map(String) : []);
  const selected = binding ? (Array.isArray(resolved) ? resolved.map(String) : []) : own;
  const [filter, setFilter] = useState("");
  const multiple = component.variant === "multipleSelection";
  const chips = component.displayStyle === "chips";
  const options = (Array.isArray(component.options) ? component.options : []) as Array<{
    label?: unknown;
    value?: unknown;
  }>;
  const set = (next: string[]) => (binding ? write(binding, next) : setOwn(next));
  const toggle = (value: string) => {
    if (multiple) {
      set(selected.includes(value) ? selected.filter((v) => v !== value) : [...selected, value]);
    } else {
      set([value]);
    }
  };
  const shown = options
    .map((o) => ({ label: asText(resolveValue(o.label, scope)), value: String(o.value) }))
    .filter((o) => filter.trim() === "" || o.label.toLowerCase().includes(filter.trim().toLowerCase()));
  return (
    <fieldset className="agent-surface__field agent-surface__picker" style={flexOf(component)}>
      {label && <legend className="agent-surface__label">{label}</legend>}
      {component.filterable === true && (
        <input
          className="input input--sm"
          type="search"
          aria-label={label ? `Filter ${label}` : "Filter"}
          value={filter}
          disabled={readOnly}
          onChange={(e) => setFilter(e.target.value)}
        />
      )}
      <div className={chips ? "agent-surface__chips" : "agent-surface__options"}>
        {shown.map((option, index) =>
          chips ? (
            <button
              key={index}
              type="button"
              className="agent-surface__chip"
              aria-pressed={selected.includes(option.value)}
              disabled={readOnly}
              onClick={() => toggle(option.value)}
            >
              {option.label}
            </button>
          ) : (
            <label key={index} className="agent-surface__choice">
              <input
                type={multiple ? "checkbox" : "radio"}
                name={name}
                checked={selected.includes(option.value)}
                disabled={readOnly}
                onChange={() => toggle(option.value)}
              />
              <span>{option.label}</span>
            </label>
          ),
        )}
      </div>
      <CheckMessages messages={failedChecks(component, scope)} />
    </fieldset>
  );
}

function SliderView({ component, scope }: Props) {
  const { readOnly, write } = useRender();
  const label = useLabel(component, scope);
  const binding = bindingOf(component.value, scope);
  const min = typeof component.min === "number" ? component.min : 0;
  const max = typeof component.max === "number" ? component.max : 100;
  const steps = typeof component.steps === "number" && component.steps > 0 ? component.steps : undefined;
  const step = steps ? (max - min) / steps : "any";
  const resolved = Number(resolveValue(component.value, scope));
  const [own, setOwn] = useState(Number.isFinite(resolved) ? resolved : min);
  const value = binding ? (Number.isFinite(resolved) ? resolved : min) : own;
  return (
    <label className="agent-surface__field" style={flexOf(component)}>
      <span className="agent-surface__label">
        {label}
        <output className="agent-surface__value">{value}</output>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={readOnly}
        onChange={(e) => (binding ? write(binding, Number(e.target.value)) : setOwn(Number(e.target.value)))}
      />
      <CheckMessages messages={failedChecks(component, scope)} />
    </label>
  );
}

function DateTimeInputView({ component, scope }: Props) {
  const { readOnly, write } = useRender();
  const label = useLabel(component, scope);
  const binding = bindingOf(component.value, scope);
  const value = useText(component.value, scope);
  const [own, setOwn] = useState(value);
  const date = component.enableDate === true;
  const time = component.enableTime === true;
  const type = date && time ? "datetime-local" : time ? "time" : "date";
  const min = useText(component.min, scope);
  const max = useText(component.max, scope);
  return (
    <label className="agent-surface__field" style={flexOf(component)}>
      <span className="agent-surface__label">{label}</span>
      <input
        className="input input--sm"
        type={type}
        value={binding ? value : own}
        min={min || undefined}
        max={max || undefined}
        disabled={readOnly}
        onChange={(e) => (binding ? write(binding, e.target.value) : setOwn(e.target.value))}
      />
      <CheckMessages messages={failedChecks(component, scope)} />
    </label>
  );
}

/** ASF-FR-GZFO: a header cell per column and a row per object. */
function TableView({ component, scope }: Props) {
  const columns = (Array.isArray(component.columns) ? component.columns : []) as Array<{
    key?: unknown;
    title?: unknown;
  }>;
  const rows = resolveValue(component.rows, scope);
  const caption = useText(component.caption, scope);
  const list = Array.isArray(rows) ? rows : [];
  return (
    <div className="agent-surface__table-wrap" style={flexOf(component)}>
      <table className="agent-surface__table">
        {caption && <caption>{caption}</caption>}
        <thead>
          <tr>
            {columns.map((column, i) => (
              <th key={i} scope="col">
                {asText(resolveValue(column.title, scope))}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {list.map((row, r) => (
            <tr key={r}>
              {columns.map((column, c) => (
                <td key={c}>
                  {row && typeof row === "object"
                    ? asText((row as Record<string, unknown>)[String(column.key)])
                    : ""}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** ASF-FR-HRQL: the one drawing of each catalog component. */
export const RENDERERS: Record<string, (props: Props) => ReactNode> = {
  ...CARD_RENDERERS,
  Row: (p) => <Stack {...p} direction="row" />,
  Column: (p) => <Stack {...p} direction="column" />,
  List: ListView,
  Card: CardView,
  Tabs: TabsView,
  Divider: DividerView,
  Text: TextView,
  Icon: IconView,
  Button: ButtonView,
  TextField: TextFieldView,
  CheckBox: CheckBoxView,
  ChoicePicker: ChoicePickerView,
  Slider: SliderView,
  DateTimeInput: DateTimeInputView,
  Table: TableView,
};

/** ASF-FR-SERF: one component by id, or the line that stands for one it can not draw. */
export function Node({ id, scope, inline }: { id: string; scope: Scope; inline?: boolean }) {
  const { byId, model } = useRender();
  const component = byId.get(id);
  if (!component) return null;
  const scoped = { ...scope, model };
  const Renderer = Object.prototype.hasOwnProperty.call(RENDERERS, component.component)
    ? RENDERERS[component.component]
    : undefined;
  if (!Renderer) {
    return <p className="agent-surface__unknown">This part cannot be shown.</p>;
  }
  // A button label is the text of its child, drawn inside the button.
  if (inline && component.component === "Text") {
    return <span>{asText(resolveValue(component.text, scoped))}</span>;
  }
  return <Renderer component={component} scope={scoped} />;
}
