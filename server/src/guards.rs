//! Contract guards over the crate's own files.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-01, BMS-FR-02, BMS-FR-13, BMS-FR-17, BMS-FR-18,
//! BMS-FR-19, BMS-FR-20, BMS-FR-22, BMS-FR-23, BMS-FR-27.
//!
//! Some parts of the contract are properties of the manifest, the resolved
//! dependency graph, the image definition, or the documentation rather than of
//! the running code. A behavioural test cannot see them, so they are read here
//! instead. The module is compiled for tests only.
//!
//! Each guard below states what it actually proves. A guard that reads a file
//! for a word can only ever be a canary, and where that is all one is, it says
//! so rather than claiming the requirement.

use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::process::Command;

const MANIFEST: &str = include_str!("../Cargo.toml");
const LOCK_FILE: &str = include_str!("../Cargo.lock");
const DOCKERFILE: &str = include_str!("../Dockerfile");
const DOCKER_IGNORE: &str = include_str!("../.dockerignore");
const README: &str = include_str!("../README.md");

/// The crate's own directory.
fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every Rust source of the service, read from disk rather than listed.
///
/// A listed set stops covering the crate the moment a file is added, which is
/// exactly when a new capability would arrive. Two files are left out, and both
/// exemptions are named here rather than being silent:
///
/// - `guards.rs` holds the words the guards look for.
/// - `build_probe.rs` runs a command, which the build script needs and the
///   running service never calls.
fn service_sources() -> Vec<(String, String)> {
    const EXEMPT: [&str; 2] = ["guards.rs", "build_probe.rs"];

    let mut sources = Vec::new();
    collect_rust_files(&crate_dir().join("src"), &mut sources);
    sources.retain(|(name, _)| !EXEMPT.iter().any(|exempt| name.ends_with(exempt)));

    assert!(
        sources.len() >= 5,
        "the walk found {} sources, which is too few to be the whole crate",
        sources.len()
    );
    sources
}

fn collect_rust_files(directory: &Path, into: &mut Vec<(String, String)>) {
    let entries = std::fs::read_dir(directory).expect("the source directory is readable");

    for entry in entries {
        let path = entry.expect("the entry is readable").path();
        if path.is_dir() {
            collect_rust_files(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path).expect("the source is readable");
            into.push((path.display().to_string(), text));
        }
    }
}

/// A text with every run of spaces collapsed to one.
///
/// The README lays its tables and its surface block out in columns, so a line
/// there is padded. Collapsing the padding lets a guard check the content of a
/// line without also pinning how it is aligned.
fn squeezed(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_space = false;

    for character in text.chars() {
        let is_space = character == ' ';
        if !(is_space && last_was_space) {
            result.push(character);
        }
        last_was_space = is_space;
    }

    result
}

/// A source with its comment lines removed.
///
/// The guards are about what the crate does, not about what its prose says. A
/// deny-list run over the comments would fail the build on a doc comment that
/// names the thing it promises the crate does not do.
fn code_of(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The names the lock file declares, one per `name = "…"` line.
fn locked_package_names() -> Vec<String> {
    LOCK_FILE
        .lines()
        .filter_map(|line| line.trim().strip_prefix("name = "))
        .map(|value| value.trim().trim_matches('"').to_string())
        .collect()
}

/// The lines of the `Dockerfile` that are directives rather than comments.
fn dockerfile_directives() -> Vec<&'static str> {
    DOCKERFILE
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// The directives of the final stage alone.
fn final_stage_directives() -> Vec<&'static str> {
    let directives = dockerfile_directives();
    let start = directives
        .iter()
        .rposition(|line| line.starts_with("FROM "))
        .expect("the Dockerfile declares a final stage");
    directives[start..].to_vec()
}

// BMS-FR-01, BMS-FR-24, BMS-FR-02: the resolved dependency graph holds neither
// `synthesis_lib` nor Tauri, so the crate builds in a checkout that holds no
// src-tauri/ directory at all.
#[test]
fn the_dependency_graph_holds_neither_the_application_nor_tauri() {
    let names = locked_package_names();
    assert!(!names.is_empty(), "the lock file declares no package");

    for forbidden in [
        "tauri",
        "tauri-build",
        "synthesis",
        "synthesis_lib",
        "synthesis-lib",
    ] {
        assert!(
            !names.iter().any(|name| name == forbidden),
            "the lock file resolves `{forbidden}`, which BMS-FR-02 forbids"
        );
    }
}

/// The features cargo resolved for one package of the dependency graph.
///
/// Read from `cargo metadata`, which reports what the build actually enables
/// rather than what the manifest happens to spell. A manifest grep would pass
/// vacuously the moment the dependency is reformatted across lines, and it
/// cannot see a feature a second dependency turns on.
fn resolved_features(package: &str) -> Vec<String> {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(crate_dir())
        .output()
        .expect("cargo runs");

    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata prints JSON");

    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .expect("the resolve graph holds nodes");

    let node = nodes
        .iter()
        .find(|node| {
            node["id"]
                .as_str()
                .is_some_and(|id| id.contains(&format!("#{package}@")))
        })
        .unwrap_or_else(|| panic!("the graph holds no package named {package}"));

    node["features"]
        .as_array()
        .expect("a node reports its features")
        .iter()
        .map(|feature| feature.as_str().unwrap_or_default().to_string())
        .collect()
}

// BMS-FR-13, WSK-FR-KTRB: the WebSocket feature the three upgrades of
// `WSK-websocket.md` need is enabled, and no optional feature beyond the ones
// the service names is resolved. Asserted against the features the build
// resolves, which is what the scenario asks to enumerate.
#[test]
fn the_websocket_feature_is_enabled_and_no_other_optional_feature_is() {
    let axum = resolved_features("axum");
    assert!(!axum.is_empty(), "axum resolved no feature at all");
    assert!(
        axum.iter().any(|feature| feature == "ws"),
        "axum resolved no `ws` feature, so the three upgrades cannot be served: {axum:?}"
    );
    assert!(
        !axum.iter().any(|feature| feature == "default"),
        "axum was taken with its default features: {axum:?}"
    );
    for forbidden in ["multipart", "macros", "http2", "form", "original-uri"] {
        assert!(
            !axum.iter().any(|feature| feature == forbidden),
            "axum resolved the `{forbidden}` feature, which the service does not need: {axum:?}"
        );
    }
}

/// Names that would appear in code implementing a capability BMS-FR-27 forbids.
///
/// A canary, not a proof: a deny-list catches the obvious arrival of a
/// capability and cannot catch a determined one. The behavioural tests in
/// `router.rs`, `tests/api.rs`, and `tests/process.rs` are what actually hold
/// the surface shut. Authentication and authorization are no longer on the
/// list: `SAS-server-application-service.md` defines both, and the persistence
/// of `SAS-FR-IRWO` is a map in memory rather than a file or a database.
const FORBIDDEN_IN_SOURCE: [&str; 8] = [
    // Persistence to disk or to a database. The V1 store is memory alone
    // (SAS-FR-PDNU).
    "sqlx",
    "rusqlite",
    "std::fs::",
    "tokio::fs",
    "File::create",
    "File::open",
    // An outbound client to another service, and any other process.
    "reqwest",
    "TcpStream::connect",
];

// BMS-FR-27: the source holds no persistence to disk and no outbound client to
// another service.
#[test]
fn the_source_holds_no_capability_beyond_the_foundation() {
    for (name, source) in service_sources() {
        let code = code_of(&source);
        for pattern in FORBIDDEN_IN_SOURCE {
            assert!(
                !code.contains(pattern),
                "{name} holds `{pattern}`, which BMS-FR-27 forbids"
            );
        }
    }
}

// BMS-FR-27, SRB-FR-TOQF, WSK-FR-KTRB: the foundation and the relay boundary
// hold no upgrade handler of their own. Upgrade handling lives in the transport
// module of `WSK-websocket.md` and nowhere else.
#[test]
fn upgrade_handling_lives_in_the_websocket_transport_alone() {
    let transport = format!(
        "adapters{}relay{}ws{}",
        MAIN_SEPARATOR, MAIN_SEPARATOR, MAIN_SEPARATOR
    );
    let mut holders = 0;

    for (name, source) in service_sources() {
        let code = code_of(&source);
        let upgrades = ["WebSocketUpgrade", "on_upgrade", "upgrade::on"]
            .iter()
            .any(|pattern| code.contains(pattern));
        if !upgrades {
            continue;
        }
        assert!(
            name.contains(&transport),
            "{name} holds an upgrade handler outside the WebSocket transport"
        );
        holders += 1;
    }

    assert!(
        holders > 0,
        "no source holds an upgrade handler, so the three routes of WSK-FR-KTRB are not served"
    );
}

// SAS-FR-PMRB: no source holds the configured token, and no source writes one
// into a log record or an answer. The one literal token in the crate is the
// fixture of `testing.rs`, which a test presents to a service it started.
#[test]
fn no_source_writes_a_credential_into_a_record() {
    for (name, source) in service_sources() {
        if name.ends_with("testing.rs") {
            continue;
        }
        let code = code_of(&source);
        for pattern in ["token.0", "token.as_str()", "presented)"] {
            assert!(
                !code.contains(pattern),
                "{name} holds `{pattern}`, which could carry the token out of the process"
            );
        }
    }
}

/// Every way a router gains a path that this crate does not use.
///
/// A merge is how BMS-FR-04 composes the application routes into the one
/// constructor, so it is not on the list. Nesting, a service route, and a
/// fallback each answer a path that no route declares, which is what the
/// `404` of BMS-FR-13 is for.
const ROUTE_COMBINATORS: [&str; 5] = [
    ".nest(",
    ".nest_service(",
    ".route_service(",
    ".fallback(",
    ".method_not_allowed_fallback(",
];

// BMS-FR-13: the health route is the one route the foundation declares. Every
// other path reaches the router through the merge of the application routes,
// each of which authenticates its own request.
#[test]
fn the_router_declares_the_health_route_alone_and_gains_no_path_another_way() {
    let router = service_sources()
        .into_iter()
        .find(|(name, _)| name.ends_with("src/router.rs"))
        .map(|(_, source)| code_of(&source))
        .expect("the crate holds a router module");

    assert_eq!(
        router.matches(".route(").count(),
        1,
        "the foundation must declare exactly one route (BMS-FR-13)"
    );
    assert_eq!(
        router.matches(".merge(").count(),
        1,
        "the foundation merges the application routes once (BMS-FR-04)"
    );

    for combinator in ROUTE_COMBINATORS {
        assert!(
            !router.contains(combinator),
            "the router uses `{combinator}`, which adds a path BMS-FR-13 does not allow"
        );
    }
}

// SAS-FR-LFCA: every route module of the application surface reads the
// principal, so no route of it is served without a credential.
#[test]
fn every_application_route_module_reads_the_principal() {
    let modules = [
        "identity_routes.rs",
        "membership_routes.rs",
        "project_routes.rs",
        "content_routes.rs",
    ];
    for module in modules {
        let source = service_sources()
            .into_iter()
            .find(|(name, _)| name.ends_with(module))
            .map(|(_, source)| code_of(&source))
            .unwrap_or_else(|| panic!("the crate holds {module}"));

        let handlers = source.matches("async fn ").count();
        let principals = source.matches("principal: Principal").count();
        assert_eq!(
            handlers, principals,
            "{module} holds {handlers} handlers and {principals} of them read the principal"
        );
    }
}

// BMS-FR-17: the final stage's base is addressed by digest rather
// than by a tag alone.
#[test]
fn every_image_base_is_pinned_by_digest() {
    let bases: Vec<&str> = dockerfile_directives()
        .into_iter()
        .filter(|line| line.starts_with("FROM "))
        .collect();

    assert_eq!(bases.len(), 2, "the image is built in two stages");

    for base in &bases {
        assert!(
            base.contains("@sha256:"),
            "the base of `{base}` must be pinned by an immutable digest"
        );
    }

    let final_stage = bases.last().expect("the Dockerfile declares a final stage");
    assert!(
        final_stage.contains("distroless"),
        "the final stage must be a distroless base: {final_stage}"
    );
    assert!(
        !final_stage.contains(" AS "),
        "the final stage is the last one: {final_stage}"
    );
}

// BMS-FR-18, as far as a file can show it: the final stage adds nothing but the
// binary. What the base image itself holds — no shell, no package manager, the
// CA bundle — is a property of the built image and is not visible here.
#[test]
fn the_final_stage_adds_nothing_but_the_binary() {
    let final_stage = final_stage_directives();

    for directive in ["RUN ", "ADD "] {
        assert!(
            !final_stage.iter().any(|line| line.starts_with(directive)),
            "the final stage uses `{directive}`, which can add more than the binary"
        );
    }

    let copies: Vec<&&str> = final_stage
        .iter()
        .filter(|line| line.starts_with("COPY "))
        .collect();
    assert_eq!(copies.len(), 1, "the final stage copies one file");
    assert!(
        copies[0].contains("/usr/local/bin/synthesis-server"),
        "the one file copied is the service binary: {}",
        copies[0]
    );
    assert!(
        copies[0].contains("--from=build"),
        "the binary comes from the build stage: {}",
        copies[0]
    );
}

// BMS-FR-19: the entrypoint is the binary alone, the default port is
// declared, no volume is declared, and the container runs as the base's
// non-root user. Every assertion is scoped to the final stage: a port declared
// in the build stage reaches no shipped image.
#[test]
fn the_image_declares_the_entrypoint_the_port_the_user_and_no_volume() {
    let final_stage = final_stage_directives();

    let entrypoints: Vec<&&str> = final_stage
        .iter()
        .filter(|line| line.starts_with("ENTRYPOINT"))
        .collect();
    assert_eq!(entrypoints.len(), 1, "the image declares one entrypoint");
    assert_eq!(
        *entrypoints[0], "ENTRYPOINT [\"/usr/local/bin/synthesis-server\"]",
        "the entrypoint is the binary alone, with nothing wrapping it"
    );

    assert!(
        final_stage.contains(&"EXPOSE 8080"),
        "the final stage declares the default port of BMS-FR-05"
    );
    assert!(
        !final_stage.iter().any(|line| line.starts_with("VOLUME")),
        "the image declares no volume"
    );
    assert!(
        !final_stage.iter().any(|line| line.starts_with("CMD")),
        "no CMD may add arguments the container was not started with"
    );

    let users: Vec<&&str> = final_stage
        .iter()
        .filter(|line| line.starts_with("USER "))
        .collect();
    assert_eq!(users.len(), 1, "the final stage declares one user");
    assert_eq!(*users[0], "USER 65532:65532", "the base's non-root user");
}

// BMS-FR-20: nothing the image definition declares can carry a
// credential into a layer. A build argument and an environment variable are the
// two ways a value reaches the image, so the whole of the first is enumerated
// and the second is refused outright in the stage that ships.
#[test]
fn the_image_definition_can_carry_no_credential_into_a_layer() {
    let build_arguments: Vec<&str> = dockerfile_directives()
        .into_iter()
        .filter(|line| line.starts_with("ARG "))
        .collect();
    assert_eq!(
        build_arguments,
        vec!["ARG SYNTHESIS_BUILD_VERSION"],
        "the version is the only build argument the image takes"
    );

    let final_stage = final_stage_directives();
    for directive in ["ENV ", "ARG "] {
        assert!(
            !final_stage.iter().any(|line| line.starts_with(directive)),
            "the final stage declares `{directive}`, which puts a value in the image"
        );
    }

    // Key material named in a directive, as opposed to in prose. Scoped to the
    // directives so that a comment explaining the requirement cannot fail it.
    for directive in dockerfile_directives() {
        for pattern in ["-----BEGIN", ".pem", ".key", "docker/config.json", ".netrc"] {
            assert!(
                !directive.contains(pattern),
                "the directive `{directive}` names `{pattern}`"
            );
        }
    }
}

// BMS-FR-01: the crate is its own workspace root, and it declares the package
// the specification names.
#[test]
fn the_manifest_declares_the_package_the_specification_names() {
    assert!(MANIFEST.contains("name = \"synthesis-server\""));
    assert!(
        MANIFEST.contains("[[bin]]"),
        "the crate builds one named binary"
    );
    assert!(
        !crate_dir().join("../Cargo.toml").exists(),
        "the repository root must hold no Cargo manifest (BMS-FR-01)"
    );
}

// The build context is the crate. Without this the target directory of a
// developer's machine is sent to the daemon on every local image build.
//
// That the runner-local Rust pin is never committed as a second pin is checked
// by `src/test/ci-workflow.test.ts`, which asks git rather than reading an
// ignore file, and is deliberately not repeated here.
#[test]
fn the_build_context_excludes_the_build_directory() {
    assert!(DOCKER_IGNORE.contains("target/"));
}

// BMS-FR-22: the README documents the configuration, the health
// response, and the version-resolution order.
//
// Cross-checked against the constants the code compiles rather than against a
// word list. A word list passes whatever the README says; this fails the moment
// a default, a variable name, the route, or the drain period changes without
// the documentation following it, which is the drift BMS-FR-22 exists to stop.
#[test]
fn the_readme_agrees_with_the_configuration_the_code_holds() {
    use crate::config::{
        DEFAULT_HOST, DEFAULT_PORT, HOST_OPTION, HOST_VARIABLE, PORT_OPTION, PORT_VARIABLE,
    };
    use crate::router::HEALTH_PATH;
    use crate::shutdown::DRAIN_PERIOD;
    use crate::version_resolve::UNDEFINED_VERSION;

    // The two rows of the configuration table, whole. A bare substring would
    // survive a row that lost its default or named the wrong variable.
    for row in [
        format!("| Bind address | `{HOST_OPTION}` | `{HOST_VARIABLE}` | `{DEFAULT_HOST}` |"),
        format!("| Port | `{PORT_OPTION}` | `{PORT_VARIABLE}` | `{DEFAULT_PORT}` |"),
    ] {
        assert!(
            README.contains(&row),
            "server/README.md must hold the row {row}"
        );
    }

    // BMS-FR-13: both methods the route accepts are documented. Checked against
    // the README with its column padding collapsed, so aligning the block does
    // not fail the guard.
    let readme = squeezed(README);
    for expected in [format!("GET {HEALTH_PATH}"), format!("HEAD {HEALTH_PATH}")] {
        assert!(
            readme.contains(&expected),
            "server/README.md must document `{expected}` (BMS-FR-13, BMS-FR-22)"
        );
    }

    for expected in [
        "{\"version\":\"<resolved build version>\",".to_string(),
        // BMS-FR-JQZW: the two capability identifiers the answer advertises.
        "\"capabilities\":[\"remote_session\",\"websocket\"]".to_string(),
        format!("{} seconds", DRAIN_PERIOD.as_secs()),
        format!("`{UNDEFINED_VERSION}`"),
        "SYNTHESIS_BUILD_VERSION".to_string(),
        // The two exit statuses an operator has to tell apart.
        "status **2**".to_string(),
        "status **1**".to_string(),
    ] {
        assert!(
            README.contains(&expected),
            "server/README.md must document `{expected}` (BMS-FR-22)"
        );
    }
}

// BMS-FR-22: the README documents the image, cross-checked against
// the Dockerfile so the two cannot drift apart.
#[test]
fn the_readme_agrees_with_the_image_the_dockerfile_builds() {
    let final_stage = final_stage_directives();
    let user = final_stage
        .iter()
        .find(|line| line.starts_with("USER "))
        .and_then(|line| line.strip_prefix("USER "))
        .and_then(|value| value.split(':').next())
        .expect("the final stage declares a user");
    let port = final_stage
        .iter()
        .find(|line| line.starts_with("EXPOSE "))
        .and_then(|line| line.strip_prefix("EXPOSE "))
        .expect("the final stage declares a port");

    for expected in [
        "/usr/local/bin/synthesis-server",
        user,
        port,
        "linux/amd64",
        "linux/arm64",
        "docker build",
        "docker buildx build",
        "docker run",
    ] {
        assert!(
            README.contains(expected),
            "server/README.md must document `{expected}` (BMS-FR-22)"
        );
    }
}

// BMS-FR-23: the README documents both pipelines — their triggers,
// their job names, the required status check, and the token permissions each
// holds.
//
// The terms below are the ones that carry meaning. Bare words such as
// "release" or "publish" are deliberately absent: they are ordinary English in
// a document about publishing, and asserting them would assert only that the
// README is written in English.
#[test]
fn the_readme_documents_both_pipelines() {
    for expected in [
        // The check names, which branch protection consumes.
        "CI / server",
        "CI / gate",
        // The workflows and their job identifiers.
        ".github/workflows/ci.yml",
        ".github/workflows/server-image.yml",
        "Server image",
        // The trigger that separates the two postures.
        "types: [published]",
        // The permissions each workflow holds.
        "contents: read",
        "packages: write",
        "GITHUB_TOKEN",
        // Where the image goes.
        "ghcr.io/<owner>/<repository>",
    ] {
        assert!(
            README.contains(expected),
            "server/README.md must document `{expected}` (BMS-FR-23)"
        );
    }

    // The division the requirement is actually about: the crate is validated on
    // every pull request, and no check of the publishing workflow is required.
    assert!(
        README.contains("only the tag of the release") || README.contains("only tag pushed"),
        "the README must state that the release tag is the only tag pushed"
    );
    assert!(
        README
            .to_lowercase()
            .contains("no check of the publishing workflow is required")
            || README.contains("required by any branch-protection"),
        "the README must state that no check of the publishing workflow is required"
    );
}

// The non-functional logging rule of `SAS-server-application-service.md`, and
// `SRB-server-relay-boundary.md` SRB-FR-IZAB: the request record writes every
// path parameter of the matched route, so a parameter must be an identifier and
// nothing else. A route keyed by a handle, an email address, or a token would
// carry that value into a record the moment it was added, and no behavioural
// test of today's surface can see that. This guard reads the route tables and
// holds the naming rule the record depends on.
#[test]
fn every_path_parameter_of_the_surface_is_an_identifier() {
    let mut parameters = Vec::new();

    for (name, source) in service_sources() {
        for literal in code_of(&source).split('"') {
            if !literal.starts_with("/v1") {
                continue;
            }
            for segment in literal.split('/') {
                if let Some(parameter) = segment
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                {
                    parameters.push((name.clone(), parameter.to_string()));
                }
            }
        }
    }

    assert!(
        parameters.len() >= 10,
        "the walk found {} path parameters, which is too few to be the whole surface",
        parameters.len()
    );

    for (source, parameter) in parameters {
        assert!(
            parameter.ends_with("_id"),
            "the route parameter `{{{parameter}}}` of {source} is not an identifier; a request \
             record writes every path parameter, so a parameter must never carry a handle, an \
             email address, a token, or any other content"
        );
    }
}
