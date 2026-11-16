/**
 * The Discussion column of the Map tab (`SMP-specification-map.md` SMP-FR-WNLR
 * .. SMP-FR-BOBB, `CMT-comments.md` CMT-FR-LVNQ).
 *
 * The column is the discussion column of the draft and the file
 * (`DDS-draft-discussion.md`): one tab for each open thread, the control that
 * begins another, the resolved disclosure, the header menu that resolves and
 * reopens, and the shared surface and composer. What the Map adds is the name
 * of each tab, the node of a thread in the header, the Focus chip, and its own
 * words for the opening composer.
 *
 * The threads are read with `"list map discussions"` and followed by the
 * `"discussion changed"` event (the shared hook). **Discuss** on a node asks
 * for the latest open thread of the node with `"find map discussion (map node)"`,
 * which creates nothing (SMP-FR-IHFM). Each opening composer creates a new thread
 * with `"open map discussion (map node, body, attachments)"` (SMP-FR-KHKC), and
 * later messages are added with `"add comment to discussion"`.
 */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

import { findMapDiscussion, listMapDiscussions, openMapDiscussion } from "../../api";
import { identityBlockFor } from "../../hooks/useComments";
import { useDiscussions } from "../../hooks/useDiscussions";
import { logDebug, logWarn } from "../../logging";
import { requestPendingFocus } from "../../state/discussionFocus";
import { MAP_COLUMN } from "../../state/discussionColumn";
import { mapThreadKey, sameMapNode } from "../../state/specMap/discussion";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { AttachmentInput, Discussion, DiscussionTarget, MapNodeRef } from "../../types";
import { DiscussionColumn } from "../DraftDiscussion/DiscussionColumn";
import type { OpeningCopy, OpenState } from "../DraftDiscussion/OpenDiscussion";
import { Icon } from "../icons";
import { MapPlanHostContext, type MapPlanHost } from "../MapPlanProposal";
import { FocusChip } from "./FocusChip";

export interface MapColumnProps {
  store: SpecMapSessionStore;
  /** SMP-FR-DUUU: the node selected on the canvas of the active lens, or null. */
  selected: MapNodeRef | null;
  /** SMP-FR-XGBI: whether the delivery source is a GitHub Project, so the agent has the plan tools. */
  planTool: boolean;
  /** MPV-FR-NMYT: what the cards of the column need of the canvas. */
  planHost: MapPlanHost | null;
}

const MAP_TARGET: DiscussionTarget = { kind: "map" };

/** SMP-FR-EIUP: what the tab of a thread reads. */
export function mapTabLabel(thread: Discussion): string {
  return thread.mapNode?.label ?? "Map";
}

/** SMP-FR-XGBI: what the opening composer of a whole-map thread says of the plan tools. */
function planToolNote(planTool: boolean): string {
  return planTool
    ? "The agent reads the plan and proposes changes only while a GitHub Project is the delivery source."
    : "The agent reads the plan and proposes changes only while a GitHub Project is the delivery source. " +
        "The delivery source is not a GitHub Project now, so the agent has no plan tool.";
}

/** SMP-FR-XGBI / SMP-FR-IHFM: the words of the opening composer. */
function openingCopy(node: MapNodeRef | null, planTool: boolean): OpeningCopy {
  return {
    statement: (state: OpenState, resolved: number) => {
      if (node) {
        return {
          lead: `This begins a discussion about ${node.label}.`,
          hint: "Each message is about this node unless it has another focus. The other threads stay where they are.",
        };
      }
      if (state === "resolved") {
        return {
          lead:
            resolved === 1
              ? "The discussion about the map is resolved."
              : "Every discussion about the map is resolved.",
          hint: `Open ${resolved === 1 ? "it" : "them"} above to read what was said, or write below to begin another. ${planToolNote(planTool)}`,
        };
      }
      return {
        lead:
          state === "another"
            ? "This begins another discussion about the map."
            : "Plan the delivery here with an agent. Each message is about the whole map unless it has a focus.",
        hint: planToolNote(planTool),
      };
    },
    ariaLabel: node ? "Discuss this node" : "Discuss the map",
    placeholder: node ? "Ask an agent about this node…" : "Ask an agent about the plan…",
    sessionKey: mapThreadKey(node),
  };
}

export function MapDiscussionColumn({ store, selected, planTool, planHost }: MapColumnProps) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const state = store.snapshot().discussion;
  const { node, composing, threadId, focusSignal, discussSignal } = state;
  const nodeRef = useRef(node);
  nodeRef.current = node;

  const overrides = useMemo(
    () => ({
      list: listMapDiscussions,
      // SMP-FR-KHKC: each opening composer begins a new thread, about the node
      // it was opened for or about the whole map.
      open: ({
        body,
        attachments,
        spokenOriginal,
      }: {
        body: string;
        attachments: AttachmentInput[];
        spokenOriginal?: string;
      }) => openMapDiscussion({ mapNode: nodeRef.current, body, attachments, spokenOriginal }),
    }),
    [],
  );
  const discussions = useDiscussions(MAP_TARGET, true, undefined, overrides);
  const threads = useMemo(() => discussions.discussions.map((d) => d.thread), [discussions.discussions]);

  // SMP-FR-IHFM: Discuss on a node asks for its latest open thread. Until the
  // answer, the column holds the opening composer of the node, and it keeps it
  // where the node has no open thread. A later Discuss supersedes the answer.
  const latestDiscuss = useRef(discussSignal);
  latestDiscuss.current = discussSignal;
  useEffect(() => {
    if (discussSignal === 0 || !node) return;
    if (store.snapshot().discussion.lookedSignal >= discussSignal) return;
    const asked = discussSignal;
    void findMapDiscussion(node)
      .then((found) => {
        store.noteDiscussLooked(asked);
        if (latestDiscuss.current !== asked) return;
        logDebug(["frontend"], "map discussion of a node looked up", { found: !!found });
        // The author can choose a tab, or begin another discussion, before the
        // answer comes. That choice stands.
        const held = store.snapshot().discussion;
        if (!held.composing || !sameMapNode(held.node, node)) return;
        if (!found) return;
        // The reply composer of the thread takes the caret that the opening
        // composer held, so the author writes on (SMP-FR-IHFM).
        requestPendingFocus(found.id, "composer");
        store.selectDiscussionThread(found.id);
      })
      .catch((error: unknown) => {
        store.noteDiscussLooked(asked);
        logWarn(["frontend"], "map discussion look-up of a node failed", { error: String(error) });
      });
    // The node is the one the Discuss named; the signal is what starts a look-up.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [discussSignal]);

  // The thread the column shows, as the shared column picks it.
  const open = threads.filter((t) => !t.resolved);
  const offered = state.showResolved ? [...open, ...threads.filter((t) => t.resolved)] : open;
  const shown = composing ? undefined : (offered.find((t) => t.id === threadId) ?? offered[0]);
  const subject = composing ? node : (shown?.mapNode ?? null);

  // SMP-FR-DUUU: the focus of the next message. It starts as the node of the
  // thread or of the opening composer, and it returns there after each post.
  const subjectKey = composing ? `new:${mapThreadKey(node)}` : `thread:${shown?.id ?? ""}`;
  const subjectRef = useRef(subject);
  subjectRef.current = subject;
  const [focus, setFocus] = useState<MapNodeRef | null>(subject);
  useEffect(() => {
    setFocus(subjectRef.current);
  }, [subjectKey]);
  const focusAttachments = useMemo<AttachmentInput[] | undefined>(
    () => (focus ? [{ kind: "mapNode", node: focus }] : undefined),
    [focus],
  );
  const chip = (
    <FocusChip focus={focus} selected={selected} onUse={setFocus} onClear={() => setFocus(null)} />
  );

  const identityBlock = identityBlockFor(discussions.identityError);
  const block = identityBlock
    ? { message: identityBlock.message, route: identityBlock.route }
    : null;

  // SMP-FR-EIUP: the header names the node of the thread, and selecting the
  // name selects the node on the canvas of its lens.
  const header = subject ? (
    <button
      type="button"
      className="mdc__node"
      title="Select this node on the canvas"
      data-testid="map-discussion-node"
      onClick={() => store.showMapNode(subject)}
    >
      <Icon.Node size={11} />
      <span className="mdc__node-name">{subject.label}</span>
    </button>
  ) : null;

  return (
    <MapPlanHostContext.Provider value={planHost}>
      <DiscussionColumn
        column={MAP_COLUMN}
        target={MAP_TARGET}
        noun="map"
        material="map"
        testId="map-discussion-column"
        discussions={threads}
        selectedThreadId={threadId}
        onSelectThread={(id) => {
          logDebug(["frontend"], "map discussion thread chosen", { threads: threads.length });
          store.selectDiscussionThread(id);
        }}
        identity={discussions.identity}
        identityBlock={block}
        agents={discussions.agents}
        onReply={async (id, body, quotes, attachments) => {
          await discussions.reply(id, body, quotes, attachments);
          setFocus(subjectRef.current);
        }}
        onSetResolved={discussions.setResolved}
        errors={discussions.errors}
        blocked={false}
        anchor={null}
        onOpenDiscussion={(request) => discussions.open(request.body, request.attachments, null)}
        onOpened={(created) => {
          store.selectDiscussionThread(created.id);
          requestPendingFocus(created.id, "composer");
        }}
        ownerLabel={subject?.label ?? "Map"}
        focusSignal={focusSignal}
        showResolved={state.showResolved}
        onShowResolved={store.setDiscussionShowResolved}
        tabLabel={mapTabLabel}
        composing={composing}
        onComposingChange={(next) => {
          // SMP-FR-KHKC: the control that begins another discussion begins one
          // about the whole map.
          if (next) store.composeDiscussion(null);
          else store.cancelDiscussionComposer();
        }}
        headerAccessory={header}
        composerAccessory={chip}
        composerAttachments={focusAttachments}
        opening={openingCopy(node, planTool)}
      />
    </MapPlanHostContext.Provider>
  );
}
