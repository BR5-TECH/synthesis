//! The administrator identifier and the static bearer token.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-QJWD, SAS-FR-VKTP, SAS-FR-HGZL, SAS-FR-PMRB,
//! SAS-FR-BZUK.
//!
//! Both settings are read from the environment alone. A command line is
//! readable by every process on the host, so no option carries the token.

use std::ffi::OsString;
use std::fmt;

use crate::domain::ids::UserId;

/// The variable that supplies the administrator user identifier.
pub const ADMIN_ID_VARIABLE: &str = "SYNTHESIS_SERVER_ADMIN_ID";
/// The variable that supplies the static bearer token.
pub const TOKEN_VARIABLE: &str = "SYNTHESIS_SERVER_TOKEN";
/// The shortest token the service accepts (SAS-FR-HGZL).
pub const MINIMUM_TOKEN_LENGTH: usize = 16;

/// The configured token.
///
/// SAS-FR-PMRB: the value reaches no diagnostic, no log record, and no
/// response. The `Debug` implementation writes a fixed word, so a structure
/// that holds one cannot print it by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct StaticToken(String);

impl StaticToken {
    /// The token, as the configuration read it.
    pub fn new(value: String) -> Self {
        StaticToken(value)
    }

    /// Whether the presented credential is the configured one (SAS-FR-BZUK).
    ///
    /// The comparison reads every byte of the longer of the two values, so its
    /// duration does not depend on how many leading characters match.
    pub fn matches(&self, presented: &str) -> bool {
        constant_time_equal(self.0.as_bytes(), presented.as_bytes())
    }
}

impl fmt::Debug for StaticToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StaticToken(redacted)")
    }
}

/// Compares two values in a time that does not depend on their content.
fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let length = left.len().max(right.len());
    let mut difference: usize = left.len() ^ right.len();
    for index in 0..length {
        let one = left.get(index).copied().unwrap_or(0);
        let other = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(one ^ other);
    }
    difference == 0
}

/// The identity settings the service starts with.
#[derive(Debug, Clone)]
pub struct IdentityConfig {
    /// The administrator user the token authenticates (SAS-FR-XRPD).
    pub administrator_id: UserId,
    /// The static bearer token of V1.
    pub token: StaticToken,
}

/// A startup failure in the identity settings (SAS-FR-VKTP).
///
/// SAS-FR-PMRB: a variant names the variable and the accepted form. It never
/// holds the value that was refused, because one of the two values is a
/// credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityConfigError {
    /// The variable is absent, or holds nothing but white space.
    Missing { variable: &'static str },
    /// The variable holds a value the service cannot use.
    Invalid {
        variable: &'static str,
        accepted: &'static str,
    },
}

impl fmt::Display for IdentityConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityConfigError::Missing { variable } => write!(
                formatter,
                "the {variable} environment variable is needed and holds no value"
            ),
            IdentityConfigError::Invalid { variable, accepted } => write!(
                formatter,
                "the {variable} environment variable was refused, and the accepted form is {accepted}"
            ),
        }
    }
}

impl std::error::Error for IdentityConfigError {}

/// Reads both settings from the environment (SAS-FR-QJWD).
///
/// `variable` reads an environment variable, so a test supplies its own
/// environment. A value that is not valid Unicode is refused rather than read
/// as absent.
pub fn resolve<F>(variable: F) -> Result<IdentityConfig, IdentityConfigError>
where
    F: Fn(&str) -> Option<OsString>,
{
    let administrator = read(&variable, ADMIN_ID_VARIABLE)?;
    let token = read(&variable, TOKEN_VARIABLE)?;

    let administrator_id =
        UserId::parse(&administrator).map_err(|_| IdentityConfigError::Invalid {
            variable: ADMIN_ID_VARIABLE,
            accepted: "a UUID, for example 4f2c3a52-6c60-4d3e-8f10-9d2a4a5f6b71",
        })?;

    if token.chars().count() < MINIMUM_TOKEN_LENGTH {
        return Err(IdentityConfigError::Invalid {
            variable: TOKEN_VARIABLE,
            accepted: "a value of at least 16 characters",
        });
    }

    Ok(IdentityConfig {
        administrator_id,
        token: StaticToken::new(token),
    })
}

/// Reads one variable as text. White space around the value is removed, and a
/// value that is empty after that counts as absent.
fn read<F>(variable: &F, name: &'static str) -> Result<String, IdentityConfigError>
where
    F: Fn(&str) -> Option<OsString>,
{
    let value = variable(name).ok_or(IdentityConfigError::Missing { variable: name })?;
    let value = value
        .into_string()
        .map_err(|_| IdentityConfigError::Invalid {
            variable: name,
            accepted: "a value that is valid Unicode",
        })?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(IdentityConfigError::Missing { variable: name });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADMIN: &str = "4f2c3a52-6c60-4d3e-8f10-9d2a4a5f6b71";
    const TOKEN: &str = "0123456789abcdef0123";

    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        move |name: &str| {
            pairs
                .iter()
                .find(|(held, _)| held == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    // SAS-FR-QJWD: both settings are read from the environment.
    #[test]
    fn both_settings_are_read() {
        let config = resolve(environment(&[
            (ADMIN_ID_VARIABLE, ADMIN),
            (TOKEN_VARIABLE, TOKEN),
        ]))
        .expect("the settings are read");
        assert_eq!(config.administrator_id.to_string(), ADMIN);
        assert!(config.token.matches(TOKEN));
    }

    // SAS-FR-VKTP: a missing value is a startup failure.
    #[test]
    fn a_missing_value_is_refused() {
        for present in [
            vec![(TOKEN_VARIABLE, TOKEN)],
            vec![(ADMIN_ID_VARIABLE, ADMIN)],
            vec![],
            vec![(ADMIN_ID_VARIABLE, ADMIN), (TOKEN_VARIABLE, "   ")],
            vec![(ADMIN_ID_VARIABLE, "  "), (TOKEN_VARIABLE, TOKEN)],
        ] {
            let error = resolve(environment(&present)).expect_err("the settings are refused");
            assert!(
                matches!(error, IdentityConfigError::Missing { .. }),
                "{error:?}"
            );
        }
    }

    // SAS-FR-HGZL: the identifier is a UUID and the token is long enough.
    #[test]
    fn a_malformed_value_is_refused() {
        let error = resolve(environment(&[
            (ADMIN_ID_VARIABLE, "administrator"),
            (TOKEN_VARIABLE, TOKEN),
        ]))
        .expect_err("a malformed identifier is refused");
        assert!(matches!(
            error,
            IdentityConfigError::Invalid {
                variable: ADMIN_ID_VARIABLE,
                ..
            }
        ));

        let error = resolve(environment(&[
            (ADMIN_ID_VARIABLE, ADMIN),
            (TOKEN_VARIABLE, "short"),
        ]))
        .expect_err("a short token is refused");
        assert!(matches!(
            error,
            IdentityConfigError::Invalid {
                variable: TOKEN_VARIABLE,
                ..
            }
        ));
    }

    // SAS-FR-PMRB: no diagnostic and no debug output holds the token.
    #[test]
    fn no_diagnostic_holds_the_token() {
        let error = resolve(environment(&[
            (ADMIN_ID_VARIABLE, ADMIN),
            (TOKEN_VARIABLE, "short-secret"),
        ]))
        .expect_err("a short token is refused");
        let message = error.to_string();
        assert!(!message.contains("short-secret"), "{message}");
        assert!(!format!("{error:?}").contains("short-secret"));

        let config = resolve(environment(&[
            (ADMIN_ID_VARIABLE, ADMIN),
            (TOKEN_VARIABLE, TOKEN),
        ]))
        .expect("the settings are read");
        assert!(!format!("{config:?}").contains(TOKEN));
        assert!(
            !format!("{:?}", config.token).contains(TOKEN),
            "the token debug output holds the value"
        );
    }

    // SAS-FR-BZUK: the comparison reads every byte, and refuses a prefix, a
    // longer value, and an empty value.
    #[test]
    fn the_token_comparison_refuses_everything_but_the_token() {
        let token = StaticToken::new(TOKEN.to_string());
        assert!(token.matches(TOKEN));
        for refused in ["", "0", "0123456789abcdef012", "0123456789abcdef01234", "x"] {
            assert!(!token.matches(refused), "{refused}");
        }
    }

    #[test]
    fn the_comparison_is_over_bytes_and_not_over_length_alone() {
        assert!(constant_time_equal(b"abc", b"abc"));
        assert!(!constant_time_equal(b"abc", b"abd"));
        assert!(!constant_time_equal(b"abc", b"ab"));
        assert!(!constant_time_equal(b"", b"a"));
        assert!(constant_time_equal(b"", b""));
    }
}
