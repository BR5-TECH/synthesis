/**
 * Links of a rich Markdown surface that can be followed
 * (`specifications/ui/EDT-editor.md` EDT-FR-YHKG to EDT-FR-QBIE).
 *
 * The extension reads a click on a link and its gesture, and gives each link a
 * tooltip. What the destination names is `markdownLinks.ts`'s, and where a file
 * link goes is the host's (EDT-FR-QBIE).
 */
import { Extension, type Editor as TiptapEditor } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";
import { openUrl } from "@tauri-apps/plugin-opener";
import { logDebug, logWarn } from "../logging";
import { isMacPlatform } from "../platform";
import { showLinkRefused, showLinkTargetMissing } from "../state/linkToasts";
import { revealRichHeading } from "./headingReveal";
import type { LinkHost } from "./linkFollow";
import { anchorIndex, classifyLink, schemeOfLink, type LinkBase } from "./markdownLinks";

/** EDT-FR-QBIE: the base of a host that resolves no file. */
const NO_FILE: LinkBase = { fileDir: null, roots: [] };

/** A long destination, such as a `data:` address, is cut in the tooltip. */
const TOOLTIP_LIMIT = 200;

/**
 * EDT-FR-YHKG: does this click follow the link? On a surface that takes input,
 * only a click with the command modifier of the platform does. Ctrl+click on
 * macOS follows nothing.
 */
export function followsOnClick(
  event: Pick<MouseEvent, "button" | "metaKey" | "ctrlKey">,
  editable: boolean,
  mac: boolean,
): boolean {
  if (event.button !== 0) return false;
  if (mac && event.ctrlKey) return false;
  if (!editable) return true;
  return mac ? event.metaKey : event.ctrlKey;
}

/** EDT-FR-BBIF: the tooltip of a link with the destination `href`. */
export function linkTooltip(href: string, editable: boolean, mac: boolean): string {
  const shown = href.length > TOOLTIP_LIMIT ? `${href.slice(0, TOOLTIP_LIMIT - 1)}…` : href;
  if (!editable) return shown;
  return `${shown}\n${mac ? "⌘-click to follow" : "Ctrl+click to follow"}`;
}

/**
 * EDT-FR-HMJI / EDT-FR-HYVP: jump to the heading of `editor` whose anchor is
 * `fragment`. `false`, with nothing moved, when no heading matches.
 */
export function revealFragmentIn(editor: TiptapEditor, fragment: string): boolean {
  // The walk of `Editor/outline.ts` richHeadings, which this module cannot
  // import: that module imports the extension set that holds this one.
  const headings: { text: string; pos: number }[] = [];
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name === "heading") {
      headings.push({ text: node.textContent.trim(), pos });
      return false;
    }
    return !node.isTextblock;
  });
  const index = anchorIndex(
    headings.map((h) => h.text),
    fragment,
  );
  return index < 0 ? false : revealRichHeading(editor, headings[index].pos);
}

/**
 * EDT-FR-UXYY to EDT-FR-QBIE: follow the destination `href`. `reveal` is the
 * jump of the surface that holds the link. The log names the kind and the
 * scheme of the link, never its address, which can carry a token.
 */
export function followLink(
  href: string,
  host: LinkHost | null,
  reveal: (fragment: string) => boolean,
): void {
  const target = classifyLink(href, host?.base ?? NO_FILE);
  const jump = (fragment: string) => {
    if (reveal(fragment)) {
      logDebug(["frontend"], "link followed", { kind: "fragment", rule: "EDT-FR-HMJI" });
      return;
    }
    logWarn(["frontend"], "link heading not found", { rule: "EDT-FR-HYVP" });
    showLinkTargetMissing();
  };
  switch (target.kind) {
    case "none":
      return;
    case "external":
      logDebug(["frontend"], "link followed", {
        kind: "external",
        scheme: schemeOfLink(target.url),
        rule: "EDT-FR-UXYY",
      });
      void Promise.resolve()
        .then(() => openUrl(target.url))
        .catch(() => {
          logWarn(["frontend"], "link could not be opened", {
            scheme: schemeOfLink(target.url),
          });
        });
      return;
    case "fragment":
      jump(target.fragment);
      return;
    case "refused":
      logWarn(["frontend"], "link refused", {
        reason: target.reason,
        scheme: schemeOfLink(href),
        rule: "EDT-FR-AWFH",
      });
      showLinkRefused(target.reason);
      return;
    case "file":
      // EDT-FR-GEHE: a link to the surface's own file opens no tab.
      if (host?.selfPath != null && target.path === host.selfPath) {
        if (target.fragment !== null) jump(target.fragment);
        return;
      }
      if (!host?.openFile) {
        logWarn(["frontend"], "link refused", { reason: "no-file", rule: "EDT-FR-QBIE" });
        showLinkRefused("no-file");
        return;
      }
      logDebug(["frontend"], "link to a file", {
        fragment: target.fragment !== null,
        rule: "EDT-FR-GEHE",
      });
      host.openFile(target.path, target.fragment);
  }
}

/** The link element of the surface that a pointer event is on, if any. */
function linkAt(view: EditorView, target: EventTarget | null): HTMLAnchorElement | null {
  if (!(target instanceof Element)) return null;
  const anchor = target.closest("a[href]");
  return anchor instanceof HTMLAnchorElement && view.dom.contains(anchor) ? anchor : null;
}

/** EDT-FR-BBIF: a tooltip on the text of each link. */
function tooltips(doc: PMNode, editable: boolean, mac: boolean): DecorationSet {
  const decorations: Decoration[] = [];
  doc.descendants((node, pos) => {
    if (!node.isText) return true;
    const link = node.marks.find((mark) => mark.type.name === "link");
    const href = link?.attrs.href;
    if (typeof href === "string" && href !== "") {
      decorations.push(
        Decoration.inline(pos, pos + node.nodeSize, {
          title: linkTooltip(href, editable, mac),
        }),
      );
    }
    return false;
  });
  return DecorationSet.create(doc, decorations);
}

/** EDT-FR-BBIF: the class on the document root while the modifier is held. */
export const MODIFIER_HELD_CLASS = "link-modifier-held";
let modifierWatchers = 0;
let releaseModifierWatch: (() => void) | null = null;

/**
 * EDT-FR-BBIF: mark the document root while the command modifier is held, so
 * the style sheet gives a link on an editable surface the hand pointer. One
 * set of window listeners serves every surface.
 */
function watchModifier(mac: boolean): () => void {
  modifierWatchers += 1;
  if (modifierWatchers === 1 && typeof window !== "undefined") {
    const root = document.documentElement;
    const update = (event: KeyboardEvent) => {
      root.classList.toggle(MODIFIER_HELD_CLASS, mac ? event.metaKey : event.ctrlKey);
    };
    const clear = () => root.classList.remove(MODIFIER_HELD_CLASS);
    window.addEventListener("keydown", update);
    window.addEventListener("keyup", update);
    window.addEventListener("blur", clear);
    releaseModifierWatch = () => {
      window.removeEventListener("keydown", update);
      window.removeEventListener("keyup", update);
      window.removeEventListener("blur", clear);
      clear();
    };
  }
  return () => {
    modifierWatchers -= 1;
    if (modifierWatchers === 0) {
      releaseModifierWatch?.();
      releaseModifierWatch = null;
    }
  };
}

export interface LinkNavigationOptions {
  /** EDT-FR-QBIE: the host, read at each click so that it can change. */
  host: () => LinkHost | null;
}

const linkNavigationKey = new PluginKey("linkNavigation");

export const LinkNavigation = Extension.create<LinkNavigationOptions>({
  name: "linkNavigation",

  addOptions() {
    return { host: () => null };
  },

  addProseMirrorPlugins() {
    const editor = this.editor;
    const options = this.options;
    const mac = isMacPlatform();
    let cached: { doc: PMNode; editable: boolean; set: DecorationSet } | null = null;
    return [
      new Plugin({
        key: linkNavigationKey,
        view: () => ({ destroy: watchModifier(mac) }),
        props: {
          decorations: (state) => {
            // The option, not `isEditable`: the view does not exist yet when
            // the first decorations are drawn, and `setEditable` sets it.
            const editable = editor.options.editable !== false;
            if (!cached || cached.doc !== state.doc || cached.editable !== editable) {
              cached = { doc: state.doc, editable, set: tooltips(state.doc, editable, mac) };
            }
            return cached.set;
          },
          // EDT-FR-ZGKO: a click that follows a link is the link's alone. The
          // command modifier is also ProseMirror's node-select modifier, and
          // without this claim the click selects the paragraph that holds the
          // link, which the next key press then replaces.
          handleClick: (view, _pos, event) =>
            linkAt(view, event.target) !== null && followsOnClick(event, view.editable, mac),
          handleDOMEvents: {
            click: (view, event) => {
              const anchor = linkAt(view, event.target);
              if (!anchor) return false;
              // The window never goes to the address of a link (EDT-FR-UXYY).
              event.preventDefault();
              if (!followsOnClick(event, view.editable, mac)) return false;
              // A drag that selected text in the link is a selection, not a
              // click on it.
              const selection = view.dom.ownerDocument.getSelection();
              if (!view.editable && selection && !selection.isCollapsed) return false;
              const host = options.host();
              followLink(anchor.getAttribute("href") ?? "", host, (fragment) =>
                host?.revealFragment
                  ? host.revealFragment(fragment, anchor)
                  : revealFragmentIn(editor, fragment),
              );
              return true;
            },
            auxclick: (view, event) => {
              if (linkAt(view, event.target)) event.preventDefault();
              return false;
            },
          },
        },
      }),
    ];
  },
});
