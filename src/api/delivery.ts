/**
 * The delivery operations: the delivery source and its settings
 * (`DSR-delivery-source.md`), the plan held on GitHub
 * (`GDP-github-delivery-plan.md`), and the per-machine layout
 * (`DMS-delivery-machine-state.md`).
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  DeliveryLayout,
  DeliveryMilestones,
  DeliverySettings,
  DeliverySource,
  DeliveryExportResult,
  GithubDeliveryEdit,
  GithubDeliveryPlan,
  PublishResult,
} from "../types/delivery";

/** DSR contract surface: `"get delivery settings"`. */
export const getDeliverySettings = () => invoke<DeliverySettings>("get_delivery_settings");

/** DSR-FR-NFFP, DSR-FR-CPYM: `"set delivery settings (settings)"`. */
export const setDeliverySettings = (settings: DeliverySettings) =>
  invoke<DeliverySettings>("set_delivery_settings", { settings });

/** DSR-FR-AECK: `"get delivery source"`. */
export const getDeliverySource = () => invoke<DeliverySource>("get_delivery_source");

/** DSR-FR-RCLA: `"publish delivery to github"`. */
export const publishDeliveryToGithub = () =>
  invoke<PublishResult>("publish_delivery_to_github");

/** DSR-FR-OIAS: `"export delivery to file (path)"`. */
export const exportDeliveryToFile = (path: string) =>
  invoke<DeliveryExportResult>("export_delivery_to_file", { path });

/** DSR-FR-OJLB: `"load delivery milestones (refresh)"`. */
export const loadDeliveryMilestones = (refresh: boolean) =>
  invoke<DeliveryMilestones>("load_delivery_milestones", { refresh });

/** GDP-FR-QSHX: `"load github delivery plan (refresh)"`. */
export const loadGithubDeliveryPlan = (refresh: boolean) =>
  invoke<GithubDeliveryPlan>("load_github_delivery_plan", { refresh });

/** GDP-FR-VNNU: `"apply github delivery edit (edit)"`. */
export const applyGithubDeliveryEdit = (edit: GithubDeliveryEdit) =>
  invoke<GithubDeliveryPlan>("apply_github_delivery_edit", { edit });

/** GDP-FR-PIAQ: `"open tracker link (url)"`. */
export const openTrackerLink = (url: string) => invoke<void>("open_tracker_link", { url });

/** DMS-FR-DPXR: `"load delivery layout (scope)"`. */
export const loadDeliveryLayout = (scope: string) =>
  invoke<DeliveryLayout>("load_delivery_layout", { scope });

/** DMS-FR-XAZV: `"save delivery layout (scope, layout)"`. */
export const saveDeliveryLayout = (scope: string, layout: DeliveryLayout) =>
  invoke<void>("save_delivery_layout", { scope, layout });
