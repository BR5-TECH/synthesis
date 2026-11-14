import { specFolder, type TreeIndex } from "../../state/specMap/tree";
import { EMPTY_ROLLUP, rollupOfSpec, type Rollup } from "../../state/specMap/rollups";
import { isOpenSpecIndex, type MapRef } from "../../state/specMap/types";
import type { MapView } from "../../state/specMap/session";
import type { MapLayout } from "./layout";
import { placeHoverCard } from "./zoom";
import type { SpecLinks } from "../../state/delivery/specLinks";
import { CompletenessBar } from "./CompletenessBar";

interface HoverCardProps {
  tree: TreeIndex;
  layout: MapLayout;
  rollups: Map<string, Rollup>;
  target: MapRef;
  /** SDL-FR-SGQM: the delivery links, or null where none show. */
  links?: SpecLinks | null;
  view: MapView;
  width: number;
  height: number;
}

/** SMN-FR-MVWA / SMN-FR-FXRL / SMN-FR-YSKQ: the card for the hovered node. */
export function HoverCard({ tree, layout, rollups, target, links, view, width, height }: HoverCardProps) {
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
      // SDL-FR-SGQM: the meta line ends with the count of the items that link it.
      const items = links?.bySpec.get(spec.code)?.length ?? 0;
      const plannedMeta = items > 0 ? ` · ${items} ${items === 1 ? "plan item" : "plan items"}` : "";
      content = {
        x: chip.x,
        y: chip.y,
        // SMN-FR-PMZM: the badge of the node where it carries one.
        badge: spec.badge ?? spec.code,
        folder: specFolder(spec),
        label: spec.label,
        summary: spec.summary,
        rollup: rollupOfSpec(spec),
        // SMN-FR-EIDC: an OpenSpec requirement cites nothing.
        meta:
          (isOpenSpecIndex(tree.index)
            ? `${spec.scenarios} scenarios`
            : `${spec.requirements} reqs · ${spec.scenarios} scenarios · cites ${cites}`) + plannedMeta,
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
