/**
 * The official A2UI renderer of a surface (`ASF-agent-surfaces.md`
 * ASF-FR-HRQL, ASF-FR-HFPF, ASF-FR-RTRX, ASF-FR-ENRG, ASF-FR-JLUJ).
 *
 * This module is the chunk that loads with the first panel. It replays the
 * record's message log into a v1.0 message processor of `@a2ui/web_core`, and
 * `A2uiSurface` of `@a2ui/react` draws it. The processor of an open surface is
 * kept for as long as the window runs, so the data model the author edits
 * stays when the panel moves to another owner surface.
 */
import { useEffect, useMemo } from "react";
import { A2uiSurface, MarkdownComponentContext, type ReactCatalogComponent } from "@a2ui/react";
import { MessageProcessor, injectBasicCatalogStyles, type SurfaceModel } from "@a2ui/web_core/v1_0";
import "material-symbols/outlined.css";

import { logWarn } from "../../../logging";
import { discussionCatalog } from "./catalog";
import { SurfaceHostProvider, type CardSend } from "./host";
import { SurfaceMarkdown } from "./parts";
import type { AgentSurface } from "../../../types";

/** What a press of a Button sends to the panel (ASF-FR-QBTR). */
export interface RendererAction {
  name: string;
  sourceComponentId: string;
  context: Record<string, unknown>;
  dataModel: Record<string, unknown>;
}

interface Entry {
  processor: MessageProcessor<ReactCatalogComponent>;
  surface: SurfaceModel<ReactCatalogComponent>;
  onAction: (action: RendererAction) => void;
}

/** ASF-FR-JLUJ: the processors of the surfaces of this window, by id and state. */
const entries = new Map<string, Entry>();

let stylesInjected = false;

/** For tests: forget every processor. */
export function resetSurfaceRenderersForTest(): void {
  for (const entry of entries.values()) entry.processor.dispose();
  entries.clear();
}

/** The line that stands in for a component the renderer does not know. */
export const UNKNOWN_PART_TEXT = "This part cannot be shown.";

/**
 * ASF-FR-SERF: a component of a type the catalog does not hold is drawn as a
 * caption line in its place, so the rest of the panel draws as usual.
 */
function knownParts(message: unknown): unknown {
  const m = message as Record<string, { components?: Array<Record<string, unknown>> } | undefined>;
  const known = discussionCatalog().components;
  const swap = (components: Array<Record<string, unknown>> | undefined) =>
    components?.map((c) =>
      typeof c.component === "string" && known.has(c.component)
        ? c
        : { id: c.id, component: "Text", text: UNKNOWN_PART_TEXT, variant: "caption" },
    );
  for (const key of ["createSurface", "updateComponents"] as const) {
    const body = m[key];
    if (body?.components) return { ...m, [key]: { ...body, components: swap(body.components) } };
  }
  return message;
}

/** The messages the renderer replays: the log, and the sent values of a submitted surface. */
function replayOf(record: AgentSurface): unknown[] {
  const messages: unknown[] = record.messages.map(knownParts);
  if (record.submission) {
    messages.push({
      version: "1.0",
      updateDataModel: { surfaceId: record.id, path: "/", value: record.submission.dataModel },
    });
  }
  return messages;
}

function entryOf(record: AgentSurface): Entry | null {
  // A record whose log grows draws again, so the key names its length too.
  const key = `${record.id}|${record.state}|${record.messages.length}`;
  const held = entries.get(key);
  if (held) return held;
  const entry: Partial<Entry> = { onAction: () => {} };
  const processor = new MessageProcessor<ReactCatalogComponent>(
    [discussionCatalog()],
    (payload) => {
      const surface = processor.getSurface(record.id);
      entry.onAction?.({
        name: payload.name,
        sourceComponentId: payload.sourceComponentId,
        context: (payload.context ?? {}) as Record<string, unknown>,
        dataModel: (surface?.dataModel.get("/") ?? {}) as Record<string, unknown>,
      });
    },
    {
      errorHandler: (error: unknown) => {
        // ASF-FR-MGVD: the id and the kind of error, never a component.
        logWarn(["frontend"], "agent surface message refused", {
          surfaceId: record.id,
          error: String((error as { code?: unknown })?.code ?? "unknown").slice(0, 60),
        });
      },
    } as never,
  );
  processor.processMessages(replayOf(record) as never);
  const surface = processor.getSurface(record.id);
  if (!surface) {
    processor.dispose();
    return null;
  }
  entry.processor = processor;
  entry.surface = surface;
  entries.set(key, entry as Entry);
  // A submitted surface takes no more input, so its open processor goes.
  // An older entry of the same surface, open or with a shorter log, goes.
  for (const [held, old] of entries) {
    if (held !== key && held.startsWith(`${record.id}|`)) {
      old.processor.dispose();
      entries.delete(held);
    }
  }
  return entry as Entry;
}

export default function SurfaceRenderer({
  record,
  readOnly,
  onAction,
  sendCard,
}: {
  record: AgentSurface;
  readOnly: boolean;
  onAction: (action: RendererAction) => void;
  sendCard: (sourceId: string, send: CardSend) => void;
}) {
  if (!stylesInjected) {
    // ASF-FR-HFPF: the official styles, mapped to the application's tokens
    // inside `.agent-surface` by `src/styles/kit/agent-surface.css`.
    injectBasicCatalogStyles();
    stylesInjected = true;
  }
  const entry = useMemo(() => entryOf(record), [record]);
  useEffect(() => {
    if (entry) entry.onAction = onAction;
  }, [entry, onAction]);
  // ASF-FR-MLKG: a card sends the panel's data model beside its own context.
  const host = useMemo(
    () => ({
      surfaceId: record.id,
      readOnly,
      submission: record.submission,
      sendCard: (sourceId: string, send: CardSend) =>
        sendCard(sourceId, {
          ...send,
          dataModel: (entry?.surface.dataModel.get("/") ?? {}) as Record<string, unknown>,
        }),
    }),
    [record.id, readOnly, record.submission, sendCard, entry],
  );
  if (!entry) return <p className="agent-surface__unknown">This part cannot be shown.</p>;
  return (
    <SurfaceHostProvider value={host}>
      <MarkdownComponentContext.Provider value={SurfaceMarkdown}>
        <fieldset className="agent-surface__fieldset" disabled={readOnly}>
          <A2uiSurface surface={entry.surface} />
        </fieldset>
      </MarkdownComponentContext.Provider>
    </SurfaceHostProvider>
  );
}
