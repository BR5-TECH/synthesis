//! The stop of the VMs of one project (EAC-FR-ABHG, PWN-FR-ISOL).

use super::*;

fn vm_of(project: &str, queue: &str) -> String {
    vm_name(project, &SandboxScope { queue_key: queue.to_string() })
}

// EAC-FR-ABHG: a project that closes stops its own VMs. The VM of a run in
// another open project keeps running, and its next launch reuses it.
#[test]
fn closing_one_project_stops_only_its_own_vms() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let first = vm_of("project-a", "stream-1");
    let second = vm_of("project-a", "stream-2");
    let other = vm_of("project-b", "stream-1");
    assert!(first.starts_with(&vm_prefix("project-a")));
    assert!(!other.starts_with(&vm_prefix("project-a")));
    block_on(async {
        for vm in [&first, &second, &other] {
            pool.launch(&Silent, launch(&fx, vm, &spec, &s, None), process(&[])).await.unwrap();
        }
        pool.stop_project("project-a").await;
    });

    let stops: Vec<Call> =
        runtime.calls().into_iter().filter(|c| matches!(c, Call::Stop(_))).collect();
    assert_eq!(stops.len(), 2, "two VMs of the project a stop: {stops:?}");
    assert!(stops.contains(&Call::Stop(first.clone())));
    assert!(stops.contains(&Call::Stop(second.clone())));
    assert_eq!(pool.booted(&first), None);
    assert_eq!(pool.booted(&second), None);
    assert!(pool.booted(&other).is_some(), "the VM of the project b still runs");

    block_on(pool.launch(&Silent, launch(&fx, &other, &spec, &s, None), process(&[]))).unwrap();
    let boots = runtime.calls().into_iter().filter(|c| matches!(c, Call::Boot(..))).count();
    assert_eq!(boots, 3, "the project b reuses its VM and boots nothing new");
}

// EAC-FR-ABHG: a VM of the closing project that a launch holds is never reused,
// and a VM of another project is not made stale by the stop.
#[test]
fn a_held_vm_of_the_closed_project_is_not_reused() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/one");
    let mine = vm_of("project-a", "stream-1");
    let other = vm_of("project-b", "stream-1");
    block_on(async {
        for vm in [&mine, &other] {
            pool.launch(&Silent, launch(&fx, vm, &spec, &s, None), process(&[])).await.unwrap();
        }
        let slot = pool.slot(&mine);
        let held = slot.lock().await;
        pool.stop_project("project-a").await;
        drop(held);
        pool.launch(&Silent, launch(&fx, &mine, &spec, &s, None), process(&[])).await.unwrap();
        pool.launch(&Silent, launch(&fx, &other, &spec, &s, None), process(&[])).await.unwrap();
    });
    let boots = runtime.calls().into_iter().filter(|c| matches!(c, Call::Boot(..))).count();
    assert_eq!(boots, 3, "only the VM of the closed project boots again");
}
