//! A killed process whose end the VM does not report (EAC-FR-KWXB).

use super::*;

// EAC-FR-KWXB: a killed process whose end the VM did not report has its VM
// stopped before the launch returns, the launch reports the stop as done, and
// the next launch of the scope boots a new VM.
#[test]
fn a_process_with_no_reported_end_has_its_vm_stopped() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    runtime.unconfirmed_stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    let (first, after_first) = block_on(async {
        let first = pool
            .launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args))
            .await
            .unwrap();
        let after_first = lifecycle(&runtime);
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        (first, after_first)
    });
    assert!(!first.stop_unconfirmed, "the stopped VM ended the process");
    assert_eq!(after_first, vec!["boot", "stop"], "the VM stops before the launch returns");
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
}

// EAC-FR-KWXB: a process whose end was reported keeps its VM for the next
// launch of the scope.
#[test]
fn a_process_with_a_reported_end_keeps_its_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime).iter().filter(|c| **c == "boot").count(), 1);
    assert!(!lifecycle(&runtime).contains(&"stop"));
}
