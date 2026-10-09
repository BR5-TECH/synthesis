use super::*;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

use rcgen::{
    date_time_ymd, BasicConstraints, CertificateParams, DnType, IsCa, KeyPair,
};

// ---------------------------------------------------------------------------
// The typed error
// ---------------------------------------------------------------------------

// AAP-FR-HZTB, AAP-FR-PKWE
#[test]
fn a_tls_failure_is_written_as_cause_then_host_and_read_back() {
    let failure = TlsFailure::new("api.example.com", TlsCause::UnknownIssuer);
    assert_eq!(failure.wire(), "tls_untrusted:unknown_issuer:api.example.com");
    assert_eq!(TlsFailure::parse_wire(&failure.wire()), Some(failure));
    for cause in [
        TlsCause::UnknownIssuer,
        TlsCause::Expired,
        TlsCause::HostnameMismatch,
        TlsCause::Other,
    ] {
        let failure = TlsFailure::new("h.test", cause);
        assert_eq!(TlsFailure::parse_wire(&failure.wire()), Some(failure));
    }
}

// AAP-FR-PKWE
#[test]
fn an_ipv6_host_survives_the_wire_text() {
    let failure = TlsFailure::new("::1", TlsCause::Other);
    assert_eq!(failure.wire(), "tls_untrusted:other:::1");
    assert_eq!(TlsFailure::parse_wire(&failure.wire()), Some(failure));
}

// AAP-FR-HZTB
#[test]
fn other_error_text_is_not_read_as_a_tls_failure() {
    for text in [
        "unreachable",
        "rejected",
        "tls_untrusted",
        "tls_untrusted:",
        "tls_untrusted:unknown_issuer",
        "tls_untrusted:unknown_issuer:",
        "tls_untrusted:bogus:host.test",
    ] {
        assert_eq!(TlsFailure::parse_wire(text), None, "{text}");
    }
}

// AAP-FR-PKWE
#[test]
fn the_host_has_no_scheme_port_path_or_user_information() {
    assert_eq!(host_of("https://api.openai.com/v1/models"), "api.openai.com");
    assert_eq!(host_of("https://gateway.corp:8443/v1"), "gateway.corp");
    assert_eq!(host_of("https://user:secret@gateway.corp/v1"), "gateway.corp");
    assert_eq!(host_of("http://localhost:11434"), "localhost");
    assert_eq!(host_of("https://[::1]:8443/x"), "::1");
    assert_eq!(host_of("https://host.test?token=abc"), "host.test");
    assert_eq!(host_of(""), "");
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

// AAP-FR-HZTB
#[test]
fn certificate_errors_map_to_their_cause_codes() {
    use rustls::CertificateError as C;
    let cause = |e: C| cause_of_rustls(&rustls::Error::InvalidCertificate(e));
    assert_eq!(cause(C::UnknownIssuer), TlsCause::UnknownIssuer);
    assert_eq!(cause(C::Expired), TlsCause::Expired);
    assert_eq!(cause(C::NotValidForName), TlsCause::HostnameMismatch);
    assert_eq!(cause(C::BadSignature), TlsCause::Other);
    assert_eq!(cause(C::NotValidYet), TlsCause::Other);
    assert_eq!(cause_of_rustls(&rustls::Error::NoCertificatesPresented), TlsCause::Other);
}

// AAP-FR-HZTB
#[test]
fn a_tls_error_is_found_inside_wrapping_io_errors() {
    let inner = rustls::Error::InvalidCertificate(rustls::CertificateError::Expired);
    let wrapped = std::io::Error::other(std::io::Error::new(std::io::ErrorKind::InvalidData, inner));
    assert_eq!(classify_error(&wrapped), Some(TlsCause::Expired));
    let plain = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
    assert_eq!(classify_error(&plain), None);
}

// AAP-FR-HZTB
#[test]
fn a_ureq_error_that_is_not_tls_has_no_tls_failure() {
    assert_eq!(ureq_failure(&ureq::Error::StatusCode(500), "https://h.test"), None);
    assert_eq!(ureq_failure(&ureq::Error::ConnectionFailed, "https://h.test"), None);
    let tls = ureq::Error::Rustls(rustls::Error::InvalidCertificate(
        rustls::CertificateError::UnknownIssuer,
    ));
    assert_eq!(
        ureq_failure(&tls, "https://h.test:8443/v1"),
        Some(TlsFailure::new("h.test", TlsCause::UnknownIssuer))
    );
}

// ---------------------------------------------------------------------------
// The roots
// ---------------------------------------------------------------------------

pub(crate) fn private_ca() -> (rcgen::Certificate, KeyPair) {
    let mut params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name.push(DnType::CommonName, "Synthesis test CA");
    let key = KeyPair::generate().expect("ca key");
    let cert = params.self_signed(&key).expect("ca cert");
    (cert, key)
}

pub(crate) struct Leaf {
    pub der: Vec<u8>,
    pub key: Vec<u8>,
}

pub(crate) fn leaf_signed_by(
    ca: &(rcgen::Certificate, KeyPair),
    names: &[&str],
    expired: bool,
) -> Leaf {
    let mut params =
        CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
            .expect("leaf params");
    if expired {
        params.not_before = date_time_ymd(2001, 1, 1);
        params.not_after = date_time_ymd(2002, 1, 1);
    }
    let key = KeyPair::generate().expect("leaf key");
    let cert = params.signed_by(&key, &ca.0, &ca.1).expect("leaf cert");
    Leaf { der: cert.der().to_vec(), key: key.serialize_der() }
}

// AAP-FR-MXJD
#[test]
fn the_bundled_roots_stand_alone_when_the_operating_system_gives_none() {
    let roots = TrustRoots::assemble(Vec::new(), 3);
    assert!(!roots.os_store_readable());
    assert_eq!(roots.os_roots(), 0);
    assert!(roots.bundled_roots() > 100);
    assert_eq!(roots.ders().len(), roots.bundled_roots());
}

// AAP-FR-MXJD
#[test]
fn operating_system_roots_are_trusted_in_addition_to_the_bundled_ones() {
    let (ca, _) = private_ca();
    let os_root = ca.der().to_vec();
    let roots = TrustRoots::assemble(vec![os_root.clone(), os_root.clone()], 0);
    assert!(roots.os_store_readable());
    assert_eq!(roots.os_roots(), 1, "a repeated root is held once");
    assert!(roots.bundled_roots() > 100);
    assert!(roots.ders().contains(&os_root));
}

// AAP-FR-MXJD
#[test]
fn a_root_the_tls_library_cannot_read_is_left_out() {
    let roots = TrustRoots::assemble(vec![vec![1, 2, 3, 4]], 0);
    assert_eq!(roots.os_roots(), 0);
    assert!(!roots.os_store_readable());
}

// AAP-FR-MXJD
#[test]
fn the_roots_of_this_run_are_read_once_and_shared() {
    let first = roots();
    let second = roots();
    assert!(Arc::ptr_eq(&first, &second));
    assert!(first.ders().len() >= first.bundled_roots());
}

// AAP-FR-HZTB
#[test]
fn a_clients_record_holds_its_own_refusal_and_gives_it_once() {
    let first = TlsRecord::new();
    let second = TlsRecord::new();
    first.set_for_test(TlsFailure::new("h.test", TlsCause::Expired));
    assert_eq!(second.take(), None, "another client's record is not read");
    assert_eq!(first.take(), Some(TlsFailure::new("h.test", TlsCause::Expired)));
    assert_eq!(first.take(), None);
}

// ---------------------------------------------------------------------------
// A real handshake
// ---------------------------------------------------------------------------

pub(crate) fn serve(leaf: &Leaf) -> SocketAddr {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let chain = vec![rustls::pki_types::CertificateDer::from(leaf.der.clone())];
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
        rustls::pki_types::PrivatePkcs8KeyDer::from(leaf.key.clone()),
    );
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("server certificate"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("address");
    std::thread::spawn(move || {
        for tcp in listener.incoming() {
            let Ok(tcp) = tcp else { continue };
            let config = config.clone();
            std::thread::spawn(move || {
                let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));
                let Ok(connection) = rustls::ServerConnection::new(config) else {
                    return;
                };
                let mut tls = rustls::StreamOwned::new(connection, tcp);
                let mut buffer = [0u8; 4096];
                if tls.read(&mut buffer).is_err() {
                    return;
                }
                let _ = tls.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                );
                let _ = tls.flush();
            });
        }
    });
    addr
}

fn ureq_get(roots: &TrustRoots, port: u16) -> Result<String, ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(roots.ureq_tls_config())
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let url = format!("https://localhost:{port}/");
    agent.get(&url).call()?.body_mut().read_to_string()
}

fn ureq_cause(roots: &TrustRoots, port: u16) -> Option<TlsCause> {
    let error = ureq_get(roots, port).expect_err("the handshake is refused");
    classify_ureq(&error)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

/// Call the server through the `reqwest` client of `rig`. `name` is a unique
/// host name that resolves to the server, so tests running in parallel do not
/// read each other's record.
fn rig_get(
    roots: &TrustRoots,
    name: &str,
    addr: SocketAddr,
) -> (Result<String, String>, TlsRecord) {
    let record = TlsRecord::new();
    let client = roots
        .reqwest013_builder(&record)
        .expect("rustls config")
        .no_proxy()
        .resolve(name, addr)
        .timeout(Duration::from_secs(20))
        .build()
        .expect("client");
    let result = runtime().block_on(async {
        let response = client
            .get(format!("https://{name}:{}/", addr.port()))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        response.text().await.map_err(|e| e.to_string())
    });
    (result, record)
}

/// The same call through the `reqwest` client of `openrouter-rs`.
fn openrouter_get(
    roots: &TrustRoots,
    name: &str,
    addr: SocketAddr,
) -> (Result<String, String>, TlsRecord) {
    let record = TlsRecord::new();
    let client = roots
        .reqwest012_builder(&record)
        .expect("rustls config")
        .no_proxy()
        .resolve(name, addr)
        .timeout(Duration::from_secs(20))
        .build()
        .expect("client");
    let result = runtime().block_on(async {
        let response = client
            .get(format!("https://{name}:{}/", addr.port()))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        response.text().await.map_err(|e| e.to_string())
    });
    (result, record)
}

fn trusting(ca: &(rcgen::Certificate, KeyPair)) -> TrustRoots {
    TrustRoots::assemble(vec![ca.0.der().to_vec()], 0)
}

// AAP-FR-QLNV, AAP-FR-MXJD
#[test]
fn a_certificate_from_a_root_in_the_operating_system_store_is_trusted() {
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["localhost", "trusted-rig.test", "trusted-or.test"], false));
    let roots = trusting(&ca);
    assert_eq!(ureq_get(&roots, addr.port()).expect("ureq connects"), "{}");
    assert_eq!(rig_get(&roots, "trusted-rig.test", addr).0.expect("rig connects"), "{}");
    assert_eq!(
        openrouter_get(&roots, "trusted-or.test", addr).0.expect("openrouter connects"),
        "{}"
    );
}

// AAP-FR-QLNV, AAP-FR-HZTB
#[test]
fn a_certificate_from_an_unknown_root_is_refused_with_unknown_issuer() {
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["localhost", "unknown-rig.test", "unknown-or.test"], false));
    let roots = TrustRoots::assemble(Vec::new(), 0);
    assert_eq!(ureq_cause(&roots, addr.port()), Some(TlsCause::UnknownIssuer));

    let (result, record) = rig_get(&roots, "unknown-rig.test", addr);
    assert!(result.is_err());
    assert_eq!(
        record.take(),
        Some(TlsFailure::new("unknown-rig.test", TlsCause::UnknownIssuer))
    );

    let (result, record) = openrouter_get(&roots, "unknown-or.test", addr);
    assert!(result.is_err());
    assert_eq!(
        record.take(),
        Some(TlsFailure::new("unknown-or.test", TlsCause::UnknownIssuer))
    );
}

// AAP-FR-QLNV, AAP-FR-HZTB
#[test]
fn an_expired_certificate_is_refused_with_expired() {
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["localhost", "expired-rig.test", "expired-or.test"], true));
    let roots = trusting(&ca);
    assert_eq!(ureq_cause(&roots, addr.port()), Some(TlsCause::Expired));

    let (result, record) = rig_get(&roots, "expired-rig.test", addr);
    assert!(result.is_err());
    assert_eq!(
        record.take(),
        Some(TlsFailure::new("expired-rig.test", TlsCause::Expired))
    );

    let (result, record) = openrouter_get(&roots, "expired-or.test", addr);
    assert!(result.is_err());
    assert_eq!(
        record.take(),
        Some(TlsFailure::new("expired-or.test", TlsCause::Expired))
    );
}

// AAP-FR-QLNV, AAP-FR-HZTB
#[test]
fn a_certificate_for_another_host_is_refused_with_hostname_mismatch() {
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["some-other-host.test"], false));
    let roots = trusting(&ca);
    assert_eq!(ureq_cause(&roots, addr.port()), Some(TlsCause::HostnameMismatch));

    let (result, record) = rig_get(&roots, "mismatch-rig.test", addr);
    assert!(result.is_err());
    assert_eq!(
        record.take(),
        Some(TlsFailure::new("mismatch-rig.test", TlsCause::HostnameMismatch))
    );

    let (result, record) = openrouter_get(&roots, "mismatch-or.test", addr);
    assert!(result.is_err());
    assert_eq!(
        record.take(),
        Some(TlsFailure::new("mismatch-or.test", TlsCause::HostnameMismatch))
    );
}

// AAP-FR-WMCX
#[test]
fn no_client_of_the_application_builds_its_own_trust_setup() {
    const SOURCES: [(&str, &str); 9] = [
        ("ai_shared.rs", include_str!("../ai_shared.rs")),
        ("github_tokens.rs", include_str!("../github_tokens.rs")),
        ("relay_endpoint.rs", include_str!("../relay_endpoint.rs")),
        ("github_polling/client.rs", include_str!("../github_polling/client.rs")),
        ("github_publication/client.rs", include_str!("../github_publication/client.rs")),
        ("git/pull_requests/client.rs", include_str!("../git/pull_requests/client.rs")),
        ("ai_openrouter.rs", include_str!("../ai_openrouter.rs")),
        ("agent_conversations/rig_bridge.rs", include_str!("../agent_conversations/rig_bridge.rs")),
        (
            "agent_conversations/openrouter_carrier.rs",
            include_str!("../agent_conversations/openrouter_carrier.rs"),
        ),
    ];
    for (name, source) in SOURCES {
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        let code: String = code
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("ureq::Agent::config_builder"),
            "{name} builds a ureq agent with the default trust setup"
        );
        for forbidden in ["reqwest::Client", "reqwest013::Client", "reqwest012::Client", "ureq::agent(", "ureq::get(", "ureq::post("] {
            assert!(!code.contains(forbidden), "{name} builds its own HTTP client: {forbidden}");
        }
        let builders = code.matches("Client::builder()").count();
        let injected = code.matches(".http_client(").count();
        assert_eq!(
            builders, injected,
            "{name}: every rig or openrouter-rs client takes the shared HTTP client"
        );
    }
}

// AAP-FR-05, AAP-FR-WMCX, AAP-FR-HZTB
#[test]
fn the_endpoint_prober_reports_an_untrusted_certificate_as_tls_untrusted() {
    use crate::ai_shared::{
        AuthStyle, EndpointProber, HttpEndpointProber, ModelsFormat, ProbeError, ProbeRequest,
    };
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["localhost"], false));
    let base_url = format!("https://localhost:{}", addr.port());
    let error = HttpEndpointProber
        .probe(&ProbeRequest {
            base_url: &base_url,
            api_key: Some("sk-secret-key"),
            auth: AuthStyle::Bearer,
            models_path: "/models",
            models_format: ModelsFormat::Lenient,
            report_status: false,
        })
        .expect_err("the certificate is from no trusted root");
    assert_eq!(
        error,
        ProbeError::TlsUntrusted(TlsFailure::new("localhost", TlsCause::UnknownIssuer))
    );
}

// AAP-FR-23, AAP-FR-WMCX, AAP-FR-HZTB
#[test]
fn the_openrouter_prober_reports_an_untrusted_certificate_as_tls_untrusted() {
    use crate::ai_shared::{
        AuthStyle, EndpointProber, ModelsFormat, ProbeError, ProbeRequest,
    };
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["localhost"], false));
    let base_url = format!("https://localhost:{}", addr.port());
    let error = crate::ai_openrouter::OpenRouterProber
        .probe(&ProbeRequest {
            base_url: &base_url,
            api_key: Some("sk-secret-key"),
            auth: AuthStyle::Bearer,
            models_path: "/models",
            models_format: ModelsFormat::Lenient,
            report_status: false,
        })
        .expect_err("the certificate is from no trusted root");
    assert_eq!(
        error,
        ProbeError::TlsUntrusted(TlsFailure::new("localhost", TlsCause::UnknownIssuer))
    );
}

// AAP-FR-LRTC, AAP-FR-HZTB
#[test]
fn the_relay_check_reports_a_refused_certificate() {
    use crate::relay_endpoint::{HttpRelayProbe, RelayProbe};
    let ca = private_ca();
    let addr = serve(&leaf_signed_by(&ca, &["localhost"], false));
    let error = HttpRelayProbe
        .health(&format!("https://localhost:{}/v1/health", addr.port()))
        .expect_err("the certificate is from no trusted root");
    assert_eq!(error, "tls_untrusted:unknown_issuer:localhost");
}

// AAP-FR-LRTC, AAP-FR-HZTB
#[test]
fn a_refused_certificate_reads_as_a_sentence_naming_the_host_and_the_cause() {
    for (cause, words) in [
        (TlsCause::UnknownIssuer, "issuer"),
        (TlsCause::Expired, "expired"),
        (TlsCause::HostnameMismatch, "host name"),
        (TlsCause::Other, "failed"),
    ] {
        let text = TlsFailure::new("api.github.com", cause).describe();
        assert!(text.contains("api.github.com"), "{text}");
        assert!(text.contains(words), "{text}");
    }
    assert_eq!(
        crate::github_publication::remotes::refusal_reason("tls_untrusted:expired:api.github.com"),
        TlsFailure::new("api.github.com", TlsCause::Expired).describe()
    );
    assert_eq!(
        crate::github_polling::records::error_text("tls_untrusted:expired:api.github.com"),
        TlsFailure::new("api.github.com", TlsCause::Expired).describe()
    );
}

// AAP-FR-LRTC, AAP-FR-HZTB
#[test]
fn an_error_of_a_lower_layer_that_is_the_typed_tls_error_is_kept() {
    let wire = "tls_untrusted:expired:api.github.com".to_string();
    assert_eq!(keep_tls(wire.clone(), "status_update_failed"), wire);
    assert_eq!(keep_tls("github_unreachable".into(), "status_update_failed"), "status_update_failed");
}

// AAP-FR-LRTC
#[test]
fn a_publication_remote_refused_by_the_tls_check_says_so_and_does_not_read_as_inaccessible() {
    use crate::github_publication::records::{RemoteEligibility, ERR_ISSUES_INACCESSIBLE};
    use crate::github_publication::remotes::refusal_reason;
    let eligibility = RemoteEligibility::TlsUntrusted;
    assert_eq!(eligibility.error_code(), ERR_TLS_UNTRUSTED);
    assert_ne!(eligibility.error_code(), ERR_ISSUES_INACCESSIBLE);
    assert!(refusal_reason(ERR_TLS_UNTRUSTED).contains("not trusted"));
}

/// A server for tests of other modules: a certificate for `names` from a fresh
/// private root, which no client trusts.
pub(crate) fn serve_for_tests(ca: &(rcgen::Certificate, KeyPair), names: &[&str]) -> SocketAddr {
    serve(&leaf_signed_by(ca, names, false))
}

pub(crate) fn private_ca_for_tests() -> (rcgen::Certificate, KeyPair) {
    private_ca()
}
