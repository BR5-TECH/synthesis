/**
 * The application's own parts of the discussion catalog
 * (`ASF-agent-surfaces.md` ASF-FR-NOBK, ASF-FR-IKTX, ASF-FR-NEPE,
 * ASF-FR-QXYW, ASF-FR-GZFO): media from the sources the application allows,
 * the dialog of the one-overlay rule, the table, and the Markdown of `Text`.
 */
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { createComponentImplementation } from "@a2ui/react";
import {
  AudioPlayerApi,
  ButtonApi,
  CommonSchemas,
  ImageApi,
  ModalApi,
  VideoApi,
  componentId,
} from "@a2ui/web_core/v1_0";
import { z } from "zod";

import { readSurfaceMedia } from "../../../api";
import { logWarn } from "../../../logging";
import { CommentMarkdown } from "../../CommentMarkdown";
import { Modal as AppModal } from "../../DraftsPanelParts";
import { useSurfaceHost } from "./host";
import { openLink } from "./links";

/** AUI-FR-GUUJ: whether a source reads through the backend. */
function isLocalSource(url: string): boolean {
  return url.startsWith("project:") || url.startsWith("attachment:");
}

/**
 * ASF-FR-NOBK: the address a media element loads: an `https` address as it
 * is, a project file or an attachment as a data URL read through the
 * backend, and `null` for any other source or a read that failed.
 */
function useMediaSource(url: unknown): { src: string | null; failed: boolean } {
  const { surfaceId } = useSurfaceHost();
  const source = typeof url === "string" ? url : "";
  const [state, setState] = useState<{ source: string; src: string | null; failed: boolean }>(() => ({
    source: "",
    src: null,
    failed: false,
  }));
  useEffect(() => {
    if (!isLocalSource(source)) return;
    let cancelled = false;
    readSurfaceMedia(surfaceId, source).then(
      (media) => {
        if (!cancelled) setState({ source, src: `data:${media.mediaType};base64,${media.data}`, failed: false });
      },
      (error: unknown) => {
        if (cancelled) return;
        // ASF-FR-MGVD: the id and the typed code, never the source.
        logWarn(["frontend"], "agent surface media could not be read", {
          surfaceId,
          error: String(error instanceof Error ? error.message : error).slice(0, 40),
        });
        setState({ source, src: null, failed: true });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [surfaceId, source]);
  if (source.startsWith("https://")) return { src: source, failed: false };
  if (!isLocalSource(source)) return { src: null, failed: true };
  if (state.source !== source) return { src: null, failed: false };
  return { src: state.src, failed: state.failed };
}

function Unavailable() {
  return <p className="agent-surface__unknown">This media cannot be shown.</p>;
}

/** ASF-FR-NOBK: `Image`. */
const Image = createComponentImplementation(ImageApi, ({ props }) => {
  const { src, failed } = useMediaSource(props.url);
  if (failed) return <Unavailable />;
  if (!src) return <span className="agent-surface__media-loading" aria-hidden="true" />;
  const fit = typeof props.fit === "string" ? props.fit : "contain";
  return (
    <img
      className={`agent-surface__image agent-surface__image--${String(props.variant ?? "mediumFeature")}`}
      src={src}
      alt={typeof props.accessibility?.label === "string" ? props.accessibility.label : ""}
      style={{ objectFit: fit as React.CSSProperties["objectFit"] }}
    />
  );
});

/** ASF-FR-NOBK / ASF-FR-IKTX: `Video`, which plays on a press alone. */
const Video = createComponentImplementation(VideoApi, ({ props }) => {
  const { src, failed } = useMediaSource(props.url);
  if (failed) return <Unavailable />;
  if (!src) return <span className="agent-surface__media-loading" aria-hidden="true" />;
  return <video className="agent-surface__video" src={src} controls preload="metadata" />;
});

/** ASF-FR-NOBK / ASF-FR-IKTX: `AudioPlayer`, which plays on a press alone. */
const AudioPlayer = createComponentImplementation(AudioPlayerApi, ({ props }) => {
  const { src, failed } = useMediaSource(props.url);
  if (failed) return <Unavailable />;
  if (!src) return <span className="agent-surface__media-loading" aria-hidden="true" />;
  const label = typeof props.description === "string" ? props.description : "Audio";
  return <audio className="agent-surface__audio" src={src} controls preload="metadata" aria-label={label} />;
});

/**
 * ASF-FR-GKYM / ASF-FR-HRQL: `Button`, drawn with the application's button
 * and its own check messages. A false check disables it, and the message of
 * each false check shows under it.
 */
const Button = createComponentImplementation(ButtonApi, ({ props, buildChild }) => {
  const variant =
    props.variant === "primary" ? "btn--primary" : props.variant === "borderless" ? "btn--ghost" : "";
  const invalid = props.isValid === false;
  const messages = (props.validationErrors ?? []).filter((m) => typeof m === "string" && m.trim() !== "");
  return (
    <span className="agent-surface__button">
      <button
        type="button"
        className={`btn btn--sm ${variant}`.trim()}
        disabled={invalid}
        onClick={() => {
          if (!invalid) void props.action?.();
        }}
      >
        {typeof props.child === "string" ? buildChild(props.child) : null}
      </button>
      {invalid && messages.length > 0 && (
        <ul className="agent-surface__checks">
          {messages.map((message, i) => (
            <li key={i}>{message}</li>
          ))}
        </ul>
      )}
    </span>
  );
});

/**
 * ASF-FR-NEPE: `Modal` opens its content in the application's shared dialog,
 * which is the one floating overlay of the window, closes on Escape, and keeps
 * the focus inside.
 */
const Modal = createComponentImplementation(ModalApi, ({ props, buildChild }) => {
  const { readOnly } = useSurfaceHost();
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLSpanElement>(null);
  // The focus goes back to the control that opened the dialog.
  const returnFocus = () =>
    (triggerRef.current?.querySelector("button, [tabindex], input, a") as HTMLElement | null) ?? null;
  // The trigger opens the dialog and nothing else: the press stops here, so a
  // Button used as a trigger sends no action.
  const onTrigger = (event: MouseEvent) => {
    if (readOnly) return;
    event.preventDefault();
    event.stopPropagation();
    setOpen(true);
  };
  return (
    <>
      <span className="agent-surface__modal-trigger" ref={triggerRef} onClickCapture={onTrigger}>
        {typeof props.trigger === "string" ? buildChild(props.trigger) : null}
      </span>
      {open && (
        <AppModal label="Panel" onClose={() => setOpen(false)} returnFocus={returnFocus()}>
          <div className="agent-surface agent-surface__modal-content" data-testid="agent-surface-modal">
            {typeof props.content === "string" ? buildChild(props.content) : null}
            <div className="agent-surface__modal-actions">
              <button type="button" className="btn btn--sm" onClick={() => setOpen(false)}>
                Close
              </button>
            </div>
          </div>
        </AppModal>
      )}
    </>
  );
});

/** AUI contract surface: the `Table` extension. */
export const TableApi = {
  name: "Table",
  schema: z.object({
    columns: z.array(z.object({ key: z.string(), title: CommonSchemas.DynamicString })),
    rows: CommonSchemas.DynamicValue,
    caption: CommonSchemas.DynamicString.optional(),
    weight: z.number().optional(),
  }),
};

function cellText(value: unknown): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

/** ASF-FR-GZFO: a header cell per column and a row per object. */
const Table = createComponentImplementation(TableApi, ({ props }) => {
  const columns = (props.columns ?? []) as Array<{ key: string; title: unknown }>;
  const rows = Array.isArray(props.rows) ? props.rows : [];
  return (
    <div className="agent-surface__table-wrap">
      <table className="agent-surface__table">
        {typeof props.caption === "string" && props.caption && <caption>{props.caption}</caption>}
        <thead>
          <tr>
            {columns.map((column, i) => (
              <th key={i} scope="col">
                {cellText(column.title)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, r) => (
            <tr key={r}>
              {columns.map((column, c) => (
                <td key={c}>
                  {row && typeof row === "object" ? cellText((row as Record<string, unknown>)[column.key]) : ""}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
});

/**
 * ASF-FR-QXYW: the Markdown of `Text`, drawn by the application's Markdown
 * renderer with no raw HTML. A link opens through the application's opener.
 */
export function SurfaceMarkdown({ markdown }: { markdown: string }) {
  const onClick = (event: MouseEvent<HTMLSpanElement>) => {
    // The application's Markdown renders a link inert, with its address in
    // `title`; the panel opens it through the opener.
    const anchor = (event.target as HTMLElement).closest("a[title], a[href]");
    if (!anchor) return;
    event.preventDefault();
    event.stopPropagation();
    openLink(anchor.getAttribute("href") ?? anchor.getAttribute("title") ?? "");
  };
  return (
    <span className="agent-surface__markdown" onClickCapture={onClick}>
      <CommentMarkdown body={markdown} className="agent-surface__markdown-body" />
    </span>
  );
}

export const APP_PARTS = [Button, Image, Video, AudioPlayer, Modal, Table];

/** For tests: the names the application draws itself. */
export const APP_PART_NAMES = APP_PARTS.map((part) => part.name);

export { componentId };
