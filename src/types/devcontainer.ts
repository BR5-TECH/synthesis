/**
 * The dev container environment of a project
 * (`DVC-devcontainer-environment.md`, "Contract surface").
 */
import type { AgenticCliVendor } from "./docker";

/** DVC-FR-EVZD: where the agentic CLI work of the project runs. */
export type AgentEnvironmentSource = "vendor_images" | "devcontainer";

/** DVC-FR-EVZD: the stored agent environment, project-local. */
export interface AgentEnvironment {
  source: AgentEnvironmentSource;
  devcontainerConfig: string | null;
}

/** DVC-FR-WZSY: what the application knows of a vendor CLI. */
export type AgentCliState = "found" | "missing" | "unknown";

export interface AgentCliRow {
  vendor: Extract<AgenticCliVendor, "claude_code" | "codex">;
  state: AgentCliState;
  knownFeature: string | null;
}

export interface AgentEnvironmentState {
  environment: AgentEnvironment;
  configExists: boolean;
  agentCli: AgentCliRow[];
}

/** DVC-FR-WXYK / DVC-FR-JVKA / DVC-FR-BCDZ: a refused change. */
export type AgentEnvironmentError =
  | { code: "config_path_invalid"; reason: "absolute" | "leaves_project" | "not_a_file" }
  | { code: "config_missing" }
  | { code: "config_invalid"; message: string; line: number | null }
  | { code: "config_exists" }
  | { code: "no_known_feature" }
  | { code: "store_failed"; message: string };

/** DVC-FR-LFQQ: the build that runs for the project. */
export interface DevcontainerBuildInFlight {
  operationId: string;
  lines: string[];
}

/** DVC-FR-RTBA: one line of a running build. */
export interface DevcontainerBuildProgress {
  operationId: string;
  message: string;
}

/** DVC-FR-RTBA: the end of a build. */
export interface DevcontainerBuildFinished {
  operationId: string;
  outcome: "succeeded" | "failed" | "cancelled";
  image: string | null;
  error: string | null;
}

/** DVC-FR-WOAM: the dev container CLI of this machine. */
export interface DevcontainerCli {
  path: string;
  state: "unverified" | "verified";
  version: string | null;
  verifiedAt: string | null;
}

/** DVC-FR-BAID: why a verification failed. */
export interface DevcontainerCliFailure {
  code: "not_found" | "not_executable" | "failed" | "timed_out";
  message: string;
}
