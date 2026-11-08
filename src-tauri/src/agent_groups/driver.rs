//! Reading the driver out of the classifier's reply (AGC-FR-WPCZ, CVL-FR-VUDT).

/// CVL-FR-VUDT: the nickname on the first non-empty line of the reply, without
/// the `@` and without case, matched against the candidates. `None` where the
/// reply names none of them.
pub fn parse_driver(reply: &str, candidates: &[String]) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let word = line
        .trim_matches(|c: char| c == '`' || c == '*' || c == '"' || c == '\'' || c == '.')
        .trim_start_matches('@')
        .trim();
    candidates.iter().find(|candidate| candidate.eq_ignore_ascii_case(word)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<String> {
        vec!["arch".to_string(), "Sec".to_string()]
    }

    // CVL-FR-VUDT: the nickname is read without case and without the `@`.
    #[test]
    fn reads_the_nickname_from_the_first_line() {
        assert_eq!(parse_driver("@ARCH", &candidates()), Some("arch".to_string()));
        assert_eq!(
            parse_driver("\n  sec\nbecause it matters", &candidates()),
            Some("Sec".to_string())
        );
        assert_eq!(parse_driver("`@arch`.", &candidates()), Some("arch".to_string()));
    }

    // AGC-FR-RMUQ: a reply that names no candidate picks no driver.
    #[test]
    fn a_reply_naming_nobody_picks_no_driver() {
        assert_eq!(parse_driver("nobody", &candidates()), None);
        assert_eq!(parse_driver("", &candidates()), None);
        assert_eq!(parse_driver("arch and sec", &candidates()), None);
    }
}
