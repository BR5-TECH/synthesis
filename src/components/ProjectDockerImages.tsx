/**
 * Project settings → **Docker** (SET-FR-21 through SET-FR-27).
 *
 * Which container image each agentic CLI vendor's work runs in for this
 * project. One tab per vendor, each holding the image reference — the image
 * name and its optional tag, written as one control because that is how Docker
 * writes them — the optional project-relative Dockerfile, a Build control, and
 * the region a build's progress and its result are rendered in.
 *
 * Every field persists at once through "save project vendor image", so the
 * section holds no dirty state and takes no part in the per-section save model
 * of SET-FR-08. The Docker **backend** is not configured here at all — that is
 * Global settings' (GLS-FR-29). This section reads whether it has verified and
 * says so at the top, in a line that opens that section rather than only naming
 * it (SET-FR-24).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../api";
import {
  onProjectImageBuildFinished,
  onProjectImageBuildProgress,
} from "../events";
import { logWarn } from "../logging";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import type {
  AgenticCliVendor,
  ImageBuildOutcome,
  ProjectVendorImageStatus,
  TreeNode,
} from "../types";

/**
 * SET-FR-21: one tab per vendor, in this order, Claude Code selected when the
 * section opens.
 */
export const DOCKER_VENDOR_TABS: [AgenticCliVendor, string][] = [
  ["claude_code", "Claude Code"],
  ["codex", "Codex"],
  ["opencode", "OpenCode"],
];

/** SET-FR-22: which of PSS-FR-24's three refusals a configured path failed. */
export function dockerfileProblemMessage(status: ProjectVendorImageStatus): string | null {
  switch (status.dockerfileProblem) {
    case "absolute_path":
      return "That path is absolute. A Dockerfile is named relative to the project.";
    case "escapes_project_root":
      return "That path leaves the project. A Dockerfile is named relative to the project.";
    case "not_a_regular_file":
      return "That path does not name a file in this project.";
    default:
      return null;
  }
}

/** SET-FR-22: the same three, as a save's typed refusal reports them. */
export function saveRefusalMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  const known: [string, string][] = [
    ["image_name_empty", "An image name is required."],
    [
      "dockerfile_absolute",
      "That path is absolute. A Dockerfile is named relative to the project.",
    ],
    [
      "dockerfile_escapes_project_root",
      "That path leaves the project. A Dockerfile is named relative to the project.",
    ],
    [
      "dockerfile_not_a_regular_file",
      "That path does not name a file in this project.",
    ],
  ];
  for (const [code, sentence] of known) {
    if (raw.includes(code)) return sentence;
  }
  return raw || "That could not be saved.";
}

/** SET-FR-25: why Build is disabled, when it is. */
export function buildRefusalMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  const known: [string, string][] = [
    ["dockerfile_unset", "Name a Dockerfile to build."],
    ["image_name_empty", "An image name is required to build."],
    [
      "build_already_running",
      "This project is already building an image. One at a time.",
    ],
    [
      "docker_backend_unverified",
      "Verify the Docker backend in Global settings first.",
    ],
  ];
  for (const [code, sentence] of known) {
    if (raw.includes(code)) return sentence;
  }
  return raw || "The build could not be started.";
}

/**
 * SET-FR-23: what this tab's configuration is worth, in the states
 * "load project docker images" reports (PSS-FR-25).
 *
 * An **absent** optional Dockerfile is stated as configuring no build rather
 * than as a problem; a configured one that is **invalid** names the problem and
 * offers the field to correct it. OpenCode states that execution support does
 * not exist yet rather than that its configuration is wrong, because its entry
 * is stored and may be perfectly valid.
 */
export function configurationSentence(status: ProjectVendorImageStatus): string {
  if (status.graduationState === "execution_unsupported") {
    return "Synthesis cannot run OpenCode yet. This image is stored with the project and nothing runs it until execution support exists.";
  }
  if (status.configuration === "unset") {
    return "Nothing is configured for this agent yet.";
  }
  if (status.dockerfileState === "invalid") {
    return `Configured in this project as ${status.imageReference ?? "an image with no name"}, and its Dockerfile cannot be used here.`;
  }
  if (status.dockerfileState === "valid") {
    // The image reference and the Dockerfile are both in the fields directly
    // above this line, so repeating them here only costs the section height it
    // does not have (SWN-FR-03). What the fields do *not* say is where the
    // configuration lives, so that is all this states.
    return `Committed with the project as ${status.imageReference}.`;
  }
  if (status.imageReference) {
    return `Configured in this project as ${status.imageReference}. No Dockerfile is set, so nothing is built here.`;
  }
  return "Configured in this project, and it still needs an image name.";
}

/** SET-FR-22: every Dockerfile-looking file in the project, for the picker. */
export function dockerfileCandidates(root: TreeNode | null): string[] {
  if (!root) return [];
  const found: string[] = [];
  const walk = (node: TreeNode) => {
    if (node.nodeKind === "file") {
      const name = node.name.toLowerCase();
      if (name === "dockerfile" || name.endsWith(".dockerfile") || name.startsWith("dockerfile.")) {
        found.push(node.path);
      }
    }
    for (const child of node.children ?? []) walk(child);
  };
  walk(root);
  return found.sort();
}

/**
 * SET-FR-26: how many output lines one tab keeps.
 *
 * A Docker build reports a line per step and a line per layer, and a long one
 * outruns any window. The oldest lines are dropped rather than the newest,
 * because the end of a build is where its failure is written.
 */
export const BUILD_LOG_LIMIT = 400;

/** What one tab is showing about a build of its own. */
interface BuildReport {
  operationId: string;
  running: boolean;
  completed?: number;
  total?: number;
  /**
   * SET-FR-26: every update this build has reported, in arrival order. They
   * accumulate rather than replace, so the author reads what the build did and
   * not only what it is doing this instant.
   */
  lines: string[];
  outcome?: ImageBuildOutcome;
  imageReference?: string;
  diagnostic?: string;
}

/** One more output line, with the oldest dropped once the log is full. */
function appended(lines: string[], line: string): string[] {
  const next = [...lines, line];
  return next.length > BUILD_LOG_LIMIT
    ? next.slice(next.length - BUILD_LOG_LIMIT)
    : next;
}

export function ProjectDockerImages() {
  const [statuses, setStatuses] = useState<ProjectVendorImageStatus[] | null>(
    null,
  );
  const [selected, setSelected] = useState<AgenticCliVendor>("claude_code");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [fieldError, setFieldError] = useState<Partial<
    Record<AgenticCliVendor, string>
  > | null>(null);
  const [backendVerified, setBackendVerified] = useState<boolean | null>(null);
  const [resolvedVendor, setResolvedVendor] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<string[]>([]);
  // SET-FR-26: a build is rendered against the tab that started it, and a build
  // still running when the section is left and returned to is still rendered
  // there.
  const [builds, setBuilds] = useState<Partial<Record<AgenticCliVendor, BuildReport>>>(
    {},
  );
  // The draft each field holds while the author is typing in it. The committed
  // status is what a tab renders from; this is only what is being typed.
  const [drafts, setDrafts] = useState<
    Partial<Record<AgenticCliVendor, { imageName: string; tag: string; dockerfile: string }>>
  >({});
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // SET-FR-21: the section reads every tab from "load project docker images"
  // when it mounts, so a tab renders the committed project's own entry rather
  // than anything this window holds.
  const reload = useCallback(async () => {
    try {
      const loaded = await api.loadProjectDockerImages();
      if (!mounted.current) return;
      setStatuses(loaded);
      setLoadError(null);
    } catch (e) {
      if (!mounted.current) return;
      const message = e instanceof Error ? e.message : String(e);
      setLoadError(message);
      logWarn(["frontend"], "project docker images could not be read", {
        error: message,
      });
    }
  }, []);

  useEffect(() => {
    void reload();
    // SET-FR-24: which vendor the project resolves to for graduation, read from
    // the same operation the Project section names (SET-FR-13).
    void api
      .getProjectAgenticIntegration()
      .then((resolved) => {
        if (mounted.current) setResolvedVendor(resolved.vendor ?? null);
      })
      .catch(() => {
        // A resolution this surface cannot read marks no tab rather than
        // marking the wrong one.
      });
    // SET-FR-24 / SET-FR-25: whether the machine's Docker backend has verified.
    void api
      .loadDockerBackend()
      .then((backend) => {
        if (mounted.current) setBackendVerified(backend.state === "verified");
      })
      .catch(() => {
        if (mounted.current) setBackendVerified(false);
      });
    // SET-FR-25 / SET-FR-26: a build outlives this section. Reading the one the
    // project has in flight is what keeps it rendered against its own tab, and
    // every Build control disabled, when the author leaves the section and
    // comes back to it (PSS-FR-29).
    void api
      .loadProjectImageBuildInFlight()
      .then((running) => {
        if (!mounted.current || !running) return;
        setBuilds((current) => {
          // A build this section already knows about — one it started itself,
          // or one an event has already reported — is left exactly as it is,
          // so re-reading never discards progress that has already arrived.
          if (current[running.vendor]?.operationId === running.operationId) {
            return current;
          }
          return {
            ...current,
            [running.vendor]: {
              operationId: running.operationId,
              running: true,
              lines: ["This build was started earlier and is still running."],
            },
          };
        });
      })
      .catch(() => {
        // A registry this surface cannot read leaves the controls enabled,
        // which the backend's own single-build refusal then answers rather
        // than a second build starting.
      });
    // SET-FR-22: the Dockerfiles this project holds, for the picker.
    void api
      .loadProjectTree()
      .then((tree) => {
        if (mounted.current) setCandidates(dockerfileCandidates(tree));
      })
      .catch(() => {
        // A tree this surface cannot read leaves the field to be typed into,
        // which is the other half of SET-FR-22 rather than a failure.
      });
  }, [reload]);

  // SET-FR-26: the progress and the outcome come from the backend's own events.
  useEffect(() => {
    const subscriptions = [
      onProjectImageBuildProgress((event) => {
        if (!mounted.current) return;
        setBuilds((current) => ({
          ...current,
          [event.vendor]: {
            operationId: event.operationId,
            running: true,
            completed: event.completed,
            total: event.total,
            lines: appended(
              current[event.vendor]?.lines ?? [],
              `${event.phase}: ${event.message}`,
            ),
          },
        }));
      }),
      onProjectImageBuildFinished((event) => {
        if (!mounted.current) return;
        setBuilds((current) => ({
          ...current,
          [event.vendor]: {
            ...(current[event.vendor] ?? {
              operationId: event.operationId,
              lines: [],
            }),
            operationId: event.operationId,
            running: false,
            outcome: event.outcome,
            imageReference: event.imageReference,
            diagnostic: event.diagnostic,
          },
        }));
      }),
    ];
    return () => {
      for (const subscription of subscriptions) {
        void subscription.then((unlisten) => unlisten());
      }
    };
  }, []);

  /** SET-FR-25: a project builds one image at a time, so **every** tab's Build
   * is disabled while any build runs — whichever tab started it. */
  const aBuildIsRunning = useMemo(
    () => Object.values(builds).some((build) => build?.running),
    [builds],
  );

  /**
   * SET-FR-26: the output region follows the build.
   *
   * A log that accumulates is only readable while it holds its tail, so each new
   * line scrolls the region to the bottom. Declared here, above the two early
   * returns below, because a hook cannot be reached conditionally.
   */
  const logRef = useRef<HTMLDivElement | null>(null);
  const shownLines = builds[selected]?.lines.length ?? 0;
  useEffect(() => {
    const node = logRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [shownLines, selected]);

  const status = statuses?.find((entry) => entry.vendor === selected) ?? null;

  const draftFor = (entry: ProjectVendorImageStatus) =>
    drafts[entry.vendor] ?? {
      imageName: entry.imageName,
      tag: entry.tag ?? "",
      dockerfile: entry.dockerfile ?? "",
    };

  const setDraft = (
    vendor: AgenticCliVendor,
    patch: Partial<{ imageName: string; tag: string; dockerfile: string }>,
  ) =>
    setDrafts((current) => {
      const entry = statuses?.find((s) => s.vendor === vendor);
      const base =
        current[vendor] ??
        (entry
          ? {
              imageName: entry.imageName,
              tag: entry.tag ?? "",
              dockerfile: entry.dockerfile ?? "",
            }
          : { imageName: "", tag: "", dockerfile: "" });
      return { ...current, [vendor]: { ...base, ...patch } };
    });

  /**
   * SET-FR-24: editing a field persists at once, so the section holds no dirty
   * state. A refused save writes nothing and says which of the three the path
   * was (SET-FR-22).
   */
  const persist = async (vendor: AgenticCliVendor) => {
    const draft = drafts[vendor];
    if (!draft) return;
    setFieldError((current) => ({ ...current, [vendor]: undefined }));
    try {
      const saved = await api.saveProjectVendorImage(vendor, {
        imageName: draft.imageName,
        tag: draft.tag.trim() === "" ? null : draft.tag,
        dockerfile: draft.dockerfile.trim() === "" ? null : draft.dockerfile,
      });
      if (!mounted.current) return;
      setStatuses((current) =>
        (current ?? []).map((entry) =>
          entry.vendor === vendor ? saved : entry,
        ),
      );
      setDrafts((current) => ({ ...current, [vendor]: undefined }));
    } catch (e) {
      if (!mounted.current) return;
      setFieldError((current) => ({
        ...current,
        [vendor]: saveRefusalMessage(e),
      }));
    }
  };

  /**
   * SET-FR-25 / SET-FR-27: Build, and the Retry that is one more of it. Nothing
   * retries on its own.
   */
  const build = async (vendor: AgenticCliVendor) => {
    setFieldError((current) => ({ ...current, [vendor]: undefined }));
    setBuilds((current) => ({
      ...current,
      [vendor]: {
        operationId: "",
        running: true,
        lines: ["Starting the build…"],
      },
    }));
    try {
      const { operationId } = await api.buildProjectVendorImage(vendor);
      if (!mounted.current) return;
      setBuilds((current) => ({
        ...current,
        [vendor]: { ...current[vendor]!, operationId },
      }));
    } catch (e) {
      if (!mounted.current) return;
      // A refused build started nothing, so nothing is left running.
      setBuilds((current) => ({ ...current, [vendor]: undefined }));
      setFieldError((current) => ({
        ...current,
        [vendor]: buildRefusalMessage(e),
      }));
    }
  };

  /**
   * PSS-FR-28: ask the running build to stop. The terminal result arrives on
   * the finished channel as `cancelled`, so nothing here decides the outcome —
   * and the project's image configuration is left exactly as it was.
   */
  const cancel = async (operationId: string) => {
    try {
      await api.cancelProjectVendorImageBuild(operationId);
    } catch (e) {
      // A cancellation the backend would not take leaves the build running,
      // which is what the progress region already says.
      logWarn(["frontend"], "project image build cancellation refused", {
        operationId,
        error: e instanceof Error ? e.message : String(e),
      });
    }
  };

  /**
   * SET-FR-24: where the backend is set up, as something the author can follow.
   *
   * Naming a section they then have to find themselves is a pointer; opening it
   * is an answer. The two settings windows are one window at a time (SWN-FR-05,
   * SWN-FR-07), so this replaces the window it is activated from — which is the
   * documented behaviour of every other route into a settings section.
   */
  const openBackendSettings = () => {
    void api.openSettingsWindow("global", "docker").catch((e) => {
      logWarn(["frontend"], "global docker settings could not be opened", {
        error: e instanceof Error ? e.message : String(e),
      });
    });
  };

  if (loadError) {
    return (
      <p className="docker-alert" role="alert">
        This project&apos;s configuration could not be read, so nothing is shown
        and nothing was changed.
      </p>
    );
  }
  if (!statuses || !status) {
    return (
      <p className="docker-state" style={{ margin: 0 }}>
        Reading this project&apos;s Docker settings…
      </p>
    );
  }

  const draft = draftFor(status);
  const report = builds[status.vendor];
  const dockerfileProblem = dockerfileProblemMessage(status);
  const needsThisTab = resolvedVendor === status.vendor;
  const buildable =
    draft.dockerfile.trim() !== "" &&
    draft.imageName.trim() !== "" &&
    backendVerified === true;

  return (
    <div className="docker-images">
      {/* SET-FR-24: the one statement about the backend, at the top because it
          governs everything under it — a Build control the section cannot
          enable is explained here before the author reaches it. */}
      <div className="docker-backend">
        <span
          className="docker-backend__state"
          data-state={
            backendVerified === null
              ? "reading"
              : backendVerified
                ? "verified"
                : "unverified"
          }
        >
          {backendVerified === null
            ? "Reading the Docker backend…"
            : backendVerified
              ? "Docker backend verified"
              : "Docker backend not verified"}
        </span>
        <button
          type="button"
          className="docker-backend__link"
          onClick={openBackendSettings}
        >
          Global settings → Docker
        </button>
      </div>

      <div
        role="tablist"
        aria-label="Agent"
        style={{ ...SETTINGS_TABLIST_STYLE, marginBottom: 14 }}
      >
        {DOCKER_VENDOR_TABS.map(([vendor, label]) => (
          <button
            key={vendor}
            type="button"
            role="tab"
            aria-selected={selected === vendor}
            className="btn btn--ghost btn--sm docker-tab"
            data-active={selected === vendor}
            /* SET-FR-24: the tab graduation needs wears a mark rather than a
               word, so the strip stays three labels wide and the tab's own
               accessible name stays the vendor's name. */
            data-graduates={resolvedVendor === vendor}
            style={settingsTabStyle(selected === vendor)}
            onClick={() => setSelected(vendor)}
          >
            {label}
          </button>
        ))}
      </div>

      <div role="tabpanel" aria-label={vendorLabel(status.vendor)}>
        {needsThisTab && (
          <p className="docker-graduates">
            {status.imageName.trim() === ""
              ? "This project graduates through this agent, so graduation cannot start until an image name is filled in here."
              : "This project graduates through this agent."}
          </p>
        )}

        {/* SET-FR-21: the image reference, written the way Docker writes it.
            Two fields in one frame separated by the colon that joins them, so
            the tag reads as part of the reference rather than as a second
            unrelated setting — and takes the width a tag actually needs. */}
        <div className="docker-field">
          <span className="picker-field__label" id="docker-image-label">
            Image
          </span>
          <div
            className="joined-field"
            role="group"
            aria-labelledby="docker-image-label"
          >
            <input
              id="docker-image-name"
              type="text"
              aria-label="Image name"
              className="joined-field__part"
              spellCheck={false}
              autoComplete="off"
              value={draft.imageName}
              placeholder="registry.example/agent"
              onChange={(e) =>
                setDraft(status.vendor, { imageName: e.target.value })
              }
              onBlur={() => void persist(status.vendor)}
            />
            <span className="joined-field__sep" aria-hidden="true">
              :
            </span>
            <input
              id="docker-image-tag"
              type="text"
              aria-label="Tag (optional)"
              className="joined-field__part joined-field__part--tag"
              spellCheck={false}
              autoComplete="off"
              value={draft.tag}
              placeholder="latest"
              onChange={(e) => setDraft(status.vendor, { tag: e.target.value })}
              onBlur={() => void persist(status.vendor)}
            />
          </div>
        </div>

        {/* SET-FR-23: what this tab's configuration is worth. One line, under
            the fields it qualifies. */}
        <p className="docker-state">{configurationSentence(status)}</p>

        <div className="docker-field">
          <label className="picker-field__label" htmlFor="docker-dockerfile">
            Dockerfile <span className="docker-field__optional">optional</span>
          </label>
          <input
            id="docker-dockerfile"
            type="text"
            aria-label="Dockerfile (optional)"
            className="input input--mono input--sm"
            list="docker-dockerfile-candidates"
            spellCheck={false}
            autoComplete="off"
            value={draft.dockerfile}
            placeholder="docker/agent.Dockerfile"
            onChange={(e) =>
              setDraft(status.vendor, { dockerfile: e.target.value })
            }
            onBlur={() => void persist(status.vendor)}
          />
          <datalist id="docker-dockerfile-candidates">
            {candidates.map((path) => (
              <option key={path} value={path} />
            ))}
          </datalist>
        </div>

        {dockerfileProblem && (
          <p className="docker-alert" role="alert">
            {dockerfileProblem}
          </p>
        )}
        {fieldError?.[status.vendor] && (
          <p className="docker-alert" role="alert">
            {fieldError[status.vendor]}
          </p>
        )}

        <div className="docker-actions">
          <button
            type="button"
            className="btn btn--default btn--sm"
            onClick={() => void build(status.vendor)}
            disabled={!buildable || aBuildIsRunning}
          >
            {report?.outcome === "failed" ? "Retry" : "Build"}
          </button>
          {/* PSS-FR-28: a cancellation is explicit. It is offered only once the
              build has an operation id to name, because the backend cancels by
              id and there is nothing to ask for before the start has
              answered. */}
          {report?.running && report.operationId !== "" && (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={() => void cancel(report.operationId)}
            >
              Cancel
            </button>
          )}
          {/* SET-FR-25: while it is disabled, the tab states which of the three
              things it needs is missing. */}
          {!buildable && (
            <span className="docker-actions__hint">
              {draft.dockerfile.trim() === ""
                ? "Name a Dockerfile to build one."
                : draft.imageName.trim() === ""
                  ? "An image name is required: it is what the build produces."
                  : "Verify the Docker backend in Global settings first."}
            </span>
          )}
        </div>

        {/* SET-FR-26: the build's progress, its output, and its outcome, inline,
            on the tab that started it. The whole region is the live region, so
            the terminal result is announced once; the output inside it is not,
            because a build that announces every layer it pulls is unusable. */}
        {report && (
          <div className="docker-build" role="status">
            {report.running && (
              <progress
                className="docker-build__bar"
                aria-label="Build progress"
                {...(report.total !== undefined
                  ? { value: report.completed ?? 0, max: report.total }
                  : {})}
              />
            )}
            {/* The outcome stands above the output it came out of: once a
                build has ended, what it did is the answer and the log under it
                is the evidence. */}
            {!report.running && (
              <p className="docker-build__result" data-outcome={report.outcome}>
                {report.outcome === "succeeded"
                  ? `Built ${report.imageReference}.`
                  : report.outcome === "cancelled"
                    ? "The build was cancelled. Nothing here changed."
                    : (report.diagnostic ?? "The build failed.")}
              </p>
            )}
            {report.lines.length > 0 && (
              <div
                className="docker-build__log"
                ref={logRef}
                role="log"
                aria-live="off"
                aria-label="Build output"
                tabIndex={0}
              >
                {report.lines.map((line, index) => (
                  <div className="docker-build__line" key={index}>
                    {line}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** A vendor's tab label, for the panel that tab presents. */
function vendorLabel(vendor: AgenticCliVendor): string {
  return DOCKER_VENDOR_TABS.find(([id]) => id === vendor)?.[1] ?? vendor;
}
