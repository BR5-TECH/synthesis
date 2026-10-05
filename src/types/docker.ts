// Docker (GSS-global-settings-storage.md / PSS-project-settings-storage.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

// ---------------------------------------------------------------------------
// Docker (GSS-global-settings-storage.md GSS-FR-35..GSS-FR-40 /
//         PSS-project-settings-storage.md PSS-FR-22..PSS-FR-30)
// ---------------------------------------------------------------------------

/** GSS-FR-35: which of the two ways to reach Docker is selected. */
export type DockerBackendMode = "bollard" | "docker_cli";

/**
 * GSS-FR-36: the platform-neutral endpoint, in one of four forms — the
 * platform's automatic default, a Unix socket path, a Windows named pipe, and a
 * TCP URL.
 */
export type DockerEndpoint =
  | "automatic"
  | { unix_socket: string }
  | { windows_pipe: string }
  | { tcp: string };

/** Which form an endpoint holds, for a control that renders one field per form. */
export type DockerEndpointKind =
  | "automatic"
  | "unix_socket"
  | "windows_pipe"
  | "tcp";

/** GSS-FR-36: what the author selected, as `save docker backend` takes it. */
export interface DockerBackendConfig {
  mode: DockerBackendMode;
  endpoint: DockerEndpoint;
  cliPath: string | null;
}

/** GSS-FR-39: whether the selection standing in the store has verified. */
export type DockerBackendState = "unverified" | "verified";

/** What `load docker backend` and `verify docker backend` return. */
export interface DockerBackend extends DockerBackendConfig {
  state: DockerBackendState;
  /** The Docker server version the daemon answered with, on a success. */
  serverVersion: string | null;
  verifiedAt: string | null;
}

/** PSS-FR-22: the three agentic CLI vendors an image entry is configured for. */
export type AgenticCliVendor = "claude_code" | "codex" | "opencode";

/** PSS-FR-22: one vendor's committed entry, as `save project vendor image` takes it. */
export interface ProjectVendorImage {
  imageName: string;
  /** PSS-FR-23: optional. Omitted, Docker applies its own default tag. */
  tag?: string | null;
  /** PSS-FR-24: optional, and project-relative. */
  dockerfile?: string | null;
}

/** PSS-FR-25: whether the committed project holds an entry for this vendor. */
export type VendorImageConfiguration = "unset" | "configured";

/**
 * PSS-FR-25: an **absent** optional Dockerfile, told apart from a configured one
 * that is **invalid** for this project.
 */
export type DockerfileState = "absent" | "valid" | "invalid";

/** PSS-FR-25: which of PSS-FR-24's three refusals a configured Dockerfile fails. */
export type DockerfileProblem =
  | "absolute_path"
  | "escapes_project_root"
  | "not_a_regular_file";

/** PSS-FR-25: what this vendor's entry is worth to a graduation. */
export type VendorGraduationState =
  | "usable"
  | "image_name_missing"
  | "dockerfile_invalid"
  | "execution_unsupported";

/** PSS-FR-25: one vendor's entry and what it is worth. */
export interface ProjectVendorImageStatus {
  vendor: AgenticCliVendor;
  configuration: VendorImageConfiguration;
  imageName: string;
  tag: string | null;
  imageReference: string | null;
  dockerfile: string | null;
  dockerfileState: DockerfileState;
  dockerfileProblem: DockerfileProblem | null;
  graduationState: VendorGraduationState;
}

/**
 * PSS-FR-29: the build holding the project's one build slot right now.
 *
 * A build outlives the surface that started it, so the Docker section reads
 * this on mount rather than forgetting one is running (SET-FR-25, SET-FR-26).
 */
export interface InFlightImageBuild {
  vendor: AgenticCliVendor;
  operationId: string;
}

/** PSS-FR-27: one update while a build runs. */
export interface ImageBuildProgress {
  vendor: AgenticCliVendor;
  operationId: string;
  phase: string;
  completed?: number;
  total?: number;
  message: string;
}

/** PSS-FR-28: the one terminal result a build has. */
export type ImageBuildOutcome = "succeeded" | "failed" | "cancelled";

export interface ImageBuildFinished {
  vendor: AgenticCliVendor;
  operationId: string;
  outcome: ImageBuildOutcome;
  /** Present on a success: the reference that was built. */
  imageReference?: string;
  /** Present on a failure: a safe, user-displayable account of what went wrong. */
  diagnostic?: string;
}
