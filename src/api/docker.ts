/**
 * The container runtime the project's agentic work runs in.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AgenticCliVendor,
  DockerBackend,
  InFlightImageBuild,
  DockerBackendConfig,
  ProjectVendorImage,
  ProjectVendorImageStatus,
} from "../types";

// ---------------------------------------------------------------------------
// Docker backend (GSS-global-settings-storage.md GSS-FR-35..GSS-FR-40)
// ---------------------------------------------------------------------------

/**
 * GSS-FR-35: the Docker backend this machine reaches a container runtime
 * through. Reads the stored record; runs no executable and reaches no daemon,
 * so the Docker section renders offline and instantly.
 */
export const loadDockerBackend = () =>
  invoke<DockerBackend>("load_docker_backend");

/**
 * GSS-FR-39: persist the selected mode, endpoint, and CLI path, and answer with
 * the verification state those values carry.
 *
 * Changing any of the three returns the record to `unverified`: the success
 * that was earned belonged to the values that earned it.
 */
export const saveDockerBackend = (config: DockerBackendConfig) =>
  invoke<DockerBackend>("save_docker_backend", { config });

/**
 * GSS-FR-37: the first Docker CLI on `PATH` or in a conventional location.
 * Persists nothing and executes nothing.
 */
export const detectDockerCliBinary = () =>
  invoke<{ path: string | null }>("detect_docker_cli_binary");

/**
 * GSS-FR-38: verify the selected mode, and commit the success only where the
 * **daemon answered**.
 *
 * This is the one call in the Docker section that leaves the machine or runs
 * another program. Rejects with the selected mode's own typed failures —
 * `cli_path_empty`, `cli_not_found`, `cli_not_executable`, `not_the_docker_cli`,
 * `daemon_unreachable`, and `timed_out` in Docker CLI mode; `endpoint_empty`,
 * `endpoint_invalid`, `daemon_unreachable`, and `timed_out` in Docker Engine
 * mode — and on every one of them the stored record is exactly as it was.
 */
export const verifyDockerBackend = (config: DockerBackendConfig) =>
  invoke<DockerBackend>("verify_docker_backend", { config });

// ---------------------------------------------------------------------------

// Project Docker images (PSS-project-settings-storage.md PSS-FR-22..PSS-FR-30)
// ---------------------------------------------------------------------------

/**
 * PSS-FR-22 / PSS-FR-25: one status per agentic CLI vendor in a stable order,
 * configured or not, so a surface never has to reason about an absent record.
 */
export const loadProjectDockerImages = () =>
  invoke<ProjectVendorImageStatus[]>("load_project_docker_images");

/**
 * PSS-FR-22 / PSS-FR-24: write one vendor's entry into the committed project,
 * carrying every other vendor's entry and every other section through
 * unchanged.
 *
 * Rejects with `image_name_empty`, `dockerfile_absolute`,
 * `dockerfile_escapes_project_root`, or `dockerfile_not_a_regular_file`, and a
 * refused save writes nothing.
 */
export const saveProjectVendorImage = (
  vendor: AgenticCliVendor,
  entry: ProjectVendorImage,
) =>
  invoke<ProjectVendorImageStatus>("save_project_vendor_image", {
    vendor,
    entry,
  });

/**
 * PSS-FR-26 / PSS-FR-29: build this vendor's Dockerfile **locally**, into this
 * vendor's image, through the backend the author verified.
 *
 * It pushes to no registry, and it answers at once with the operation id the
 * two build events carry; the build itself reports through those. Rejects with
 * `dockerfile_unset`, `image_name_empty`, `build_already_running`, or
 * `docker_backend_unverified`, and a refusal starts nothing.
 */
export const buildProjectVendorImage = (vendor: AgenticCliVendor) =>
  invoke<{ operationId: string }>("build_project_vendor_image", { vendor });

/**
 * PSS-FR-29: the build holding this project's one build slot right now, or
 * `null`.
 *
 * A build outlives the surface that started it, so the Docker section reads
 * this when it mounts: that is what keeps a running build rendered against its
 * own tab and every Build control disabled on re-entry (SET-FR-25, SET-FR-26).
 */
export const loadProjectImageBuildInFlight = () =>
  invoke<InFlightImageBuild | null>("load_project_image_build_in_flight");

/**
 * PSS-FR-28: ask the named build to stop. The terminal result arrives on
 * `"project image build finished"` as `cancelled`, and the project's image
 * configuration is left exactly as it was.
 */
export const cancelProjectVendorImageBuild = (operationId: string) =>
  invoke<void>("cancel_project_vendor_image_build", { operationId });
