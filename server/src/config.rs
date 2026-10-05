//! The command line and the environment of the service.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-05, BMS-FR-06, BMS-FR-07.

use std::ffi::OsString;
use std::fmt;
use std::net::IpAddr;

/// The environment variable that supplies the bind address.
pub const HOST_VARIABLE: &str = "SYNTHESIS_SERVER_HOST";
/// The environment variable that supplies the port.
pub const PORT_VARIABLE: &str = "SYNTHESIS_SERVER_PORT";
/// The command-line option that supplies the bind address.
pub const HOST_OPTION: &str = "--host";
/// The command-line option that supplies the port.
pub const PORT_OPTION: &str = "--port";

/// The bind address the service uses when nothing else supplies one.
pub const DEFAULT_HOST: &str = "0.0.0.0";
/// The port the service uses when nothing else supplies one.
///
/// BMS-FR-06: the value is above 1024, so the non-root user of the image binds
/// it without an added capability.
pub const DEFAULT_PORT: u16 = 8080;

// BMS-FR-06: checked when the crate compiles, so a later edit that drops the
// default below the privileged range fails the build rather than a test.
const _: () = assert!(DEFAULT_PORT > 1024);

const HOST_FORM: &str = "an IP address, for example 0.0.0.0 or 127.0.0.1";
const PORT_FORM: &str = "a port number in 1..=65535";

/// The resolved settings the service starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    /// The address the listener binds.
    pub host: IpAddr,
    /// The port the listener binds.
    pub port: u16,
}

/// Where a setting came from. The diagnostic names it, so an operator knows
/// which of the two inputs to correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A command-line option supplied the value.
    Option(&'static str),
    /// An environment variable supplied the value.
    Variable(&'static str),
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Option(name) => write!(formatter, "the {name} option"),
            Origin::Variable(name) => write!(formatter, "the {name} environment variable"),
        }
    }
}

/// A startup failure in the configuration (BMS-FR-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A setting held a value the service cannot use.
    InvalidValue {
        /// The name of the setting, as the diagnostic reports it.
        setting: &'static str,
        /// Where the value came from.
        origin: Origin,
        /// The value that was refused.
        value: String,
        /// The form the setting accepts.
        accepted: &'static str,
    },
    /// An option was given with no value after it.
    MissingValue {
        /// The option that needs a value.
        option: &'static str,
        /// The form the setting accepts.
        accepted: &'static str,
    },
    /// An argument the service does not know.
    UnknownArgument {
        /// The argument that was refused.
        argument: String,
    },
    /// A value the operating system supplied that is not valid Unicode.
    ///
    /// The service reads its settings as text. A value it cannot read as text
    /// is refused here, because the alternative — `std::env::args`, which
    /// panics, and `std::env::var`, which reports the value as absent — either
    /// crashes the process or starts it on a setting the operator did not ask
    /// for.
    NotUnicode {
        /// What held the value, as the diagnostic names it.
        source: String,
        /// The value, with every part that is not Unicode replaced.
        value: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::InvalidValue {
                setting,
                origin,
                value,
                accepted,
            } => write!(
                formatter,
                "the {setting} setting was refused: {origin} holds \"{value}\", but the accepted form is {accepted}"
            ),
            ConfigError::MissingValue { option, accepted } => write!(
                formatter,
                "the {option} option needs a value, and the accepted form is {accepted}"
            ),
            ConfigError::UnknownArgument { argument } => write!(
                formatter,
                "the argument \"{argument}\" is unknown; the accepted arguments are {HOST_OPTION} <ip-address> and {PORT_OPTION} <port>"
            ),
            ConfigError::NotUnicode { source, value } => write!(
                formatter,
                "{source} holds a value that is not valid Unicode: \"{value}\""
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// The raw text of a setting, together with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawSetting {
    origin: Origin,
    value: String,
}

/// Resolves the settings from the arguments and the environment (BMS-FR-05).
///
/// The option wins over the variable, and the variable wins over the default.
/// Only the value that wins is checked, because a value that loses is never
/// used. A variable that holds an empty string counts as unset, which is how a
/// container runtime usually expresses "no value".
///
/// White space around a value is removed before the value is read, so a
/// runtime that pads a variable does not produce a startup failure.
///
/// `arguments` holds the arguments after the program name. `variable` reads an
/// environment variable, so a test supplies its own environment. Both are taken
/// as operating-system strings, because that is what the operating system
/// actually holds and a value that is not text must be refused rather than
/// crash the process or be read as absent.
pub fn resolve<F>(arguments: &[OsString], variable: F) -> Result<ServerConfig, ConfigError>
where
    F: Fn(&str) -> Option<OsString>,
{
    let arguments = to_text(arguments)?;
    let (host_option, port_option) = parse_arguments(&arguments)?;

    let host_setting = match host_option {
        Some(setting) => Some(setting),
        None => read_variable(&variable, HOST_VARIABLE)?,
    };
    let port_setting = match port_option {
        Some(setting) => Some(setting),
        None => read_variable(&variable, PORT_VARIABLE)?,
    };

    let host = match &host_setting {
        Some(setting) => {
            setting
                .value
                .parse::<IpAddr>()
                .map_err(|_| ConfigError::InvalidValue {
                    setting: "host",
                    origin: setting.origin,
                    value: setting.value.clone(),
                    accepted: HOST_FORM,
                })?
        }
        None => DEFAULT_HOST
            .parse::<IpAddr>()
            .expect("the default host is a valid IP address"),
    };

    let port = match &port_setting {
        Some(setting) => {
            let parsed = setting
                .value
                .parse::<u16>()
                .map_err(|_| invalid_port(setting))?;
            if parsed == 0 {
                return Err(invalid_port(setting));
            }
            parsed
        }
        None => DEFAULT_PORT,
    };

    Ok(ServerConfig { host, port })
}

fn invalid_port(setting: &RawSetting) -> ConfigError {
    ConfigError::InvalidValue {
        setting: "port",
        origin: setting.origin,
        value: setting.value.clone(),
        accepted: PORT_FORM,
    }
}

/// Reads the arguments as text.
///
/// An argument that is not valid Unicode is a startup failure rather than a
/// panic, so the process leaves through the path BMS-FR-07 describes.
fn to_text(arguments: &[OsString]) -> Result<Vec<String>, ConfigError> {
    arguments
        .iter()
        .map(|argument| {
            argument
                .clone()
                .into_string()
                .map_err(|refused| ConfigError::NotUnicode {
                    source: "the command line".to_string(),
                    value: refused.to_string_lossy().into_owned(),
                })
        })
        .collect()
}

/// Reads one environment variable.
///
/// A variable that holds nothing, or only white space, counts as unset, which
/// is how a container runtime usually expresses "no value". A variable that is
/// not valid Unicode is refused rather than read as absent, so a value the
/// operator did set never turns into the default.
fn read_variable<F>(variable: &F, name: &'static str) -> Result<Option<RawSetting>, ConfigError>
where
    F: Fn(&str) -> Option<OsString>,
{
    let Some(value) = variable(name) else {
        return Ok(None);
    };

    let value = value
        .into_string()
        .map_err(|refused| ConfigError::NotUnicode {
            source: format!("the {name} environment variable"),
            value: refused.to_string_lossy().into_owned(),
        })?;

    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    Ok(Some(RawSetting {
        origin: Origin::Variable(name),
        value: value.to_string(),
    }))
}

/// Reads `--host` and `--port` from the arguments.
///
/// Both the `--host 127.0.0.1` form and the `--host=127.0.0.1` form are
/// accepted. A repeated option keeps the last value, which is what a shell user
/// expects.
type ParsedOptions = (Option<RawSetting>, Option<RawSetting>);

fn parse_arguments(arguments: &[String]) -> Result<ParsedOptions, ConfigError> {
    let mut host: Option<RawSetting> = None;
    let mut port: Option<RawSetting> = None;

    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();

        let (option, accepted, slot) =
            if argument == HOST_OPTION || argument.starts_with(&format!("{HOST_OPTION}=")) {
                (HOST_OPTION, HOST_FORM, &mut host)
            } else if argument == PORT_OPTION || argument.starts_with(&format!("{PORT_OPTION}=")) {
                (PORT_OPTION, PORT_FORM, &mut port)
            } else {
                return Err(ConfigError::UnknownArgument {
                    argument: argument.to_string(),
                });
            };

        let value = match argument.split_once('=') {
            Some((_, inline)) => {
                index += 1;
                inline.to_string()
            }
            None => {
                let next = arguments
                    .get(index + 1)
                    .ok_or(ConfigError::MissingValue { option, accepted })?;
                index += 2;
                next.clone()
            }
        };

        // Trimmed on the same terms as a variable, so `--port " 9099"` from a
        // quoted shell expansion is read rather than refused. A value that held
        // only white space is a value that was not given.
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(ConfigError::MissingValue { option, accepted });
        }

        *slot = Some(RawSetting {
            origin: Origin::Option(option),
            value,
        });
    }

    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn no_variables(_: &str) -> Option<OsString> {
        None
    }

    fn variables(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), OsString::from(*value)))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    /// A value the operating system holds that is not valid Unicode.
    #[cfg(unix)]
    fn not_unicode() -> OsString {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff, 0xfe])
    }

    // BMS-FR-03, BMS-FR-05: with no option and no variable, the defaults apply.
    #[test]
    fn defaults_apply_when_nothing_is_supplied() {
        let config = resolve(&[], no_variables).expect("the defaults are valid");
        assert_eq!(config.host, DEFAULT_HOST.parse::<IpAddr>().unwrap());
        assert_eq!(config.port, 8080);
    }

    // BMS-FR-05: the variables apply when no option is given.
    #[test]
    fn variables_apply_when_no_option_is_given() {
        let config = resolve(
            &[],
            variables(&[(HOST_VARIABLE, "127.0.0.1"), (PORT_VARIABLE, "9099")]),
        )
        .expect("both values are valid");
        assert_eq!(config.host.to_string(), "127.0.0.1");
        assert_eq!(config.port, 9099);
    }

    // BMS-FR-05: the option wins over the variable.
    #[test]
    fn the_option_wins_over_the_variable() {
        let config = resolve(
            &arguments(&["--port", "9100"]),
            variables(&[(HOST_VARIABLE, "127.0.0.1"), (PORT_VARIABLE, "9099")]),
        )
        .expect("both values are valid");
        assert_eq!(config.host.to_string(), "127.0.0.1");
        assert_eq!(config.port, 9100);
    }

    #[test]
    fn the_host_option_wins_over_the_host_variable() {
        let config = resolve(
            &arguments(&["--host", "10.0.0.7"]),
            variables(&[(HOST_VARIABLE, "127.0.0.1")]),
        )
        .expect("both values are valid");
        assert_eq!(config.host.to_string(), "10.0.0.7");
    }

    #[test]
    fn the_inline_form_of_an_option_is_accepted() {
        let config = resolve(&arguments(&["--host=::1", "--port=9100"]), no_variables)
            .expect("both values are valid");
        assert_eq!(config.host.to_string(), "::1");
        assert_eq!(config.port, 9100);
    }

    #[test]
    fn a_repeated_option_keeps_the_last_value() {
        let config = resolve(
            &arguments(&["--port", "9100", "--port", "9200"]),
            no_variables,
        )
        .expect("both values are valid");
        assert_eq!(config.port, 9200);
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        let config = resolve(&[], variables(&[(HOST_VARIABLE, ""), (PORT_VARIABLE, "")]))
            .expect("an empty variable falls back to the default");
        assert_eq!(config.host.to_string(), DEFAULT_HOST);
        assert_eq!(config.port, DEFAULT_PORT);
    }

    // BMS-FR-07: a port that is not a number is refused, and the diagnostic
    // names the setting, the value, and the accepted form.
    #[test]
    fn a_port_that_is_not_a_number_is_refused() {
        let error = resolve(&[], variables(&[(PORT_VARIABLE, "not-a-port")]))
            .expect_err("the value is not a port");
        let message = error.to_string();
        assert!(message.contains("port"), "{message}");
        assert!(message.contains("not-a-port"), "{message}");
        assert!(message.contains("1..=65535"), "{message}");
        assert!(message.contains(PORT_VARIABLE), "{message}");
    }

    // BMS-FR-07: 0 is outside 1..=65535, so it is refused.
    #[test]
    fn port_zero_is_refused() {
        let error = resolve(&arguments(&["--port", "0"]), no_variables)
            .expect_err("0 is outside the accepted range");
        assert!(error.to_string().contains("port"), "{error}");
    }

    // BMS-FR-07: 65536 is outside 1..=65535, so it is refused.
    #[test]
    fn a_port_above_the_range_is_refused() {
        let error = resolve(&arguments(&["--port", "65536"]), no_variables)
            .expect_err("65536 is outside the accepted range");
        assert!(error.to_string().contains("65536"), "{error}");
    }

    #[test]
    fn the_highest_port_is_accepted() {
        let config = resolve(&arguments(&["--port", "65535"]), no_variables)
            .expect("65535 is inside the accepted range");
        assert_eq!(config.port, 65535);
    }

    // BMS-FR-07: a host that is not an IP address is refused the same way.
    #[test]
    fn a_host_that_is_not_an_ip_address_is_refused() {
        let error = resolve(&arguments(&["--host", "999.999.999.999"]), no_variables)
            .expect_err("the value is not an IP address");
        let message = error.to_string();
        assert!(message.contains("host"), "{message}");
        assert!(message.contains("999.999.999.999"), "{message}");
        assert!(message.contains("IP address"), "{message}");
        assert!(message.contains(HOST_OPTION), "{message}");
    }

    #[test]
    fn a_host_name_is_refused_because_the_service_resolves_no_name() {
        let error = resolve(&arguments(&["--host", "localhost"]), no_variables)
            .expect_err("a name is not an IP address");
        assert!(error.to_string().contains("localhost"), "{error}");
    }

    #[test]
    fn an_option_with_no_value_is_refused() {
        let error =
            resolve(&arguments(&["--port"]), no_variables).expect_err("the option needs a value");
        assert!(error.to_string().contains(PORT_OPTION), "{error}");
    }

    #[test]
    fn an_unknown_argument_is_refused() {
        let error =
            resolve(&arguments(&["--verbose"]), no_variables).expect_err("the argument is unknown");
        assert!(error.to_string().contains("--verbose"), "{error}");
    }

    // BMS-FR-07: an argument that is not valid Unicode is a startup failure
    // rather than the panic `std::env::args` would raise.
    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_unicode_is_refused() {
        let error =
            resolve(&[not_unicode()], no_variables).expect_err("the argument is not valid Unicode");

        assert!(matches!(error, ConfigError::NotUnicode { .. }), "{error:?}");
        assert!(error.to_string().contains("command line"), "{error}");
        assert!(error.to_string().contains("Unicode"), "{error}");
    }

    // BMS-FR-07: a variable that is not valid Unicode is refused rather than
    // read as absent, which would start the service on the default instead.
    #[cfg(unix)]
    #[test]
    fn a_variable_that_is_not_unicode_is_refused() {
        let refused = not_unicode();
        let error = resolve(&[], |name| (name == PORT_VARIABLE).then(|| refused.clone()))
            .expect_err("the variable is not valid Unicode");

        assert!(error.to_string().contains(PORT_VARIABLE), "{error}");
        assert!(error.to_string().contains("Unicode"), "{error}");
    }

    // White space around a value is removed, so a runtime that pads a variable
    // does not produce a startup failure.
    #[test]
    fn white_space_around_a_value_is_removed() {
        let config = resolve(
            &arguments(&["--host", " 127.0.0.1 "]),
            variables(&[(PORT_VARIABLE, "  9099  ")]),
        )
        .expect("both values are valid once trimmed");

        assert_eq!(config.host.to_string(), "127.0.0.1");
        assert_eq!(config.port, 9099);
    }

    // A variable holding only white space is a variable that was not set.
    #[test]
    fn a_variable_of_only_white_space_counts_as_unset() {
        let config = resolve(&[], variables(&[(PORT_VARIABLE, "   ")]))
            .expect("the variable falls back to the default");
        assert_eq!(config.port, DEFAULT_PORT);
    }

    // The inline form with nothing after the sign gives no value, and so does
    // the inline form holding only white space.
    #[test]
    fn an_inline_option_with_no_value_is_refused() {
        for argument in ["--port=", "--port= ", "--host=", "--host=  "] {
            let error = resolve(&arguments(&[argument]), no_variables)
                .expect_err("the option carries no value");
            assert!(
                matches!(error, ConfigError::MissingValue { .. }),
                "{argument}: {error:?}"
            );
        }
    }

    // The prefix match for the inline form must not swallow a longer argument
    // that merely starts with an option's letters.
    #[test]
    fn an_argument_that_only_starts_like_an_option_is_unknown() {
        for argument in ["--hostile", "--ports", "--host-name", "--portable=3"] {
            let error = resolve(&arguments(&[argument]), no_variables)
                .expect_err("the argument is unknown");
            assert!(
                matches!(error, ConfigError::UnknownArgument { .. }),
                "{argument}: {error:?}"
            );
        }
    }

    // The bracketed form is what a reader copies out of a URL. The setting takes
    // a bare address, and the diagnostic says so rather than failing to bind.
    #[test]
    fn the_bracketed_form_of_an_address_is_refused() {
        let error = resolve(&arguments(&["--host", "[::1]"]), no_variables)
            .expect_err("the bracketed form is not a bare IP address");
        assert!(error.to_string().contains("[::1]"), "{error}");
        assert!(error.to_string().contains("IP address"), "{error}");
    }

    // Last wins even when the last value is the one that is refused, so a
    // correct earlier value cannot mask a mistake in a later one.
    #[test]
    fn the_last_option_wins_even_when_it_is_refused() {
        let error = resolve(
            &arguments(&["--host", "0.0.0.0", "--host", "not-an-address"]),
            no_variables,
        )
        .expect_err("the last value is refused");
        assert!(error.to_string().contains("not-an-address"), "{error}");
    }

    // BMS-FR-05: a losing value is never used, so it is never checked either.
    #[test]
    fn an_invalid_variable_is_ignored_when_the_option_wins() {
        let config = resolve(
            &arguments(&["--port", "9100"]),
            variables(&[(PORT_VARIABLE, "not-a-port")]),
        )
        .expect("the option supplies the value that is used");
        assert_eq!(config.port, 9100);
    }
}
