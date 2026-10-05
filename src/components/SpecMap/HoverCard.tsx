import { specFolder, type TreeIndex } from "../../state/specMap/tree";
import { EMPTY_ROLLUP, rollupOfSpec, type Rollup } from "../../state/specMap/rollups";
import type { MapRef } from "../../state/specMap/types";
import type { MapView } from "../../state/specMap/session";
import type { MapLayout } from "./layout";
import { placeHoverCard } from "./zoom";
import { CompletenessBar } from "./CompletenessBar";

interface HoverCardProps {
  tree: TreeIndex;
  layout: MapLayout;
  rollups: Map<string, Rollup>;
  target: MapRef;
  view: MapView;
  width: number;
  height: number;
}

/** SMN-FR-MVWA / SMN-FR-FXRL / SMN-FR-YSKQ: the card for the hovered node. */
export function HoverCard({ tree, layout, rollups, target, view, width, height }: HoverCardProps) {
  let content: {
    x: number;
    y: number;
    badge: string;
    folder: string;
    label: string;
    summary: string;
    rollup: Rollup;
    meta: string;
  } | null = null;

  if (target.kind === "spec") {
    const spec = tree.specs.get(target.code);
    const chip = layout.chip.get(target.code);
    if (spec && chip) {
      const cites = tree.index.dependencies.filter((d) => d.from === spec.code).length;
      content = {
        x: chip.x,
        y: chip.y,
        badge: spec.code,
        folder: specFolder(spec),
        label: spec.label,
        summary: spec.summary,
        rollup: rollupOfSpec(spec),
        meta: `${spec.requirements} reqs · ${spec.scenarios} scenarios · cites ${cites}`,
      };
    }
  } else if (target.kind === "index") {
    const node = tree.nodes.get(target.id);
    const box = layout.box.get(target.id);
    if (node && box) {
      const r = rollups.get(node.id) ?? EMPTY_ROLLUP;
      content = {
        x: box.x,
        y: box.y,
        badge: `${r.specs} specs`,
        folder: "ui",
        label: node.label,
        summary: node.summary,
        rollup: r,
        meta: `${r.requirements} reqs · ${r.scenarios} scenarios · ${r.gap} gaps`,
      };
    }
  }
  if (!content) return null;

  const at = placeHoverCard(content.x, content.y, view, width, height);
  return (
    <div className="smap-hover" role="tooltip" style={{ left: at.x, top: at.y }}>
      <div className="smap-hover__head">
        <span className="smap-code" data-folder={content.folder}>
          {content.badge}
        </span>
        <span className="smap-hover__label">{content.label}</span>
      </div>
      <div className="smap-hover__summary">{content.summary}</div>
      <CompletenessBar rollup={content.rollup} variant="hover" />
      <div className="smap-hover__meta">{content.meta}</div>
    </div>
  );
}
