/**
 * The events the Delivery lens follows while the Map tab is mounted
 * (`MDL-map-delivery-lens.md` MDL-FR-YTAP, MDL-FR-LZIE).
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

import * as events from "../../events";
import type { GithubPlanSession } from "../../state/delivery/githubPlan";
import type { SpecMapSessionStore } from "../../state/specMap/session";

/** A listen that answered after the cleanup is undone at once. */
function keep(pending: Promise<() => void>, sink: Array<() => void>, cancelled: () => boolean) {
  void pending
    .then((fn) => {
      if (cancelled()) fn();
      else sink.push(fn);
    })
    .catch(() => {});
}

/**
 * MDL-FR-YTAP: the source is read again on `"delivery source changed"`,
 * `"github polling changed"`, and `"project tree changed"`. MDL-FR-LZIE: a
 * GitHub plan loads with `refresh` true on `"github polling changed"`, and
 * with `refresh` false on `"drafts changed"` and `"graduation run changed"`.
 */
export function useLensEvents(store: SpecMapSessionStore, github: GithubPlanSession | null) {
  const githubRef = useRef(github);
  githubRef.current = github;
  useEffect(() => {
    let cancelled = false;
    const off: Array<() => void> = [];
    const isCancelled = () => cancelled;
    const resolve = () => void store.resolveDeliverySource();
    // A plan loads on an event only once it has started to load. A start
    // whose loads all refused counts, so an event can bring the plan back.
    const reload = (refresh: boolean) => {
      const plan = githubRef.current;
      if (plan?.hasStarted()) void plan.load(refresh);
    };
    keep(listen(events.DELIVERY_SOURCE_CHANGED, resolve), off, isCancelled);
    keep(events.onProjectTreeChanged(resolve), off, isCancelled);
    keep(
      events.onGithubPollingChanged(() => {
        resolve();
        reload(true);
      }),
      off,
      isCancelled,
    );
    keep(events.onDraftsChanged(() => reload(false)), off, isCancelled);
    keep(events.onGraduationRunChanged(() => reload(false)), off, isCancelled);
    return () => {
      cancelled = true;
      off.forEach((fn) => fn());
    };
  }, [store]);
}
