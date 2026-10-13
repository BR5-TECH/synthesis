// Isolation (GSS-global-settings-storage.md)

// ---------------------------------------------------------------------------
// Isolation (GSS-global-settings-storage.md GSS-FR-YDKS..GSS-FR-NCAQ)
// ---------------------------------------------------------------------------

/** GLS-FR-YAEA: the isolation graduation agent launches run under. */
export type IsolationMode = "none" | "microsandbox";

/** GSS-FR-YDKS: the stored record, as `save isolation settings` takes it. */
export interface IsolationSettings {
  mode: IsolationMode;
  /** `null` means the default of GSS-FR-WHYC. */
  cpus: number | null;
  /** Whole GiB. `null` means the default of GSS-FR-WHYC. */
  memoryGib: number | null;
  blockLocalNetwork: boolean;
  allowedLocalHosts: string[];
}

/** GSS-FR-DPWS: why this machine cannot run Microsandbox. */
export type IsolationUnsupportedReason =
  | "unsupported_platform"
  | "apple_silicon_required"
  | "kvm_missing"
  | "kvm_inaccessible";

/** GSS-FR-DPWS: the facts of this machine, read at each load. */
export interface IsolationHost {
  microsandboxSupported: boolean;
  unsupportedReason: IsolationUnsupportedReason | null;
  logicalCpus: number;
  totalMemoryGib: number;
}

/** What `load isolation settings` and `save isolation settings` return. */
export interface IsolationState {
  settings: IsolationSettings;
  host: IsolationHost;
  /** GSS-FR-WHYC: the CPU count a launch uses. */
  effectiveCpus: number;
  /** GSS-FR-WHYC: the RAM, in GiB, a launch uses. */
  effectiveMemoryGib: number;
}
