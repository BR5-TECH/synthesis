/**
 * The Rich rendering of one file of a change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-NOTT,
 * OCW-FR-FMDZ).
 *
 * It is the read-only Markdown of `DTV-document-text-viewer.md` DTV-FR-SSQI,
 * with the image rule of a change. An image whose destination is a relative
 * path inside the change folder renders the bytes `"read change image"`
 * returns for it on a stream location (OSM-FR-BLEY). A read that refuses,
 * and every other destination, render the alt text and the destination as a
 * link that the application does not fetch.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";

import { readChangeImage } from "../../api/openspec";
import { logDebug } from "../../logging";
import { DocumentImageContext, DocumentMarkdown } from "../DocumentViewer/DocumentMarkdown";

/** The change whose working copy an image is read from. */
export interface ChangeImageSource {
  streamId: string;
  change: string;
}

/**
 * OCW-FR-FMDZ: whether a Markdown destination is a relative path that stays
 * inside the change folder. `fileDir` is the change-relative folder of the
 * file that holds the image.
 */
export function insideChangeFolder(src: string, fileDir: string): boolean {
  return changePathOf(src, fileDir) !== null;
}

/**
 * OCW-FR-FMDZ: the change-relative path a destination names, or `null` where
 * it is not a relative path that stays inside the change folder.
 */
export function changePathOf(src: string, fileDir: string): string | null {
  const destination = src.trim().split(/[?#]/)[0];
  if (destination === "" || destination.startsWith("/") || destination.startsWith("\\")) return null;
  // A scheme (`https:`, `data:`, `file:`) or a drive letter is not relative.
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(destination)) return null;
  const parts = fileDir === "" ? [] : fileDir.split("/");
  for (const part of destination.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.length > 0 ? parts.join("/") : null;
}

/** What a long destination shows, so a `data:` source does not fill the page. */
function shownDestination(src: string): string {
  return src.length > 80 ? `${src.slice(0, 77)}…` : src;
}

function ImageFallback({ alt, src, inside }: { alt: string; src: string; inside: boolean }) {
  return (
    <span
      className="openspec-image"
      data-testid="openspec-image"
      data-inside={inside ? "true" : undefined}
    >
      <span className="openspec-image__alt" role="img" aria-label={alt || "Image"}>
        {alt || "Image"}
      </span>{" "}
      {/* DTV-FR-SSQI: inert, with no href, so nothing navigates or fetches. */}
      <span className="doc-link openspec-image__link" title={src}>
        {shownDestination(src)}
      </span>
    </span>
  );
}

/**
 * OCW-FR-FMDZ: an image inside the change folder, read from the stream's
 * working copy. The fallback stands while the read runs and where it refuses.
 */
function ChangeImage({
  alt,
  src,
  path,
  source,
}: {
  alt: string;
  src: string;
  path: string;
  source: ChangeImageSource;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setUrl(null);
    readChangeImage(source.streamId, source.change, path).then(
      (image) => {
        if (live) setUrl(`data:${image.mediaType};base64,${image.data}`);
      },
      (error: unknown) => {
        // The path is change-relative and names no content of the file.
        logDebug(["frontend"], "change image not read", {
          change: source.change,
          path,
          reason: String(error),
        });
      },
    );
    return () => {
      live = false;
    };
  }, [source.streamId, source.change, path]);
  if (url === null) return <ImageFallback alt={alt} src={src} inside />;
  return <img className="openspec-image__img" src={url} alt={alt} data-testid="openspec-image-shown" />;
}

/** OCW-FR-FMDZ: the image rule of a change, for every Markdown below it. */
export function ChangeImages({
  dir,
  source,
  children,
}: {
  dir: string;
  /** Present on a stream location alone; without it every image falls back. */
  source?: ChangeImageSource;
  children: ReactNode;
}) {
  const streamId = source?.streamId;
  const change = source?.change;
  const render = useMemo(
    () => (alt: string, src: string) => {
      const path = changePathOf(src, dir);
      if (path !== null && streamId !== undefined && change !== undefined) {
        return <ChangeImage alt={alt} src={src} path={path} source={{ streamId, change }} />;
      }
      return <ImageFallback alt={alt} src={src} inside={path !== null} />;
    },
    [dir, streamId, change],
  );
  return (
    <DocumentImageContext.Provider value={render}>
      {children}
    </DocumentImageContext.Provider>
  );
}

export function ChangeMarkdown({
  text,
  path,
  source,
}: {
  text: string;
  path: string;
  source?: ChangeImageSource;
}) {
  const dir = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
  return (
    <ChangeImages dir={dir} source={source}>
      <DocumentMarkdown text={text} />
    </ChangeImages>
  );
}
