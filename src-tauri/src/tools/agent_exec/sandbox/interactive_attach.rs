//! The interactive attach to a running VM (EAC-FR-QQMA, EAC-FR-INWE).
//!
//! An interactive process is not a launch of the scope's queue. It does not
//! take the scope's slot, so it waits for no launch and no launch waits for
//! it. It does not keep its VM either: a stop or a reboot of the VM ends it.

use super::SandboxPool;
use crate::terminals::process::ProcessHandle;

/// EAC-FR-QQMA: the refusal where no VM of the scope runs.
pub const NO_VM_RUNNING: &str = "no_vm_running";

/// One process with a terminal in a running VM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveVmProcess {
    /// The whole argument vector, program first. The image's entrypoint does
    /// not run it (EAC-FR-HTEB).
    pub argv: Vec<String>,
    /// EAC-FR-ZKMR: the directory the process stands in.
    pub workdir: String,
    /// `<uid>:<gid>`, the host user's.
    pub user: String,
    /// Variables set over the image's `Env` (EAC-FR-PWLA). No credential.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

impl SandboxPool {
    /// TRS-FR-KJYH: whether the pool holds `vm` as booted now. It never waits
    /// for a slot: a slot that a launch holds is a VM that runs.
    pub fn is_booted(&self, vm: &str) -> bool {
        let slot = {
            let slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            slots.get(vm).cloned()
        };
        let Some(slot) = slot else {
            return false;
        };
        let booted = match slot.try_lock() {
            Ok(booted) => booted.as_ref().is_some_and(|b| b.epoch == self.epoch_of(vm)),
            Err(_) => true,
        };
        booted
    }

    /// EAC-FR-QQMA: start `process` in `vm` where it runs. It never boots,
    /// and it holds no slot (EAC-FR-INWE).
    pub async fn attach_interactive(
        &self,
        vm: &str,
        process: InteractiveVmProcess,
    ) -> Result<ProcessHandle, String> {
        if !self.is_booted(vm) {
            return Err(NO_VM_RUNNING.to_string());
        }
        self.runtime.attach_interactive(vm, process).await
    }
}

#[cfg(test)]
impl SandboxPool {
    /// Records `vm` as booted in the current epoch, for a test.
    pub fn mark_booted_for_test(&self, vm: &str) {
        let spec = super::SandboxSpec {
            image: "image".to_string(),
            owner: "1:1".to_string(),
            cpus: 1,
            memory_gib: 1,
            disk_gib: 1,
            mounts: Vec::new(),
            network: super::SandboxNetwork {
                block_local_network: false,
                allowed: Vec::new(),
                gateway_addresses: Vec::new(),
            },
            docker_daemon: false,
        };
        let booted =
            super::Booted { spec, identities: Vec::new(), epoch: self.epoch_of(vm), pins: None };
        let slot = self.slot(vm);
        *slot.try_lock().expect("the slot is free") = Some(booted);
    }

    /// Runs `work` while the slot of `vm` is held, as a launch holds it.
    pub async fn while_slot_held<F: std::future::Future>(&self, vm: &str, work: F) -> F::Output {
        let slot = self.slot(vm);
        let _held = slot.lock().await;
        work.await
    }
}
