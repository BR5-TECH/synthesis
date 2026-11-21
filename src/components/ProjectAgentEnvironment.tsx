/**
 * Project settings → Docker → **Agent environment** (SET-FR-SVBS).
 *
 * The section opens with the choice of where the agents of this project run:
 * the vendor images of SET-FR-21, or the project's dev container
 * (`DVC-devcontainer-environment.md`). The choice is kept on this machine and
 * is never committed, and it persists at once through "save agent
 * environment", outside the dirty state of SET-FR-08.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type { AgentEnvironmentSource, AgentEnvironmentState } from "../types";
import { ProjectDevcontainer, agentEnvironmentErrorMessage } from "./ProjectDevcontainer";
import { ProjectDockerImages } from "./ProjectDockerImages";

const SOURCE_CHOICES: ReadonlyArray<[AgentEnvironmentSource, string, string]> = [
  ["vendor_images", "Vendor images", "One Docker image for each agent, set below."],
  [
    "devcontainer",
    "Dev container",
    "The project's devcontainer.json defines the whole environment, and each agent runs in it.",
  ],
];

interface ProjectAgentEnvironmentProps {
  contentRoot?: string;
}

export function ProjectAgentEnvironment({ contentRoot = "" }: ProjectAgentEnvironmentProps) {
  const [state, setState] = useState<AgentEnvironmentState | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  /** The choice on screen. Dev container can show before a config is stored. */
  const [shown, setShown] = useState<AgentEnvironmentSource | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => {
    api
      .loadAgentEnvironment()
      .then((loaded) => {
        if (!live.current) return;
        setState(loaded);
        setShown(loaded.environment.source);
      })
      .catch((reason) => {
        logWarn(["frontend"], "the agent environment could not be read", {
          reason: agentEnvironmentErrorMessage(reason),
        });
        if (live.current) setLoadFailed(true);
      });
  }, []);

  /** The panel hands back the state each of its operations returns. */
  const applied = useCallback((next: AgentEnvironmentState) => {
    if (!live.current) return;
    setState(next);
    setShown(next.environment.source);
  }, []);

  const choose = async (source: AgentEnvironmentSource) => {
    if (!state || source === shown) return;
    setRefusal(null);
    const config = state.environment.devcontainerConfig;
    // SET-FR-TEZH: Dev container with no config stores nothing yet. The panel
    // offers Create devcontainer.json, which stores the choice with the file.
    if (source === "devcontainer" && !config) {
      setShown(source);
      return;
    }
    try {
      const next = await api.saveAgentEnvironment({ source, devcontainerConfig: config });
      logInfo(["frontend"], "agent environment chosen", { source });
      applied(next);
    } catch (reason) {
      const message = agentEnvironmentErrorMessage(reason);
      logWarn(["frontend"], "agent environment refused", { source, reason: message });
      if (live.current) setRefusal(message);
    }
  };

  if (loadFailed) {
    return (
      <p className="docker-alert" role="alert">
        The agent environment of this project could not be read, so nothing is shown and
        nothing was changed.
      </p>
    );
  }
  if (!state || !shown) {
    return (
      <p className="docker-state" style={{ margin: 0 }}>
        Reading this project&apos;s agent environment…
      </p>
    );
  }

  return (
    <>
      <fieldset className="agent-env" aria-label="Agent environment">
        <legend className="agent-env__legend">Agent environment</legend>
        <div className="agent-env__choices" role="radiogroup" aria-label="Agent environment">
          {SOURCE_CHOICES.map(([source, label, note]) => (
            <label className="agent-env__choice" key={source} data-selected={shown === source}>
              <input
                type="radio"
                name="agent-environment"
                value={source}
                checked={shown === source}
                onChange={() => void choose(source)}
              />
              <span className="agent-env__choice-text">
                <span className="agent-env__choice-label">{label}</span>
                <span className="agent-env__choice-note">{note}</span>
              </span>
            </label>
          ))}
        </div>
        <p className="docker-state agent-env__local">
          This choice is kept on this machine and is not committed with the project.
        </p>
        {refusal && (
          <p className="docker-alert" role="alert">
            {refusal}
          </p>
        )}
      </fieldset>
      {shown === "devcontainer" ? (
        <ProjectDevcontainer state={state} onState={applied} />
      ) : (
        <ProjectDockerImages contentRoot={contentRoot} />
      )}
    </>
  );
}
