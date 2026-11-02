//! The Docker daemon in the VM (EAC-FR-QMRW, EAC-FR-ZPTA, EAC-FR-UEFI).

use super::*;

fn daemon_spec(workspace: &str) -> SandboxSpec {
    SandboxSpec { docker_daemon: true, ..spec("agent:1", workspace) }
}

// EAC-FR-QMRW: a VM booted with the daemon option starts the daemon after the
// boot and before the process, with the host user's GID as the socket group. A
// reuse asks the daemon again, which ends at once where it answers.
#[test]
fn a_daemon_vm_starts_its_daemon_before_each_process() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = SandboxSpec { owner: "501:20".to_string(), ..daemon_spec("/w/daemon") };
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[])).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[])).await.unwrap();
    });
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Boot("vm-a".into(), BootImage::Registry("agent:1".into())),
            Call::Daemon("vm-a".into(), 20),
            Call::Run("vm-a".into(), Vec::new()),
            Call::Refresh("vm-a".into()),
            Call::Daemon("vm-a".into(), 20),
            Call::Run("vm-a".into(), Vec::new()),
        ]
    );
    assert_eq!(pool.booted("vm-a"), Some(spec));
}

// EAC-FR-QMRW, EAC-FR-UEFI: a reused VM whose daemon no longer answers is
// rebooted, and the new VM starts its daemon before the process.
#[test]
fn a_reused_vm_whose_daemon_does_not_answer_is_rebooted() {
    let fx = fixture();
    let runtime = Arc::new(RecordingSandbox {
        daemons: StdMutex::new(vec![Ok(()), Err("dockerd exited: killed (exit 3)".to_string())]),
        ..Default::default()
    });
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = daemon_spec("/w/gone");
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[])).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[])).await.unwrap();
    });
    let lifecycle: Vec<&str> = runtime
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::Boot(..) => Some("boot"),
            Call::Stop(..) => Some("stop"),
            Call::Daemon(..) => Some("daemon"),
            Call::Run(..) => Some("run"),
            _ => None,
        })
        .collect();
    assert_eq!(lifecycle, vec!["boot", "daemon", "run", "daemon", "stop", "boot", "daemon", "run"]);
    assert_eq!(pool.booted("vm-a"), Some(spec));
}

// EAC-FR-ZPTA, EAC-FR-KTWO: an owner that names no group gives the daemon
// socket no group, so the launch fails as `docker_daemon_unavailable`, the VM
// is stopped, and nothing is kept.
#[test]
fn an_owner_without_a_group_fails_the_daemon_start() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = SandboxSpec { owner: "1000".to_string(), ..daemon_spec("/w/nogroup") };
    let failed = block_on(pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[])));
    assert!(matches!(
        failed,
        Err(SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::DockerDaemonUnavailable,
            ..
        }))
    ));
    let calls = runtime.calls();
    assert_eq!(calls.last(), Some(&Call::Stop("vm-a".into())), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Daemon(..) | Call::Run(..))));
    assert_eq!(pool.booted("vm-a"), None);
}

// EAC-FR-QMRW: a VM booted without the option starts no daemon.
#[test]
fn a_vm_without_the_option_starts_no_daemon() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/plain");
    block_on(pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[]))).unwrap();
    assert!(!runtime.calls().iter().any(|call| matches!(call, Call::Daemon(..))));
}

// EAC-FR-ZPTA, EAC-FR-NCSA: a daemon that is not available fails the launch
// with `docker_daemon_unavailable` and the runtime's message, runs no process,
// stops the VM, and keeps it for no later launch: the next launch boots again.
#[test]
fn a_missing_daemon_fails_stops_the_vm_and_is_not_reused() {
    let fx = fixture();
    let runtime = Arc::new(RecordingSandbox {
        daemons: StdMutex::new(vec![Err("dockerd is not in the image (exit 127)".to_string())]),
        ..Default::default()
    });
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = daemon_spec("/w/missing");
    let failed = block_on(pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[])));
    assert_eq!(
        failed,
        Err(SandboxLaunchError::Sandbox(SandboxFailure {
            cause: SandboxFailureCause::DockerDaemonUnavailable,
            message: "dockerd is not in the image (exit 127)".to_string(),
        }))
    );
    assert_eq!(
        runtime.calls(),
        vec![
            Call::Boot("vm-a".into(), BootImage::Registry("agent:1".into())),
            Call::Daemon("vm-a".into(), 1000),
            Call::Stop("vm-a".into()),
        ]
    );
    assert_eq!(pool.booted("vm-a"), None);

    // EAC-FR-ZPTA: Continue boots a new VM, which finds the daemon this time.
    block_on(pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[]))).unwrap();
    let after: Vec<Call> = runtime.calls().into_iter().skip(3).collect();
    assert_eq!(
        after,
        vec![
            Call::Boot("vm-a".into(), BootImage::Registry("agent:1".into())),
            Call::Daemon("vm-a".into(), 1000),
            Call::Run("vm-a".into(), Vec::new()),
        ]
    );
}

// EAC-FR-UEFI, EAC-FR-QMRW: the daemon option is part of what a VM boots with,
// so a change of it in either direction reboots the VM.
#[test]
fn a_change_of_the_daemon_option_reboots_the_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let plain = spec("agent:1", "/w/option");
    let daemon = SandboxSpec { docker_daemon: true, ..plain.clone() };
    block_on(async {
        for next in [&plain, &daemon, &plain] {
            pool.launch(&Silent, launch(&fx, "vm-a", next, &s, None), process(&[])).await.unwrap();
        }
    });
    let lifecycle: Vec<&str> = runtime
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::Boot(..) => Some("boot"),
            Call::Stop(..) => Some("stop"),
            Call::Daemon(..) => Some("daemon"),
            _ => None,
        })
        .collect();
    assert_eq!(lifecycle, vec!["boot", "stop", "boot", "daemon", "stop", "boot"]);
    assert_eq!(pool.booted("vm-a"), Some(plain));
}

// EAC-FR-QMRW, EAC-FR-MIQG: a run cancelled while its daemon starts ends as
// cancelled, runs no process, and the VM is stopped and not kept.
#[test]
fn a_cancelled_daemon_start_ends_the_launch() {
    let fx = fixture();
    let cancel = CancellationToken::new();
    let runtime = Arc::new(RecordingSandbox {
        cancel_in_daemon: StdMutex::new(Some(cancel.clone())),
        ..Default::default()
    });
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = daemon_spec("/w/cancel");
    let result = block_on(async {
        tokio::time::timeout(
            NEVER,
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), cancellable(&cancel)),
        )
        .await
        .expect("a cancelled daemon start ends the launch")
    });
    assert!(is_cancelled(&result), "{result:?}");
    let calls = runtime.calls();
    assert_eq!(calls.last(), Some(&Call::Stop("vm-a".into())), "{calls:?}");
    assert!(!calls.iter().any(|call| matches!(call, Call::Run(..))), "{calls:?}");
    assert_eq!(pool.booted("vm-a"), None);
}
