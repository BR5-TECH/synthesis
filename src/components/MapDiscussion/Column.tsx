/**
 * The Discussion column of the Map tab (`SMP-specification-map.md` SMP-FR-WNLR
 * .. SMP-FR-BOBB, `CMT-comments.md` CMT-FR-LVNQ).
 *
 * The column shows one thread at a time with the shared `DiscussionSurface`,
 * and begins a thread with the shared `DiscussionComposer`, as every owner
 * does. What it owns is the header with its thread chooser, the Focus chip,
 * and the empty states. A thread is read with `"list map discussions"` and
 * followed by the `"discussion changed"` event (the shared hook), and the
 * thread of the chosen node is also asked for with
 * `"find map discussion (map node)"`, which creates nothing (SMP-FR-IHFM). The
 * first message of an empty thread creates it with
 * `"get or create map discussion (map node, body, attachments)"`, and later
 * messages are added with `"add comment to discussion"` (SMP-FR-KHKC).
 */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

import { findMapDiscussion, getOrCreateMapDiscussion, listMapDiscussions } from "../../api";
import { identityBlockFor } from "../../hooks/useComments";
import { useDiscussions, COMPOSER_KEY } from "../../hooks/useDiscussions";
import { logDebug } from "../../logging";
import { requestPendingFocus } from "../../state/discussionFocus";
import { mapThreadKey, sameMapNode } from "../../state/specMap/discussion";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type {
  AttachmentInput,
  CommentQuote,
  Discussion,
  DiscussionTarget,
  MapNodeRef,
} from "../../types";
import { DiscussionComposer, DiscussionSurface } from "../discussion";
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

/** SMP-FR-XGBI: what the empty Map thread tells the author about the plan tools. */
function MapThreadHint({ planTool }: { planTool: boolean }) {
  return (
    <div className="mdc-empty" data-testid="map-discussion-empty">
      <p>Plan the delivery here with an agent. Each message is about the whole map unless it has a focus.</p>
      <p data-testid="map-plan-tool-note">
        The agent reads the plan and proposes changes only while a GitHub Project is the delivery
        source.
        {!planTool && " The delivery source is not a GitHub Project now, so the agent has no plan tool."}
      </p>
    </div>
  );
}

export function MapDiscussionColumn({ store, selected, planTool, planHost }: MapColumnProps) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const { node, focusSignal } = store.snapshot().discussion;
  const nodeKey = mapThreadKey(node);
  const nodeRef = useRef(node);
  nodeRef.current = node;

  const overrides = useMemo(
    () => ({
      list: listMapDiscussions,
      // SMP-FR-KHKC: the first message of a thread creates it, or joins the one
      // that another window created.
      open: ({
        body,
        attachments,
        spokenOriginal,
      }: {
        body: string;
        attachments: AttachmentInput[];
        spokenOriginal?: string;
      }) =>
        getOrCreateMapDiscussion({ mapNode: nodeRef.current, body, attachments, spokenOriginal }),
    }),
    [],
  );
  const discussions = useDiscussions(MAP_TARGET, true, undefined, overrides);
  const threads = useMemo(() => discussions.discussions.map((d) => d.thread), [discussions.discussions]);

  // SMP-FR-IHFM: the thread of the node, asked for and never created by this call.
  const [found, setFound] = useState<{ key: string; thread: Discussion | null } | null>(null);
  useEffect(() => {
    let cancelled = false;
    void findMapDiscussion(node)
      .then((thread) => {
        if (!cancelled) setFound({ key: nodeKey, thread: thread ?? null });
      })
      .catch(() => {
        if (!cancelled) setFound({ key: nodeKey, thread: null });
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodeKey]);
  const listed = threads.find((t) => sameMapNode(t.mapNode ?? null, node)) ?? null;
  const thread = listed ?? (found?.key === nodeKey ? found.thread : null);

  // SMP-FR-DUUU: the focus of the next message. In a node thread it starts as
  // the node of the thread, and it returns there after each post.
  const [focus, setFocus] = useState<MapNodeRef | null>(node);
  useEffect(() => {
    setFocus(nodeRef.current);
  }, [nodeKey]);
  const focusAttachments = useMemo<AttachmentInput[] | undefined>(
    () => (focus ? [{ kind: "mapNode", node: focus }] : undefined),
    [focus],
  );
  const chip = (
    <FocusChip focus={focus} selected={selected} onUse={setFocus} onClear={() => setFocus(null)} />
  );

  // SMP-FR-EIUP: the Map thread first, then every node thread, and the node the
  // column reads where it has no thread yet.
  const options = useMemo(() => {
    const list: { key: string; node: MapNodeRef | null; label: string }[] = [
      { key: mapThreadKey(null), node: null, label: "Map" },
    ];
    for (const t of threads) {
      if (t.mapNode) list.push({ key: mapThreadKey(t.mapNode), node: t.mapNode, label: t.mapNode.label });
    }
    if (node && !list.some((o) => o.key === nodeKey)) {
      list.push({ key: nodeKey, node, label: node.label });
    }
    return list;
  }, [threads, node, nodeKey]);

  const identityBlock = identityBlockFor(discussions.identityError);
  const block = identityBlock
    ? { message: identityBlock.message, route: identityBlock.route }
    : null;

  const reply = async (
    threadId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => {
    await discussions.reply(threadId, body, quotes, attachments);
    setFocus(nodeRef.current);
  };

  return (
    <MapPlanHostContext.Provider value={planHost}>
      <section
        className="dds-discussion mdc"
        aria-label="Discussion"
        data-testid="map-discussion-column"
      >
        <header className="dds-discussion__head mdc__head">
          <Icon.Comment size={12} />
          <label className="mdc__chooser">
            <span className="mdc__chooser-label t-eyebrow">Thread</span>
            <select
              aria-label="Thread"
              className="mdc__select"
              value={nodeKey}
              onChange={(e) => {
                const chosen = options.find((o) => o.key === e.target.value);
                if (!chosen) return;
                logDebug(["frontend"], "map discussion thread chosen", { threads: options.length });
                store.setDiscussionNode(chosen.node);
              }}
            >
              {options.map((o) => (
                <option key={o.key} value={o.key}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          {node && (
            <button
              type="button"
              className="mdc__node"
              title="Select this node on the canvas"
              onClick={() => store.showMapNode(node)}
            >
              <Icon.Node size={11} />
              <span className="mdc__node-name">{node.label}</span>
            </button>
          )}
        </header>
        <div className="dds-discussion__body">
          {!thread && block && (
            <div className="comment-rail__blocked" role="alert">
              {block.message}
              {block.route && <div className="comment-rail__route">{block.route}</div>}
            </div>
          )}
          {thread ? (
            <DiscussionSurface
              key={thread.id}
              discussion={thread}
              owner="column"
              variant="stream"
              agents={discussions.agents}
              identity={discussions.identity}
              identityBlock={block}
              disabled={block !== null}
              focused
              ownerLabel={node?.label ?? "Map"}
              error={discussions.errors[thread.id]}
              onReply={reply}
              onSetResolved={discussions.setResolved}
              composerFocusSignal={focusSignal}
              composerAccessory={chip}
              composerAttachments={focusAttachments}
            />
          ) : (
            <div className="mdc__opening">
              {node ? (
                <div className="mdc-empty" data-testid="map-discussion-empty">
                  <p>No one has discussed this node yet. Your first message begins its thread.</p>
                </div>
              ) : (
                <MapThreadHint planTool={planTool} />
              )}
              <DiscussionComposer
                mode="opening"
                target={MAP_TARGET}
                sessionKey={nodeKey}
                agents={discussions.agents}
                disabled={block !== null}
                error={discussions.errors[COMPOSER_KEY]}
                ariaLabel={node ? "Discuss this node" : "Discuss the map"}
                placeholder={node ? "Ask an agent about this node…" : "Ask an agent about the plan…"}
                focusSignal={focusSignal}
                accessory={chip}
                extraAttachments={focusAttachments}
                onOpen={(request) => discussions.open(request.body, request.attachments, null)}
                onOpened={(created) => {
                  setFocus(nodeRef.current);
                  requestPendingFocus(created.id, "composer");
                }}
              />
            </div>
          )}
        </div>
      </section>
    </MapPlanHostContext.Provider>
  );
}
