//! The targets a terminal can open on, and which kinds each one offers
//! (TRS-FR-ZCPH .. TRS-FR-XXPE).
//!
//! The rules are a pure function of an [`Environment`] so that each one is
//! tested without a project, Docker, or a VM.

use std::collections::{BTreeMap, BTreeSet};

use super::model::*;

/// The facts about the project and the machine that decide availability.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// DVC-FR-EVZD: the project's agent environment is `devcontainer`.
    pub devcontainer_project: bool,
    /// GSS-FR-40: the Docker backend is verified.
    pub docker_verified: bool,
    /// The dev container CLI is verified and the config is a file.
    pub devcontainer_ready: bool,
    /// The vendor `resolve_agentic_invocation` resolves for a `work` turn.
    pub work_vendor: Option<CliVendor>,
    /// PSS-FR-30: the vendors the project configures an image for.
    pub vendor_images: BTreeSet<CliVendor>,
    /// AIC: each verified CLI-kind vendor, with its executable.
    pub verified_vendors: BTreeMap<CliVendor, String>,
    /// EAC-FR-SNMS: the effective isolation of a queue launch is Microsandbox.
    pub effective_microsandbox: bool,
}

fn available(kind: TerminalKind, vendor: Option<CliVendor>, runs_in: RunsIn) -> KindAvailability {
    KindAvailability { kind, vendor, runs_in, available: true, reason: None }
}

fn unavailable(
    kind: TerminalKind,
    vendor: Option<CliVendor>,
    runs_in: RunsIn,
    reason: UnavailableReason,
) -> KindAvailability {
    KindAvailability { kind, vendor, runs_in, available: false, reason: Some(reason) }
}

/// TRS-FR-IRBL: the Container shell.
fn container_shell(env: &Environment) -> KindAvailability {
    let kind = TerminalKind::ContainerShell;
    if env.devcontainer_project {
        return if env.devcontainer_ready {
            available(kind, None, RunsIn::Devcontainer)
        } else {
            unavailable(kind, None, RunsIn::Devcontainer, UnavailableReason::DevcontainerNotReady)
        };
    }
    if !env.docker_verified {
        return unavailable(kind, None, RunsIn::Docker, UnavailableReason::DockerUnverified);
    }
    match env.work_vendor {
        Some(vendor) if env.vendor_images.contains(&vendor) => {
            available(kind, None, RunsIn::Docker)
        }
        _ => unavailable(kind, None, RunsIn::Docker, UnavailableReason::VendorImageUnconfigured),
    }
}

/// TRS-FR-KJYH: the Microsandbox shell.
fn microsandbox_shell(env: &Environment, vm_running: bool) -> KindAvailability {
    let kind = TerminalKind::MicrosandboxShell;
    let runs_in = RunsIn::Microsandbox;
    if env.devcontainer_project {
        return unavailable(kind, None, runs_in, UnavailableReason::DevcontainerProject);
    }
    if !env.effective_microsandbox {
        return unavailable(kind, None, runs_in, UnavailableReason::IsolationNotMicrosandbox);
    }
    if !vm_running {
        return unavailable(kind, None, runs_in, UnavailableReason::NoVmRunning);
    }
    available(kind, None, runs_in)
}

/// TRS-FR-RCGL: where an Agent CLI of `vendor` runs.
pub fn agent_runs_in(env: &Environment, vendor: CliVendor) -> RunsIn {
    if env.devcontainer_project {
        RunsIn::Devcontainer
    } else if env.docker_verified && env.vendor_images.contains(&vendor) {
        RunsIn::Docker
    } else {
        RunsIn::Host
    }
}

/// TRS-FR-RCGL: the Agent CLI entry of one vendor.
fn agent_cli(env: &Environment, vendor: CliVendor) -> KindAvailability {
    let kind = TerminalKind::AgentCli;
    let runs_in = agent_runs_in(env, vendor);
    if !env.verified_vendors.contains_key(&vendor) {
        return unavailable(kind, Some(vendor), runs_in, UnavailableReason::VendorUnverified);
    }
    if runs_in == RunsIn::Devcontainer && !env.devcontainer_ready {
        return unavailable(kind, Some(vendor), runs_in, UnavailableReason::DevcontainerNotReady);
    }
    available(kind, Some(vendor), runs_in)
}

/// TRS-FR-QHKU .. TRS-FR-XXPE: every kind for one target, in the dialog's order.
pub fn kinds_for(env: &Environment, vm_running: bool) -> Vec<KindAvailability> {
    let mut kinds = vec![
        available(TerminalKind::HostShell, None, RunsIn::Host),
        container_shell(env),
        microsandbox_shell(env, vm_running),
    ];
    kinds.extend(CliVendor::ALL.into_iter().map(|vendor| agent_cli(env, vendor)));
    kinds
}

/// One worktree as the enumeration reports it.
#[derive(Debug, Clone)]
pub struct WorktreeFact {
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    pub is_active: bool,
    pub is_missing: bool,
    /// The stream whose working copy this worktree is: id, name, busy run.
    pub stream: Option<(String, String, Option<String>)>,
}

/// TRS-FR-ZCPH: one target for each present worktree that is no stream's
/// working copy, the active one first, then one for each present stream.
/// `busy_of_worktree` gives the run that holds a worktree's own queue.
pub fn build_targets(
    worktrees: &[WorktreeFact],
    busy_of_worktree: impl Fn(&str) -> Option<String>,
    kinds_of: impl Fn(&TargetRef) -> Vec<KindAvailability>,
) -> Vec<TerminalTarget> {
    let mut plain: Vec<&WorktreeFact> =
        worktrees.iter().filter(|w| !w.is_missing && w.stream.is_none()).collect();
    plain.sort_by_key(|w| !w.is_active);
    let mut targets: Vec<TerminalTarget> = plain
        .into_iter()
        .map(|w| {
            let target = TargetRef::Worktree { path: w.path.clone() };
            TerminalTarget {
                kinds: kinds_of(&target),
                target,
                name: w.name.clone(),
                branch: w.branch.clone(),
                is_active_worktree: w.is_active,
                busy_run_id: busy_of_worktree(&w.path),
            }
        })
        .collect();
    for w in worktrees.iter().filter(|w| !w.is_missing) {
        let Some((stream_id, stream_name, busy)) = &w.stream else { continue };
        let target = TargetRef::WorkStream { stream_id: stream_id.clone() };
        targets.push(TerminalTarget {
            kinds: kinds_of(&target),
            target,
            name: stream_name.clone(),
            branch: w.branch.clone(),
            is_active_worktree: w.is_active,
            busy_run_id: busy.clone(),
        });
    }
    targets
}

#[cfg(test)]
#[path = "targets_tests.rs"]
mod tests;
