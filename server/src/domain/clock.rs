//! The time a record carries.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-KDSM.
//!
//! The domain holds no clock of its own. A time reaches it from the application
//! layer through the [`Clock`] port, so a test stamps a record without waiting
//! and asserts on the exact value.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One RFC 3339 time, as a record carries it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub String);

impl Timestamp {
    /// The text of the time.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The outbound port that supplies the time.
pub trait Clock: Send + Sync {
    /// The time now, in RFC 3339, at UTC.
    fn now(&self) -> Timestamp;
}

/// The clock the running service uses.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let now = time::OffsetDateTime::now_utc();
        Timestamp(
            now.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| now.unix_timestamp().to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_clock_writes_an_rfc_3339_time() {
        let stamp = SystemClock.now();
        assert!(stamp.as_str().contains('T'), "{stamp}");
        assert!(stamp.as_str().ends_with('Z'), "{stamp}");
        assert!(stamp.as_str().len() >= 20, "{stamp}");
    }
}
