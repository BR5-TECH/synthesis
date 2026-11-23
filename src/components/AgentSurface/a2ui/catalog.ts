/**
 * The discussion catalog as the renderer holds it (`AUI-agent-ui-surfaces.md`
 * AUI-FR-CSOS; `ASF-agent-surfaces.md` ASF-FR-HRQL, ASF-FR-VZCC).
 *
 * The official v1.0 basic catalog, derived under the application's catalog
 * id: the basic components and functions as the pinned package implements
 * them, the application's own media, dialog, and `openUrl` in place of the
 * official ones, and the extension components.
 */
import type { ReactCatalogComponent } from "@a2ui/react";
import { basicCatalog, createFunctionImplementation, OpenUrlApi, type Catalog } from "@a2ui/web_core/v1_0";

import { CARD_PARTS } from "./cards";
import { openLink } from "./links";
import { APP_PARTS } from "./parts";

/** AUI-FR-CSOS. */
export const CATALOG_ID = "synthesis:a2ui/v1";

/** The basic components that the application draws itself. */
export const REPLACED_COMPONENTS: readonly string[] = ["Button", "Image", "Video", "AudioPlayer", "Modal"];

/** ASF-FR-VZCC: `openUrl` opens through the application's opener. */
const openUrl = createFunctionImplementation(OpenUrlApi, (args) => {
  openLink(String(args.url ?? ""));
});

let built: Catalog<ReactCatalogComponent> | null = null;

/** The catalog, built once. */
export function discussionCatalog(): Catalog<ReactCatalogComponent> {
  if (built) return built;
  const basicNames = [...basicCatalog.components.keys()].filter((n) => !REPLACED_COMPONENTS.includes(n));
  const functionNames = [...basicCatalog.functions.keys()].filter((n) => n !== "openUrl");
  built = basicCatalog.derive({
    catalogId: CATALOG_ID,
    components: [...basicNames, ...APP_PARTS, ...CARD_PARTS] as never[],
    functions: [...functionNames, openUrl] as never[],
  }) as unknown as Catalog<ReactCatalogComponent>;
  return built;
}
