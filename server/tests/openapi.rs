//! The agreement between the OpenAPI document and the routes the service serves.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-JOAX.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The document the service publishes.
const DOCUMENT: &str = include_str!("../../api/openapi.yaml");

/// Every source that declares a route.
fn route_sources() -> Vec<String> {
    let mut sources = Vec::new();
    collect(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    );
    sources
}

fn collect(directory: &Path, into: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("the source directory is readable") {
        let path = entry.expect("the entry is readable").path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(std::fs::read_to_string(&path).expect("the source is readable"));
        }
    }
}

/// The paths the router declares, each with the methods it accepts.
fn routes_of_the_service() -> BTreeMap<String, BTreeSet<String>> {
    let mut routes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    // The health route is declared through the constant of BMS-FR-11 rather
    // than through a literal, and it answers `HEAD` as it answers `GET`.
    routes.insert(
        synthesis_server::router::HEALTH_PATH.to_string(),
        BTreeSet::from(["get".to_string()]),
    );

    for source in route_sources() {
        let mut rest = source.as_str();
        while let Some(start) = rest.find(".route(") {
            // rustfmt may break the line between the call and its literal, so
            // the white space is passed over rather than matched.
            let after = rest[start + ".route(".len()..].trim_start();
            let Some(after) = after.strip_prefix('"') else {
                rest = &rest[start + ".route(".len()..];
                continue;
            };
            let end = after.find('"').expect("the route literal is closed");
            let path = after[..end].to_string();
            if !path.starts_with('/') {
                // A guard that counts the calls of `.route(` holds the text
                // without holding a route.
                rest = &after[end..];
                continue;
            }

            // The methods of this entry stand between it and the next entry.
            let body = &after[end..];
            let body = match body.find(".route(") {
                Some(next) => &body[..next],
                None => body,
            };

            let methods = routes.entry(path).or_default();
            for method in ["get", "post", "patch", "delete"] {
                if body.contains(&format!("{method}(")) {
                    methods.insert(method.to_string());
                }
            }
            rest = &after[end..];
        }
    }
    routes
}

/// The paths the document declares, each with the methods it describes.
fn paths_of_the_document() -> BTreeMap<String, BTreeSet<String>> {
    let mut paths: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_paths = false;

    for line in DOCUMENT.lines() {
        if line.starts_with("paths:") {
            in_paths = true;
            continue;
        }
        if in_paths && line.starts_with("components:") {
            break;
        }
        if !in_paths {
            continue;
        }
        if let Some(path) = line
            .strip_prefix("  /")
            .and_then(|rest| rest.strip_suffix(':'))
        {
            current = Some(format!("/{path}"));
            paths.entry(format!("/{path}")).or_default();
            continue;
        }
        if let Some(method) = line
            .strip_prefix("    ")
            .and_then(|rest| rest.strip_suffix(':'))
        {
            if ["get", "post", "patch", "delete"].contains(&method) {
                if let Some(path) = current.as_ref() {
                    paths
                        .entry(path.clone())
                        .or_default()
                        .insert(method.to_string());
                }
            }
        }
    }
    paths
}

// SAS-FR-JOAX: the document describes every route the service serves, and
// describes no route it does not serve.
#[test]
fn the_document_and_the_router_declare_the_same_surface() {
    let routes = routes_of_the_service();
    let document = paths_of_the_document();

    assert!(routes.len() > 30, "the walk found {} routes", routes.len());

    let served: BTreeSet<&String> = routes.keys().collect();
    let described: BTreeSet<&String> = document.keys().collect();
    let missing: Vec<&&String> = served.difference(&described).collect();
    let extra: Vec<&&String> = described.difference(&served).collect();

    assert!(missing.is_empty(), "the document describes no {missing:?}");
    assert!(extra.is_empty(), "the service serves no {extra:?}");

    for (path, methods) in &routes {
        assert_eq!(
            methods,
            document.get(path).expect("the path is described"),
            "the methods of {path} differ"
        );
    }
}

// SAS-FR-LFCA: the document declares the bearer scheme, and the health route is
// the one operation that carries no security.
#[test]
fn the_document_declares_the_credential() {
    assert!(DOCUMENT.contains("bearerAuth:"), "the scheme is declared");
    assert!(DOCUMENT.contains("scheme: bearer"));
    assert_eq!(
        DOCUMENT.matches("security: []").count(),
        1,
        "the health route is the one unauthenticated operation"
    );
}

// SAS-FR-ZMPC: every error code the service reports is described.
#[test]
fn every_error_code_is_described() {
    for code in [
        "unauthenticated",
        "forbidden",
        "not_found",
        "invalid_field",
        "invalid_role",
        "invalid_permission",
        "duplicate_id",
        "duplicate_membership",
        "duplicate_grant",
        "owner_protected",
        "referenced",
        "stale_revision",
        "conversation_locked",
        "invitation_not_pending",
        "internal",
    ] {
        assert!(
            DOCUMENT.contains(&format!("- {code}")),
            "the document describes no {code} code"
        );
    }
}

// SAS-FR-IJRO: the fifteen permissions of the service are the fifteen the
// document names.
#[test]
fn every_permission_is_described() {
    for permission in synthesis_server::domain::permissions::Permission::ALL {
        assert!(
            DOCUMENT.contains(&format!("- {}", permission.as_str())),
            "the document names no {permission} permission"
        );
    }
}
