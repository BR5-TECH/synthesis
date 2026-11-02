//! The Custom Gateway a sandboxed turn reaches (EAC-FR-KVBT, EAC-FR-BZPC,
//! EAC-FR-RJDQ, EAC-FR-WTFH).
//!
//! The VM forwards DNS to the global resolvers of macOS only, so a gateway name
//! that only VPN split DNS knows does not resolve in the VM. The host resolves
//! the name, the egress policy allows each address that is not public, and the
//! guest `/etc/hosts` pins the name to the addresses. The URL keeps the name, so
//! the agent still checks the gateway certificate against it.

use std::collections::BTreeMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use super::super::runtime::BoxFuture;

use crate::agentic::SecretString;

/// The variable that selects the Amazon Bedrock API (AIC-FR-QHLN).
pub const BEDROCK_FLAG_VAR: &str = "CLAUDE_CODE_USE_BEDROCK";
/// The variable that carries the gateway URL (AIC-FR-QHLN).
pub const BEDROCK_BASE_URL_VAR: &str = "ANTHROPIC_BEDROCK_BASE_URL";
/// The name the sandbox runtime gives this machine inside a VM.
pub const HOST_ALIAS: &str = "host.microsandbox.internal";
/// EAC-FR-KVBT: the bound on one lookup.
pub const LOOKUP_BOUND: Duration = Duration::from_secs(15);
/// EAC-FR-WTFH: the mark at the end of each line the executor owns.
pub const PIN_MARK: &str = "# synthesis-gateway";

/// EAC-FR-KVBT / EAC-FR-BZPC: what the gateway URL of a launch names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayEndpoint {
    /// A host name, resolved on this machine and pinned in the guest.
    Name(String),
    /// An IP address. It gets an address rule and no lookup.
    Address(IpAddr),
    /// This machine, through the sandbox runtime's alias.
    HostMachine,
    /// `localhost` or a loopback address, which in a VM is the VM itself.
    Loopback,
}

/// Why the gateway URL of a launch could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnreadableGatewayUrl;

/// EAC-FR-KVBT: the gateway the final environment of a launch names. `Ok(None)`
/// where the launch uses no Custom Gateway.
pub fn endpoint_of(
    env: &BTreeMap<String, SecretString>,
) -> Result<Option<GatewayEndpoint>, UnreadableGatewayUrl> {
    let selected = env
        .get(BEDROCK_FLAG_VAR)
        .map(|value| value.expose().trim())
        .is_some_and(|value| !value.is_empty() && value != "0" && value != "false");
    let Some(url) = env.get(BEDROCK_BASE_URL_VAR).filter(|_| selected) else {
        return Ok(None);
    };
    let parsed = url::Url::parse(url.expose().trim()).map_err(|_| UnreadableGatewayUrl)?;
    let host = parsed.host().ok_or(UnreadableGatewayUrl)?;
    Ok(Some(match host {
        url::Host::Ipv4(address) => address_endpoint(IpAddr::V4(address)),
        url::Host::Ipv6(address) => address_endpoint(IpAddr::V6(address)),
        url::Host::Domain(name) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if name.is_empty() {
                return Err(UnreadableGatewayUrl);
            }
            if name == "localhost" || name.ends_with(".localhost") {
                GatewayEndpoint::Loopback
            } else if name == HOST_ALIAS {
                GatewayEndpoint::HostMachine
            } else {
                GatewayEndpoint::Name(name)
            }
        }
    }))
}

fn address_endpoint(address: IpAddr) -> GatewayEndpoint {
    if normalize(address).is_loopback() {
        GatewayEndpoint::Loopback
    } else {
        GatewayEndpoint::Address(normalize(address))
    }
}

/// An IPv4 address carried in an IPv6 one, as the IPv4 address.
fn normalize(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(address),
        IpAddr::V4(_) => address,
    }
}

/// EAC-FR-BZPC: whether the sandbox policy counts `address` as public. The
/// classes are those of the sandbox runtime's own classifier, so an address the
/// policy already allows gets no rule.
pub fn is_public(address: IpAddr) -> bool {
    match normalize(address) {
        IpAddr::V4(v4) => {
            let [a, b, _, _] = v4.octets();
            !(v4.is_unspecified()
                || v4.is_loopback()
                || a == 10
                || (a == 172 && (b & 0xf0) == 16)
                || (a == 192 && b == 168)
                || (a == 100 && (b & 0xc0) == 64)
                || (a == 169 && b == 254)
                || v4.is_multicast()
                || v4.is_broadcast())
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            !(v6.is_unspecified()
                || v6.is_loopback()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6.is_multicast())
        }
    }
}

/// EAC-FR-RJDQ: the addresses a lookup gave, without loopback answers, sorted
/// and without repeats. An unspecified, a multicast, and the cloud metadata
/// address are dropped as well, so no such address gets an allow rule.
pub fn usable_addresses(answers: impl IntoIterator<Item = IpAddr>) -> Vec<IpAddr> {
    let metadata = IpAddr::V4(std::net::Ipv4Addr::new(169, 254, 169, 254));
    let mut addresses: Vec<IpAddr> = answers
        .into_iter()
        .map(normalize)
        .filter(|a| !a.is_loopback() && !a.is_unspecified() && !a.is_multicast() && *a != metadata)
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

/// EAC-FR-WTFH: a resolved gateway name and every address it resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayRoute {
    pub host: String,
    /// Sorted, and without repeats.
    pub addresses: Vec<IpAddr>,
}

impl GatewayRoute {
    /// EAC-FR-WTFH: the lines the guest `/etc/hosts` holds for this route, for
    /// the addresses `reachable` admits, in address order. Empty where it
    /// admits none.
    pub fn hosts_text(&self, reachable: impl Fn(IpAddr) -> bool) -> String {
        self.addresses
            .iter()
            .filter(|address| reachable(**address))
            .map(|address| format!("{address} {} {PIN_MARK}\n", self.host))
            .collect()
    }
}

/// EAC-FR-KVBT: the lookup of a gateway name on this machine.
pub trait GatewayResolver: Send + Sync {
    fn resolve<'a>(&'a self, host: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, String>>;
}

/// EAC-FR-KVBT: the system resolver, which honours split DNS.
pub struct SystemResolver;

impl GatewayResolver for SystemResolver {
    fn resolve<'a>(&'a self, host: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, String>> {
        let host = host.to_string();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                (host.as_str(), 443)
                    .to_socket_addrs()
                    .map(|addresses| addresses.map(|a| a.ip()).collect())
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, SecretString> {
        pairs.iter().map(|(n, v)| (n.to_string(), SecretString::new(v.to_string()))).collect()
    }

    fn gateway(url: &str) -> Result<Option<GatewayEndpoint>, UnreadableGatewayUrl> {
        endpoint_of(&env(&[(BEDROCK_FLAG_VAR, "1"), (BEDROCK_BASE_URL_VAR, url)]))
    }

    // EAC-FR-KVBT / EAC-FR-BZPC: each form of the URL host.
    #[test]
    fn the_gateway_url_names_one_endpoint() {
        let name = |n: &str| Ok(Some(GatewayEndpoint::Name(n.to_string())));
        let address = |a: &str| Ok(Some(GatewayEndpoint::Address(a.parse().unwrap())));
        assert_eq!(
            gateway("https://Gateway.Corp.Example.:8443/v1?x=1"),
            name("gateway.corp.example")
        );
        assert_eq!(gateway("https://bücher.example/"), name("xn--bcher-kva.example"));
        assert_eq!(gateway("http://10.1.2.3:4000"), address("10.1.2.3"));
        assert_eq!(gateway("http://[fd00::7]:4000"), address("fd00::7"));
        assert_eq!(gateway("http://[::ffff:10.1.2.3]/"), address("10.1.2.3"));
        assert_eq!(gateway("http://localhost:4000"), Ok(Some(GatewayEndpoint::Loopback)));
        assert_eq!(gateway("http://api.localhost"), Ok(Some(GatewayEndpoint::Loopback)));
        assert_eq!(gateway("http://127.0.0.1:4000"), Ok(Some(GatewayEndpoint::Loopback)));
        assert_eq!(gateway("http://[::1]:4000"), Ok(Some(GatewayEndpoint::Loopback)));
        assert_eq!(
            gateway("http://host.microsandbox.internal:4000"),
            Ok(Some(GatewayEndpoint::HostMachine))
        );
        assert_eq!(gateway("not a url"), Err(UnreadableGatewayUrl));
        assert_eq!(gateway("unix:/run/gateway.sock"), Err(UnreadableGatewayUrl));
    }

    // EAC-FR-KVBT: a launch with no Bedrock flag uses no Custom Gateway.
    #[test]
    fn no_flag_means_no_gateway() {
        let url = (BEDROCK_BASE_URL_VAR, "https://gateway.corp.example");
        assert_eq!(endpoint_of(&env(&[url])), Ok(None));
        assert_eq!(endpoint_of(&env(&[(BEDROCK_FLAG_VAR, "0"), url])), Ok(None));
        assert_eq!(endpoint_of(&env(&[(BEDROCK_FLAG_VAR, "1")])), Ok(None));
        assert_eq!(endpoint_of(&env(&[])), Ok(None));
    }

    // EAC-FR-BZPC: the address classes of the sandbox runtime's classifier.
    #[test]
    fn public_is_what_the_sandbox_policy_counts_as_public() {
        for public in ["52.1.1.1", "8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:4860::8888"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
        for local in [
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "100.64.0.1",
            "169.254.169.254",
            "127.0.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "fd00::1",
            "fe80::1",
            "::1",
            "::",
            "ff02::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!is_public(local.parse().unwrap()), "{local}");
        }
    }

    // EAC-FR-RJDQ: loopback answers are dropped, and the rest is sorted once.
    #[test]
    fn usable_addresses_drop_loopback_and_repeats() {
        let answers = ["52.1.1.1", "127.0.0.1", "10.0.0.1", "::1", "52.1.1.1"]
            .map(|a| a.parse::<IpAddr>().unwrap());
        let usable = usable_addresses(answers);
        assert_eq!(
            usable,
            vec!["10.0.0.1".parse::<IpAddr>().unwrap(), "52.1.1.1".parse().unwrap()]
        );
        assert!(usable_addresses(["127.0.0.1".parse::<IpAddr>().unwrap()]).is_empty());
        let dropped = ["0.0.0.0", "::", "224.0.0.1", "169.254.169.254", "::ffff:127.0.0.1"]
            .map(|a| a.parse::<IpAddr>().unwrap());
        assert!(usable_addresses(dropped).is_empty());
    }

    // EAC-FR-WTFH: one marked line for each admitted address, in order.
    #[test]
    fn the_pin_is_one_marked_line_per_admitted_address() {
        let route = GatewayRoute {
            host: "gateway.corp.example".into(),
            addresses: usable_addresses(["10.0.0.1", "52.1.1.1"].map(|a| a.parse().unwrap())),
        };
        assert_eq!(
            route.hosts_text(|_| true),
            "10.0.0.1 gateway.corp.example # synthesis-gateway\n\
             52.1.1.1 gateway.corp.example # synthesis-gateway\n"
        );
        assert_eq!(
            route.hosts_text(is_public),
            "52.1.1.1 gateway.corp.example # synthesis-gateway\n"
        );
        assert_eq!(route.hosts_text(|_| false), "");
    }
}
