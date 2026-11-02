//! EAC-FR-LYGN, EAC-FR-WTFH, EAC-FR-XDTN — the scope's VM and the gateway
//! addresses and pin of a launch.

use std::net::IpAddr;

use super::*;
use crate::tools::agent_exec::sandbox::gateway::GatewayRoute;

const HOST: &str = "gateway.corp.example";

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn spec_with(addresses: &[&str]) -> SandboxSpec {
    let mut spec = spec("agent:1", "/w/gateway");
    spec.network.gateway_addresses = addresses.iter().map(|a| ip(a)).collect();
    spec
}

fn route(addresses: &[&str]) -> GatewayRoute {
    GatewayRoute { host: HOST.to_string(), addresses: addresses.iter().map(|a| ip(a)).collect() }
}

fn line(address: &str) -> String {
    format!("{address} {HOST} # synthesis-gateway\n")
}

/// What the double was asked, without the runs.
fn lifecycle(runtime: &RecordingSandbox) -> Vec<String> {
    runtime
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::Boot(..) => Some("boot".to_string()),
            Call::Stop(_) => Some("stop".to_string()),
            Call::Refresh(_) => Some("refresh".to_string()),
            Call::Pin(_, text) => Some(format!("pin {text}")),
            _ => None,
        })
        .collect()
}

/// Two launches of one scope, with their gateway addresses and routes.
fn twice(
    first: (&[&str], Option<&[&str]>),
    second: (&[&str], Option<&[&str]>),
) -> Arc<RecordingSandbox> {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let (spec_a, spec_b) = (spec_with(first.0), spec_with(second.0));
    let (route_a, route_b) = (first.1.map(route), second.1.map(route));
    block_on(async {
        let mut a = launch(&fx, "vm-g", &spec_a, &s, None);
        a.gateway = route_a.as_ref();
        pool.launch(&Silent, a, process(&[])).await.unwrap();
        let mut b = launch(&fx, "vm-g", &spec_b, &s, None);
        b.gateway = route_b.as_ref();
        pool.launch(&Silent, b, process(&[])).await.unwrap();
    });
    runtime
}

// EAC-FR-LYGN: a VM is reused where both address sets are empty or where they
// overlap, and is rebooted otherwise.
#[test]
fn a_vm_is_reused_only_where_it_can_reach_a_wanted_gateway_address() {
    let reused = vec!["boot", "refresh"];
    let rebooted = vec!["boot", "stop", "boot"];
    let cases: [(&[&str], &[&str], &Vec<&str>); 5] = [
        (&[], &[], &reused),
        (&["10.0.0.1", "10.0.0.2"], &["10.0.0.2", "10.0.0.3"], &reused),
        (&["10.0.0.1"], &["10.0.0.9"], &rebooted),
        (&["10.0.0.1"], &[], &rebooted),
        (&[], &["10.0.0.1"], &rebooted),
    ];
    for (booted, wanted, expected) in cases {
        let runtime = twice((booted, None), (wanted, None));
        assert_eq!(&lifecycle(&runtime), expected, "booted {booted:?}, wanted {wanted:?}");
    }
}

// EAC-FR-WTFH, EAC-FR-LYGN: a launch pins its gateway name, a reused VM is not
// written again for an equal pin, and a host whose public addresses changed is
// pinned again with no reboot.
#[test]
fn a_pin_is_written_only_where_it_changed() {
    let runtime = twice((&["10.0.0.1"], Some(&["10.0.0.1"])), (&["10.0.0.1"], Some(&["10.0.0.1"])));
    assert_eq!(
        lifecycle(&runtime),
        vec!["boot".to_string(), format!("pin {}", line("10.0.0.1")), "refresh".into()]
    );

    let runtime = twice((&[], Some(&["52.1.1.1"])), (&[], Some(&["52.1.1.2"])));
    assert_eq!(
        lifecycle(&runtime),
        vec![
            "boot".to_string(),
            format!("pin {}", line("52.1.1.1")),
            "refresh".into(),
            format!("pin {}", line("52.1.1.2")),
        ]
    );
}

// EAC-FR-LYGN: a reused VM pins public addresses and the addresses it booted
// with only, because its egress rules are those of its boot.
#[test]
fn a_reused_vm_pins_only_addresses_it_can_reach() {
    let runtime = twice(
        (&["10.0.0.1"], Some(&["10.0.0.1"])),
        (&["10.0.0.1", "10.0.0.2"], Some(&["10.0.0.1", "10.0.0.2", "52.1.1.1"])),
    );
    let calls = lifecycle(&runtime);
    assert_eq!(calls.last().unwrap(), &format!("pin {}{}", line("10.0.0.1"), line("52.1.1.1")));
    assert!(!calls.iter().any(|c| c == "stop"), "{calls:?}");
}

// EAC-FR-WTFH: a launch with no resolved name writes nothing.
#[test]
fn a_launch_with_no_gateway_name_writes_no_pin() {
    let runtime = twice((&["10.0.0.1"], None), (&["10.0.0.1"], None));
    assert!(!lifecycle(&runtime).iter().any(|c| c.starts_with("pin")));
}

// EAC-FR-XDTN: a reused VM whose pin cannot be written is rebooted and pinned
// again; a VM the launch booted that cannot be pinned is stopped, and the
// launch fails with `boot_failed`.
#[test]
fn a_failed_pin_reboots_a_reused_vm_and_fails_a_new_one() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.pins.lock().unwrap() = vec![Ok(()), Err("read-only".to_string())];
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let (spec_a, spec_b) = (spec_with(&[]), spec_with(&[]));
    let (route_a, route_b) = (route(&["52.1.1.1"]), route(&["52.1.1.2"]));
    block_on(async {
        let mut a = launch(&fx, "vm-p", &spec_a, &s, None);
        a.gateway = Some(&route_a);
        pool.launch(&Silent, a, process(&[])).await.unwrap();
        let mut b = launch(&fx, "vm-p", &spec_b, &s, None);
        b.gateway = Some(&route_b);
        pool.launch(&Silent, b, process(&[])).await.unwrap();
    });
    let calls = lifecycle(&runtime);
    assert_eq!(
        calls,
        vec![
            "boot".to_string(),
            format!("pin {}", line("52.1.1.1")),
            "refresh".into(),
            format!("pin {}", line("52.1.1.2")),
            "stop".into(),
            "boot".into(),
            format!("pin {}", line("52.1.1.2")),
        ]
    );

    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.pins.lock().unwrap() = vec![Err("read-only".to_string())];
    let pool = SandboxPool::new(runtime.clone());
    let spec = spec_with(&[]);
    let pinned = route(&["52.1.1.1"]);
    let result = block_on(async {
        let mut a = launch(&fx, "vm-q", &spec, &s, None);
        a.gateway = Some(&pinned);
        pool.launch(&Silent, a, process(&[])).await
    });
    match result {
        Err(SandboxLaunchError::Sandbox(failure)) => {
            assert_eq!(failure.cause, SandboxFailureCause::BootFailed);
            assert!(failure.message.contains("read-only"), "{}", failure.message);
        }
        other => panic!("expected a sandbox failure, got {other:?}"),
    }
    assert_eq!(lifecycle(&runtime).last().unwrap(), "stop");
    assert!(!runtime.calls().iter().any(|c| matches!(c, Call::Run(..))));
    assert_eq!(pool.booted("vm-q"), None);
}
