//! The field limits every write is checked against.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-TQIM.

use crate::domain::error::DomainError;

/// The longest name, title, or display name a record holds.
pub const NAME_LIMIT: usize = 200;
/// The longest email address a record holds.
pub const EMAIL_LIMIT: usize = 320;
/// The longest draft content or message body a record holds.
pub const CONTENT_LIMIT: usize = 1_000_000;
/// The page size a list route uses when the request names none.
pub const DEFAULT_PAGE_SIZE: usize = 100;
/// The largest page size a list route serves.
pub const MAXIMUM_PAGE_SIZE: usize = 500;

/// Reads a name-like field: present, not empty, and within the limit.
pub fn name(field: &str, value: &str) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::invalid_field(field, "the value is empty"));
    }
    if trimmed.chars().count() > NAME_LIMIT {
        return Err(DomainError::invalid_field(
            field,
            "the value is longer than 200 characters",
        ));
    }
    Ok(trimmed.to_string())
}

/// Reads an email address. The check is a shape check alone: the service sends
/// no mail and reads no address for identity (SAS-FR-TWEL).
pub fn email(field: &str, value: &str) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::invalid_field(field, "the value is empty"));
    }
    if trimmed.chars().count() > EMAIL_LIMIT {
        return Err(DomainError::invalid_field(
            field,
            "the value is longer than 320 characters",
        ));
    }
    let mut parts = trimmed.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err(DomainError::invalid_field(
            field,
            "the value holds no one @ with text on each side",
        ));
    }
    Ok(trimmed.to_string())
}

/// Reads a content field, which may be empty but not unbounded.
pub fn content(field: &str, value: &str) -> Result<String, DomainError> {
    if value.chars().count() > CONTENT_LIMIT {
        return Err(DomainError::invalid_field(
            field,
            "the value is longer than 1000000 characters",
        ));
    }
    Ok(value.to_string())
}

/// Reads a page size, and refuses one above the maximum.
pub fn page_size(value: Option<usize>) -> Result<usize, DomainError> {
    match value {
        None => Ok(DEFAULT_PAGE_SIZE),
        Some(0) => Err(DomainError::invalid_field("limit", "the value is zero")),
        Some(size) if size > MAXIMUM_PAGE_SIZE => Err(DomainError::invalid_field(
            "limit",
            "the value is above the maximum of 500",
        )),
        Some(size) => Ok(size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAS-FR-TQIM: an absent, empty, or over-long field is refused, and the
    // refusal names the field.
    #[test]
    fn a_name_is_trimmed_checked_and_named_in_its_refusal() {
        assert_eq!(name("name", "  Team  ").expect("a name is read"), "Team");
        let error = name("name", "   ").expect_err("an empty name is refused");
        assert_eq!(error.code(), "invalid_field");
        assert!(error.to_string().contains("name"), "{error}");
        assert!(name("name", &"a".repeat(NAME_LIMIT + 1)).is_err());
        assert!(name("name", &"a".repeat(NAME_LIMIT)).is_ok());
    }

    #[test]
    fn an_email_holds_one_at_with_text_on_each_side() {
        assert!(email("email", "person@example.com").is_ok());
        for refused in ["person", "@example.com", "person@", "a@b@c", ""] {
            assert!(email("email", refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn content_may_be_empty_and_may_not_be_unbounded() {
        assert_eq!(content("content", "").expect("empty content is read"), "");
        assert!(content("content", &"a".repeat(CONTENT_LIMIT + 1)).is_err());
    }

    #[test]
    fn the_page_size_defaults_and_is_capped() {
        assert_eq!(page_size(None).expect("the default"), DEFAULT_PAGE_SIZE);
        assert_eq!(page_size(Some(10)).expect("a size"), 10);
        assert!(page_size(Some(0)).is_err());
        assert!(page_size(Some(MAXIMUM_PAGE_SIZE + 1)).is_err());
    }
}
