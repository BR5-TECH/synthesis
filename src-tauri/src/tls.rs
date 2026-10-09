//! The one TLS trust setup of every outbound HTTP(S) connection.
//!
//! Specification: `specifications/core/AAP-ai-api-integrations.md`
//! (AAP-FR-WMCX, AAP-FR-QLNV, AAP-FR-MXJD, AAP-FR-HZTB, AAP-FR-PKWE).
//!
//! Every HTTP client in the application is built from the root list held here:
//! the `ureq` agents, the `reqwest` client that `rig` gets, and the `reqwest`
//! client that `openrouter-rs` gets. No client keeps its own default trust.
//!
//! The roots are the operating system store, read live on each start, plus the
//! bundled Mozilla roots. When the operating system store cannot be read, the
//! bundled roots stand alone. Validation of the chain and the host name is never
//! switched off: this module has no option for it.
//!
//! A failed TLS check is reported as the typed error `tls_untrusted`. It carries
//! the host and one cause code, and nothing else of the request.

use std::collections::HashSet;
use std::error::Error as StdError;
use std::sync::{Arc, OnceLock};

use crate::log_fields;
use crate::logging::{self, Domain};

/// AAP-FR-HZTB: the wire name of the typed error.
pub const ERR_TLS_UNTRUSTED: &str = "tls_untrusted";

/// AAP-FR-HZTB: why the TLS check refused the certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsCause {
    /// No root the application trusts signed the chain.
    UnknownIssuer,
    /// The certificate is past its validity period.
    Expired,
    /// The certificate is not valid for the host that was called.
    HostnameMismatch,
    /// Any other TLS failure.
    Other,
}

impl TlsCause {
    pub fn as_str(self) -> &'static str {
        match self {
            TlsCause::UnknownIssuer => "unknown_issuer",
            TlsCause::Expired => "expired",
            TlsCause::HostnameMismatch => "hostname_mismatch",
            TlsCause::Other => "other",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "unknown_issuer" => Some(TlsCause::UnknownIssuer),
            "expired" => Some(TlsCause::Expired),
            "hostname_mismatch" => Some(TlsCause::HostnameMismatch),
            "other" => Some(TlsCause::Other),
            _ => None,
        }
    }
}

/// AAP-FR-HZTB: the host and the cause of one TLS failure. It holds no key, no
/// token, no URL path, and no part of a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsFailure {
    pub host: String,
    pub cause: TlsCause,
}

impl TlsFailure {
    pub fn new(host: impl Into<String>, cause: TlsCause) -> Self {
        Self { host: host.into(), cause }
    }

    /// AAP-FR-PKWE: `tls_untrusted:<cause>:<host>`.
    pub fn wire(&self) -> String {
        format!("{ERR_TLS_UNTRUSTED}:{}:{}", self.cause.as_str(), self.host)
    }

    /// The inverse of [`TlsFailure::wire`]. `None` for any other text.
    pub fn parse_wire(text: &str) -> Option<Self> {
        let rest = text.strip_prefix(ERR_TLS_UNTRUSTED)?.strip_prefix(':')?;
        let (cause, host) = rest.split_once(':')?;
        if host.is_empty() {
            return None;
        }
        Some(Self { host: host.to_string(), cause: TlsCause::parse(cause)? })
    }
}

impl TlsFailure {
    /// The sentence a reader sees: the host and the cause. It holds nothing
    /// else of the request.
    pub fn describe(&self) -> String {
        let reason = match self.cause {
            TlsCause::UnknownIssuer => "the issuer of the certificate is unknown",
            TlsCause::Expired => "the certificate has expired",
            TlsCause::HostnameMismatch => "the certificate is not valid for this host name",
            TlsCause::Other => "the certificate check failed",
        };
        format!("The certificate of {} is not trusted: {reason}.", self.host)
    }
}

/// AAP-FR-PKWE: the host of a URL, without scheme, user information, port, or
/// path. An empty string where the text has no host.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if let Some(stripped) = authority.strip_prefix('[') {
        // An IPv6 literal keeps its colons and drops its brackets and port.
        return stripped.split(']').next().unwrap_or("").to_string();
    }
    authority.split(':').next().unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

fn cause_of_rustls(error: &rustls::Error) -> TlsCause {
    use rustls::CertificateError as C;
    match error {
        rustls::Error::InvalidCertificate(C::UnknownIssuer) => TlsCause::UnknownIssuer,
        rustls::Error::InvalidCertificate(C::Expired | C::ExpiredContext { .. }) => {
            TlsCause::Expired
        }
        rustls::Error::InvalidCertificate(C::NotValidForName | C::NotValidForNameContext { .. }) => {
            TlsCause::HostnameMismatch
        }
        _ => TlsCause::Other,
    }
}

/// AAP-FR-HZTB: find a TLS failure anywhere in an error and its sources.
///
/// An HTTP client wraps the TLS error several layers deep, often inside an
/// `io::Error`. `None` means the error holds no TLS failure, so the caller
/// classifies it as it did before. The error text is never read, because an
/// HTTP client's own text can echo the request it made.
pub fn classify_error(error: &(dyn StdError + 'static)) -> Option<TlsCause> {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    let mut depth = 0;
    while let Some(err) = current {
        if depth > 32 {
            return None;
        }
        depth += 1;
        if let Some(tls) = err.downcast_ref::<rustls::Error>() {
            return Some(cause_of_rustls(tls));
        }
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            if let Some(inner) = io.get_ref() {
                if let Some(cause) = classify_error(inner) {
                    return Some(cause);
                }
            }
        }
        current = err.source();
    }
    None
}

/// AAP-FR-HZTB: the TLS failure of a `ureq` error, or `None`.
pub fn classify_ureq(error: &ureq::Error) -> Option<TlsCause> {
    match error {
        ureq::Error::Rustls(tls) => Some(cause_of_rustls(tls)),
        ureq::Error::Io(io) => classify_error(io),
        _ => None,
    }
}

/// The TLS failure of a `ureq` error against `url`, ready to report.
pub fn ureq_failure(error: &ureq::Error, url: &str) -> Option<TlsFailure> {
    classify_ureq(error).map(|cause| TlsFailure::new(host_of(url), cause))
}

/// `error` where it is already the typed TLS error, or `fallback`. For a caller
/// that would otherwise replace the error of a lower layer with its own code.
pub fn keep_tls(error: String, fallback: &str) -> String {
    match TlsFailure::parse_wire(&error) {
        Some(_) => error,
        None => fallback.to_string(),
    }
}

/// The wire text of the TLS failure in a `ureq` error against `url`, or `None`.
pub fn ureq_wire(error: &ureq::Error, url: &str) -> Option<String> {
    ureq_failure(error, url).map(|failure| failure.wire())
}

// ---------------------------------------------------------------------------
// The roots
// ---------------------------------------------------------------------------

/// AAP-FR-QLNV / AAP-FR-MXJD: the DER roots every client trusts.
pub struct TrustRoots {
    ders: Vec<Vec<u8>>,
    os_roots: usize,
    os_errors: usize,
    bundled_roots: usize,
}

impl TrustRoots {
    /// Read the operating system store now, and add the bundled roots.
    pub fn load() -> Self {
        let native = rustls_native_certs::load_native_certs();
        let os = native.certs.iter().map(|c| c.as_ref().to_vec()).collect();
        Self::assemble(os, native.errors.len())
    }

    /// The roots from a given operating system list plus the bundled roots.
    pub fn assemble(os: Vec<Vec<u8>>, os_errors: usize) -> Self {
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        let mut ders: Vec<Vec<u8>> = Vec::new();
        let mut accepts = |der: Vec<u8>, ders: &mut Vec<Vec<u8>>| -> bool {
            if seen.contains(&der) {
                return false;
            }
            // A root the TLS library cannot parse would fail a client build, so
            // it is left out here and counted as unreadable by its caller.
            let mut probe = rustls::RootCertStore::empty();
            if probe.add(rustls::pki_types::CertificateDer::from(der.clone())).is_err() {
                return false;
            }
            seen.insert(der.clone());
            ders.push(der);
            true
        };
        let mut os_roots = 0;
        for der in os {
            if accepts(der, &mut ders) {
                os_roots += 1;
            }
        }
        let mut bundled_roots = 0;
        for cert in webpki_root_certs::TLS_SERVER_ROOT_CERTS {
            if accepts(cert.as_ref().to_vec(), &mut ders) {
                bundled_roots += 1;
            }
        }
        Self { ders, os_roots, os_errors, bundled_roots }
    }

    pub fn ders(&self) -> &[Vec<u8>] {
        &self.ders
    }

    /// How many roots came from the operating system store.
    pub fn os_roots(&self) -> usize {
        self.os_roots
    }

    /// How many bundled roots were added that the operating system did not hold.
    pub fn bundled_roots(&self) -> usize {
        self.bundled_roots
    }

    /// Whether the operating system store gave at least one root.
    pub fn os_store_readable(&self) -> bool {
        self.os_roots > 0
    }

    /// The `ureq` TLS configuration for these roots.
    pub fn ureq_tls_config(&self) -> ureq::tls::TlsConfig {
        let certs: Vec<ureq::tls::Certificate<'static>> = self
            .ders
            .iter()
            .map(|der| ureq::tls::Certificate::from_der(der).to_owned())
            .collect();
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::new_with_certs(&certs))
            .build()
    }

    /// The `rustls` client configuration for the `reqwest` clients that `rig`
    /// and `openrouter-rs` use. It validates the chain and the host name against
    /// these roots. It also records the cause of each refused certificate by
    /// host, because both libraries flatten the TLS error into text before the
    /// application sees it (see [`TlsRecord`]).
    fn rustls_config(
        &self,
        alpn: &[&[u8]],
        record: &TlsRecord,
    ) -> Result<rustls::ClientConfig, rustls::Error> {
        let mut store = rustls::RootCertStore::empty();
        for der in &self.ders {
            let _ = store.add(rustls::pki_types::CertificateDer::from(der.clone()));
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(store),
            provider.clone(),
        )
        .build()
        .map_err(|e| rustls::Error::General(e.to_string()))?;
        let mut config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(RecordingVerifier { inner, record: record.clone() }))
            .with_no_client_auth();
        config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
        Ok(config)
    }

    /// A `reqwest` 0.13 builder (the version `rig` uses) with these roots.
    pub fn reqwest013_builder(
        &self,
        record: &TlsRecord,
    ) -> Result<reqwest013::ClientBuilder, rustls::Error> {
        let config = self.rustls_config(&[b"h2", b"http/1.1"], record)?;
        Ok(reqwest013::Client::builder().tls_backend_preconfigured(config))
    }

    /// A `reqwest` 0.12 builder (the version `openrouter-rs` uses) with these
    /// roots.
    pub fn reqwest012_builder(
        &self,
        record: &TlsRecord,
    ) -> Result<reqwest012::ClientBuilder, rustls::Error> {
        let config = self.rustls_config(&[b"http/1.1"], record)?;
        Ok(reqwest012::Client::builder().use_preconfigured_tls(config))
    }
}

/// The certificate verifier of the `reqwest` clients. It hands every decision to
/// the standard `rustls` web PKI verifier and never accepts what that verifier
/// refuses. It only notes the cause of a refusal.
#[derive(Debug)]
struct RecordingVerifier {
    inner: Arc<rustls::client::WebPkiServerVerifier>,
    record: TlsRecord,
}

impl rustls::client::danger::ServerCertVerifier for RecordingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        intermediates: &[rustls::pki_types::CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let result =
            self.inner
                .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now);
        if let Err(error) = &result {
            self.record.set(TlsFailure::new(
                server_name.to_str().to_ascii_lowercase(),
                cause_of_rustls(error),
            ));
        }
        result
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// The refused certificate of one client, if its verifier refused one.
///
/// Each client is built with a record of its own, and a caller builds a client
/// per call. A refusal therefore belongs to that call alone, and two calls to
/// one host never read each other's.
#[derive(Clone, Debug, Default)]
pub struct TlsRecord(Arc<std::sync::Mutex<Option<TlsFailure>>>);

impl TlsRecord {
    pub fn new() -> Self {
        Self::default()
    }

    fn set(&self, failure: TlsFailure) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(failure);
    }

    /// AAP-FR-HZTB: the refusal this client's verifier noted, taken out of the
    /// record. A caller reads it after a call failed.
    pub fn take(&self) -> Option<TlsFailure> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// Note a refusal as the verifier would. For tests of the callers.
    #[cfg(test)]
    pub fn set_for_test(&self, failure: TlsFailure) {
        self.set(failure);
    }
}

static ROOTS: OnceLock<Arc<TrustRoots>> = OnceLock::new();

/// The roots of this run. The first call reads the operating system store; every
/// later call returns the same list.
pub fn roots() -> Arc<TrustRoots> {
    ROOTS.get_or_init(|| Arc::new(TrustRoots::load())).clone()
}

/// AAP-FR-MXJD: read the operating system store at application start, off the
/// start-up thread, and record what it gave.
pub fn load_at_start<S>(sink: &S)
where
    S: logging::LogSink + Clone + Send + 'static,
{
    let sink = sink.clone();
    std::thread::spawn(move || {
        let roots = roots();
        let readable = roots.os_store_readable();
        let fields = log_fields! {
            "osRoots" => roots.os_roots(),
            "osErrors" => roots.os_errors,
            "bundledRoots" => roots.bundled_roots(),
            "osStoreReadable" => readable,
        };
        if readable {
            logging::log_info(
                &sink,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "tls trust roots loaded",
                fields,
            );
        } else {
            logging::log_warn(
                &sink,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "operating system certificate store unreadable: bundled roots only",
                fields,
            );
        }
    });
}

// ---------------------------------------------------------------------------
// The clients
// ---------------------------------------------------------------------------

/// AAP-FR-WMCX: the `ureq` agent configuration every `ureq` call starts from.
pub fn ureq_config() -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    ureq::Agent::config_builder().tls_config(roots().ureq_tls_config())
}

/// AAP-FR-WMCX: the `reqwest` client `rig` is given, with the record of the
/// refusals its verifier notes.
pub fn rig_http_client() -> Result<(reqwest013::Client, TlsRecord), String> {
    let record = TlsRecord::new();
    let client = roots()
        .reqwest013_builder(&record)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    Ok((client, record))
}

/// AAP-FR-WMCX: the `reqwest` client `openrouter-rs` is given, with the record
/// of the refusals its verifier notes.
pub fn openrouter_http_client() -> Result<(reqwest012::Client, TlsRecord), String> {
    let record = TlsRecord::new();
    let client = roots()
        .reqwest012_builder(&record)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    Ok((client, record))
}

#[cfg(test)]
pub(crate) mod tests;
