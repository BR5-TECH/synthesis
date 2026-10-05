//! The graduation concurrency limit (PSS-FR-JRWC, PSS-FR-KMBT, PSS-FR-FHQU).
//!
//! One project-wide limit on how many graduation runs hold a project slot at
//! the same time. Stream runs and direct runs count together
//! (`../graduation/GRD-graduation.md` GRD-FR-KKKN).

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The named value stored and sent for a limit with no cap (PSS-FR-FHQU).
pub const UNLIMITED: &str = "unlimited";

/// PSS-FR-JRWC: a positive integer, or the named value `unlimited`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraduationConcurrency {
    /// At most this many runs hold a slot. Always one or more.
    Limited(u32),
    /// No project-wide cap (GRD-FR-KPET).
    Unlimited,
}

impl Default for GraduationConcurrency {
    /// PSS-FR-JRWC: one run at a time is the resting state.
    fn default() -> Self {
        Self::Limited(1)
    }
}

impl GraduationConcurrency {
    /// A finite limit, never below one (PSS-FR-FHQU).
    pub fn limited(count: u32) -> Self {
        Self::Limited(count.max(1))
    }

    /// GRD-FR-KKKN, GRD-FR-IJKV: whether one more run may take a slot while
    /// `in_use` slots are held.
    ///
    /// A limit lowered below `in_use` answers `false` until enough slots are
    /// released, and it stops no run that holds one.
    pub fn permits(self, in_use: usize) -> bool {
        match self {
            Self::Unlimited => true,
            Self::Limited(limit) => in_use < limit as usize,
        }
    }

    /// GRD-FR-GRHC: whether every slot of a finite limit is held.
    pub fn is_full(self, in_use: usize) -> bool {
        !self.permits(in_use)
    }

    /// PSS-FR-KMBT: read the stored value, repairing what is malformed to one.
    ///
    /// A positive integer is read as it is, whether or not the Graduation
    /// section lists it.
    pub fn from_stored(value: Option<&toml::Value>) -> Self {
        match value {
            // An integer above what a count holds is read as the most it holds,
            // as the seam reads it, rather than repaired to one.
            Some(toml::Value::Integer(n)) if *n >= 1 => {
                Self::Limited(u32::try_from(*n).unwrap_or(u32::MAX))
            }
            Some(toml::Value::String(text)) if text == UNLIMITED => Self::Unlimited,
            _ => Self::default(),
        }
    }

    /// PSS-FR-FHQU: the value as the store holds it.
    pub fn to_stored(self) -> toml::Value {
        match self {
            Self::Limited(count) => toml::Value::Integer(i64::from(count.max(1))),
            Self::Unlimited => toml::Value::String(UNLIMITED.to_string()),
        }
    }
}

impl Serialize for GraduationConcurrency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Limited(count) => serializer.serialize_u32(*count),
            Self::Unlimited => serializer.serialize_str(UNLIMITED),
        }
    }
}

struct LimitVisitor;

impl Visitor<'_> for LimitVisitor {
    type Value = GraduationConcurrency;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a positive integer or \"{UNLIMITED}\"")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        let count = u32::try_from(value).unwrap_or(u32::MAX);
        Ok(GraduationConcurrency::limited(count))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        // PSS-FR-FHQU: an integer below one is written as one.
        let count = u32::try_from(value.max(1)).unwrap_or(u32::MAX);
        Ok(GraduationConcurrency::limited(count))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        if value == UNLIMITED {
            Ok(GraduationConcurrency::Unlimited)
        } else {
            Err(E::invalid_value(de::Unexpected::Str(value), &self))
        }
    }
}

impl<'de> Deserialize<'de> for GraduationConcurrency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(LimitVisitor)
    }
}
