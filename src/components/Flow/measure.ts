/**
 * MDL-FR-ECPZ / FLO-FR-PLUZ / FLO-FR-HBBF: what a container's view measures of
 * itself, so the automatic layout gives it the room that its binding's content
 * needs. A GitHub plan adds a Status chip and a tracker tag to the header, and
 * an item can show an error line and its references above its members.
 */
import type { ContainerMeasure } from "./useFlowLayout";

/** The part of a header's width a long name takes before it truncates. */
export const HEAD_NAME_MAX = 200;

const px = (value: string | null | undefined): number => {
  const n = Number.parseFloat(value ?? "");
  return Number.isFinite(n) ? n : 0;
};

/**
 * The natural width of the header `head`, and the bottom of the content above
 * the members of the container `root`. jsdom lays nothing out, so there both
 * are 0, which the layout reads as not measured.
 *
 * MDL-FR-ECPZ: a width that is too small by one pixel truncates the name, so
 * each part is measured to the fraction of a pixel and rounded up, and the
 * border of the container counts too. The canvas can be zoomed, so a
 * rectangle on the screen is divided by the zoom of the header.
 */
export function measureContainer(root: HTMLElement, head: HTMLElement): ContainerMeasure {
  const style = getComputedStyle(head);
  const rootStyle = getComputedStyle(root);
  const headRect = head.getBoundingClientRect().width;
  const zoom = head.offsetWidth > 0 && headRect > 0 ? headRect / head.offsetWidth : 1;
  /** The width of `el` on the canvas, to the fraction of a pixel. */
  const widthOf = (el: HTMLElement) => {
    const rect = el.getBoundingClientRect().width / zoom;
    return Math.max(el.offsetWidth, rect > 0 ? rect : 0);
  };
  const gap = px(style.columnGap);
  let headWidth =
    px(style.paddingLeft) +
    px(style.paddingRight) +
    px(rootStyle.borderLeftWidth) +
    px(rootStyle.borderRightWidth);
  let shown = 0;
  for (const child of Array.from(head.children) as HTMLElement[]) {
    const cs = getComputedStyle(child);
    if (cs.position === "absolute" || cs.display === "none") continue;
    if (shown > 0) headWidth += gap;
    shown += 1;
    if (child.classList.contains("flow-ctr__spacer")) {
      headWidth += px(cs.minWidth);
    } else if (child.classList.contains("flow-ctr__name")) {
      // `scrollWidth` is a whole number that can be rounded down.
      headWidth += Math.min(Math.max(child.scrollWidth + 1, Math.ceil(widthOf(child))), HEAD_NAME_MAX);
    } else {
      headWidth += Math.ceil(widthOf(child)) + px(cs.marginLeft) + px(cs.marginRight);
    }
  }
  if (shown === 0 || head.offsetWidth === 0) headWidth = 0;
  let memberTop = 0;
  for (const child of Array.from(root.children) as HTMLElement[]) {
    if (getComputedStyle(child).position === "absolute") continue;
    memberTop = Math.max(memberTop, child.offsetTop + child.offsetHeight);
  }
  return { headWidth: Math.ceil(headWidth), memberTop: Math.ceil(memberTop) };
}
