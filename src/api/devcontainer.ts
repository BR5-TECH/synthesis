/**
 * The dev container environment of a project
 * (`DVC-devcontainer-environment.md`).
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "./invoke";
import type {
  AgentEnvironment,
  AgentEnvironmentState,
  DevcontainerBuildInFlight,
  DevcontainerCli,
} from "../types";

/** DVC-FR-WZSY: the agent environment, the config state and the CLI rows. */
export const loadAgentEnvironment = () =>
  invoke<AgentEnvironmentState>("load_agent_environment");

/** DVC-FR-WXYK: store the agent environment. Rejects with an `AgentEnvironmentError`. */
export const saveAgentEnvironment = (environment: AgentEnvironment) =>
  invoke<AgentEnvironmentState>("save_agent_environment", { environment });

/** DVC-FR-BGVU: the project-relative paths of the configs, in order. */
export const listDevcontainerConfigs = () => invoke<string[]>("list_devcontainer_configs");

/** DVC-FR-BCDZ: write the starter config and select it. */
export const createDevcontainerConfig = () =>
  invoke<AgentEnvironmentState>("create_devcontainer_config");

/** DVC-FR-ABQZ: add the vendor's known Feature to the selected config. */
export const addDevcontainerAgentFeature = (vendor: string) =>
  invoke<AgentEnvironmentState>("add_devcontainer_agent_feature", { vendor });

/** DVC-FR-OXCZ: start a local build. Rejects with a refusal code. */
export const buildDevcontainer = (noCache: boolean) =>
  invoke<{ operationId: string }>("build_devcontainer", { noCache });

/** DVC-FR-RTBA: ask the named build to stop. */
export const cancelDevcontainerBuild = (operationId: string) =>
  invoke<void>("cancel_devcontainer_build", { operationId });

/** DVC-FR-LFQQ: the build that runs for the project, or `null`. */
export const loadDevcontainerBuildInFlight = () =>
  invoke<DevcontainerBuildInFlight | null>("load_devcontainer_build_in_flight");

/** DVC-FR-WOAM: the dev container CLI of this machine. */
export const loadDevcontainerCli = () => invoke<DevcontainerCli>("load_devcontainer_cli");

/** DVC-FR-XZCD: store a path. A changed path is unverified. */
export const saveDevcontainerCli = (path: string) =>
  invoke<DevcontainerCli>("save_devcontainer_cli", { path });

/** DVC-FR-BAID: run `--version`. Rejects with a `DevcontainerCliFailure`. */
export const verifyDevcontainerCli = () => invoke<DevcontainerCli>("verify_devcontainer_cli");

/** DVC-FR-XZCD: the first `devcontainer` executable found, or `null`. */
export const detectDevcontainerCli = () =>
  invoke<{ path: string | null }>("detect_devcontainer_cli");
