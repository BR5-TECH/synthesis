//! EAC-FR-CGCE / EAC-FR-DMYB: the egress rules of one VM, as the microsandbox
//! network policy states them. Compiled for Linux and macOS, under test as
//! well, so the rules are asserted without a VM.

use microsandbox::{NetworkAction, NetworkPolicy};

use super::SandboxSpec;
use crate::isolation::AllowedHost;

/// EAC-FR-CGCE / EAC-FR-DMYB: the egress rules of one VM.
pub(super) fn policy(spec: &SandboxSpec) -> Result<NetworkPolicy, String> {
    if !spec.network.block_local_network {
        return Ok(NetworkPolicy::allow_all());
    }
    let allowed = spec.network.allowed.clone();
    NetworkPolicy::builder()
        .default_egress(NetworkAction::Deny)
        .default_ingress(NetworkAction::Allow)
        // DNS through the sandbox gateway stays allowed.
        .egress(|r| r.udp().tcp().port(53).allow_host())
        .egress(move |r| {
            for host in &allowed {
                match host {
                    AllowedHost::Network(cidr) => {
                        r.allow().cidr(cidr.clone());
                    }
                    AllowedHost::HostName(name) => {
                        r.allow().domain(name.clone());
                    }
                    AllowedHost::HostMachine => {
                        r.allow_host();
                    }
                }
            }
            r
        })
        // Outbound internet access is always allowed.
        .egress(|r| r.allow_public())
        .build()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::{SandboxMount, SandboxNetwork};
    use super::*;

    fn spec(block: bool, allowed: Vec<AllowedHost>) -> SandboxSpec {
        SandboxSpec {
            image: "agent:1".into(),
            cpus: 1,
            memory_gib: 1,
            mounts: Vec::<SandboxMount>::new(),
            network: SandboxNetwork { block_local_network: block, allowed },
        }
    }

    fn rules(policy: &NetworkPolicy) -> Vec<serde_json::Value> {
        let json = serde_json::to_value(policy).unwrap();
        json["rules"].as_array().cloned().unwrap_or_default()
    }

    // EAC-FR-CGCE: with the switch on, egress is denied by default, DNS through
    // the gateway comes first, and the public internet is allowed last.
    #[test]
    fn the_block_switch_denies_by_default_and_allows_dns_and_the_internet() {
        let policy = policy(&spec(true, Vec::new())).unwrap();
        let json = serde_json::to_value(&policy).unwrap();
        assert_eq!(json["default_egress"], "deny");
        let rules = rules(&policy);
        assert_eq!(rules.len(), 2, "{rules:?}");
        assert_eq!(rules[0]["destination"]["group"], "host");
        assert_eq!(rules[0]["ports"][0]["start"], 53);
        assert_eq!(rules[1]["destination"]["group"], "public");
        assert_eq!(rules[1]["action"], "allow");
    }

    // EAC-FR-DMYB: each allowed host becomes an allow rule before the public
    // rule: a range, a name, and this machine through the gateway.
    #[test]
    fn allowed_hosts_become_allow_rules() {
        let policy = policy(&spec(
            true,
            vec![
                AllowedHost::Network("10.1.0.0/16".into()),
                AllowedHost::HostName("gateway.lan".into()),
                AllowedHost::HostMachine,
            ],
        ))
        .unwrap();
        let rules = rules(&policy);
        assert_eq!(rules.len(), 5, "{rules:?}");
        let text = serde_json::to_string(&rules[1..4]).unwrap();
        assert!(text.contains("10.1.0.0/16"), "{text}");
        assert!(text.contains("gateway.lan"), "{text}");
        assert_eq!(rules[3]["destination"]["group"], "host");
        assert!(rules[1..4].iter().all(|r| r["action"] == "allow"));
        assert_eq!(rules[4]["destination"]["group"], "public");
    }

    // EAC-FR-CGCE: with the switch off, no egress is denied.
    #[test]
    fn the_switch_off_denies_no_egress() {
        let policy = policy(&spec(false, vec![AllowedHost::HostMachine])).unwrap();
        let json = serde_json::to_value(&policy).unwrap();
        assert_eq!(json["default_egress"], "allow");
        assert!(rules(&policy).is_empty());
    }
}
