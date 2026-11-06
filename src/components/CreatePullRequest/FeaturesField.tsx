/**
 * The **Features** selector of the Create a PR window
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-GDDO,
 * CPR-FR-PXQN).
 *
 * It offers the parent issues of the publication repository (per
 * `GHP-github-publication.md` GHP-FR-FTMC) from the metadata cache of the remote
 * that `"get note publication availability"` names. When the window opens it
 * asks for one refresh, and it reads the cache again on `"publication metadata
 * changed"`. It holds any number of selections. When publication is not
 * available, the selector is unavailable and states the reason.
 */
import { useEffect, useMemo, useState } from "react";

import * as api from "../../api";
import { onPublicationMetadataChanged } from "../../events";
import { logDebug, logWarn } from "../../logging";
import { githubPollingErrorMessage } from "../../state/githubPolling";
import type { PullRequestFeature } from "../../state/pullRequestSessions";
import type { PublicationMetadata, PublicationParentIssue } from "../../types";
import { FilterableSelect } from "../FilterableSelect";
import { parseRejection } from "../Git/errors";
import { Icon } from "../icons";
import { matchesParent, parentAnnouncement, parentOption } from "../PublicationChooser";

export type FeatureCatalog =
  | { status: "loading" }
  | { status: "unavailable"; reason: string }
  | {
      status: "ready";
      remoteName: string;
      owner: string;
      repository: string;
      /** Null until the cache or the refresh gives a list. */
      parents: PublicationParentIssue[] | null;
      error: string | null;
    };

function parentsOf(metadata: PublicationMetadata): PublicationParentIssue[] | null {
  return metadata.parents.state === "loaded" ? metadata.parents.items : null;
}

/** CPR-FR-GDDO / CPR-FR-PXQN: the parent issues the selector offers. */
export function useFeatureCatalog(): FeatureCatalog {
  const [catalog, setCatalog] = useState<FeatureCatalog>({ status: "loading" });
  useEffect(() => {
    let live = true;
    let stop: Promise<(() => void) | null> | null = null;
    const merge = (metadata: PublicationMetadata | null, error: string | null = null) =>
      setCatalog((c) =>
        c.status !== "ready"
          ? c
          : {
              ...c,
              parents: (metadata && parentsOf(metadata)) ?? c.parents,
              error: metadata && parentsOf(metadata) ? null : (error ?? c.error),
            },
      );
    void (async () => {
      let remoteName: string;
      try {
        const answer = await api.getNotePublicationAvailability();
        if (!live) return;
        if (!answer?.available || !answer.remoteName) {
          setCatalog({
            status: "unavailable",
            reason: answer?.reason ?? "Publication to GitHub is not available.",
          });
          return;
        }
        remoteName = answer.remoteName;
        setCatalog({
          status: "ready",
          remoteName,
          owner: answer.repositoryOwner ?? "",
          repository: answer.repositoryName ?? "",
          parents: null,
          error: null,
        });
      } catch (error) {
        logWarn(["frontend", "remote"], "pull request feature availability read failed", {
          error: parseRejection(error).code,
        });
        if (live) setCatalog({ status: "unavailable", reason: githubPollingErrorMessage(error) });
        return;
      }
      // CPR-FR-PXQN: the cache is read again on its change event while the
      // window is open.
      stop = onPublicationMetadataChanged((payload) => {
        if (!live || payload.remoteName !== remoteName) return;
        api
          .getPublicationMetadataCache(remoteName)
          .then((again) => {
            if (live && again) merge(again);
          })
          .catch((error: unknown) => {
            logWarn(["frontend", "remote"], "pull request feature cache read failed", {
              error: parseRejection(error).code,
            });
          });
      }).catch((error: unknown) => {
        logWarn(["frontend", "remote"], "pull request feature subscription failed", {
          error: parseRejection(error).code,
        });
        return null;
      });
      try {
        const cached = await api.getPublicationMetadataCache(remoteName);
        if (live && cached) merge(cached);
      } catch (error) {
        logWarn(["frontend", "remote"], "pull request feature cache read failed", {
          error: parseRejection(error).code,
        });
      }
      try {
        const answer = await api.refreshPublicationMetadata();
        logDebug(["frontend", "remote"], "pull request feature refresh ended", {
          outcome: answer?.outcome,
          errorCode: answer?.errorCode,
        });
        if (!live) return;
        const fresh = answer?.remoteName === remoteName ? answer.metadata : null;
        merge(fresh, answer?.error ?? (fresh ? null : "GitHub could not be read."));
      } catch (error) {
        logWarn(["frontend", "remote"], "pull request feature refresh failed", {
          error: parseRejection(error).code,
        });
        if (live) merge(null, githubPollingErrorMessage(error));
      }
    })();
    return () => {
      live = false;
      void stop?.then((unlisten) => unlisten?.());
    };
  }, []);
  return catalog;
}

/** The text of a feature chip: `#N title`. */
const featureLabel = (feature: PullRequestFeature) => `#${feature.number} ${feature.title}`;

export function FeaturesField({
  catalog,
  features,
  disabled,
  labelId,
  onChange,
}: {
  catalog: FeatureCatalog;
  features: readonly PullRequestFeature[];
  disabled: boolean;
  labelId: string;
  onChange: (features: PullRequestFeature[]) => void;
}) {
  const parents = catalog.status === "ready" ? catalog.parents : null;
  const free = useMemo(
    () =>
      (parents ?? []).filter((p) => !features.some((f) => f.number === p.number)),
    [parents, features],
  );
  const options = useMemo(() => free.map(parentOption), [free]);
  const matcher = useMemo(() => matchesParent(free), [free]);

  const add = (id: string) => {
    if (catalog.status !== "ready") return;
    const parent = free.find((p) => String(p.number) === id);
    if (!parent) return;
    onChange([
      ...features,
      {
        owner: catalog.owner,
        repository: catalog.repository,
        number: parent.number,
        title: parent.title,
      },
    ]);
  };

  return (
    <div className="picker-field create-pr__features" data-testid="create-pr-features">
      <span className="picker-field__label" id={labelId}>
        Features
      </span>
      {features.length > 0 && (
        <ul className="create-pr__chips" aria-labelledby={labelId}>
          {features.map((feature) => (
            <li key={`${feature.owner}/${feature.repository}#${feature.number}`} className="create-pr__chip">
              <span className="create-pr__chip-text" title={featureLabel(feature)}>
                {featureLabel(feature)}
              </span>
              <button
                type="button"
                className="btn btn--ghost btn--icon btn--sm"
                aria-label={`Remove feature #${feature.number}`}
                disabled={disabled}
                onClick={() => onChange(features.filter((f) => f !== feature))}
              >
                <Icon.X size={10} />
              </button>
            </li>
          ))}
        </ul>
      )}
      {catalog.status === "loading" && (
        <p className="t-meta create-pr__line" data-testid="create-pr-features-loading">
          Reading the features…
        </p>
      )}
      {catalog.status === "unavailable" && (
        <>
          <FilterableSelect
            options={[]}
            value=""
            onChange={() => {}}
            label="Features"
            placeholder="Features are unavailable"
            disabled
            testId="create-pr-features-select"
          />
          <p className="t-meta create-pr__line" data-testid="create-pr-features-unavailable">
            Features are unavailable: {catalog.reason}
          </p>
        </>
      )}
      {catalog.status === "ready" && parents === null && (
        <p className="t-meta create-pr__line" data-testid="create-pr-features-pending">
          {catalog.error ? `Features are unavailable: ${catalog.error}` : "Reading the features…"}
        </p>
      )}
      {catalog.status === "ready" && parents !== null && (
        <FilterableSelect
          options={options}
          value=""
          alwaysNotify
          onChange={add}
          label="Features"
          placeholder={free.length === 0 ? "No more features to choose" : "Filter features…"}
          emptyText="No matching feature"
          matches={matcher}
          announce={parentAnnouncement}
          disabled={disabled || free.length === 0}
          fitWindow
          testId="create-pr-features-select"
        />
      )}
    </div>
  );
}
