//! The spoken language of the author (VOP-FR-BYZC, VOP-FR-IRZC, VOP-FR-TSUS).

/// VOP-FR-IRZC: the language table, as code and English name.
pub(crate) const LANGUAGES: &[(&str, &str)] = &[
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("pl", "Polish"),
    ("nl", "Dutch"),
    ("tr", "Turkish"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("zh", "Chinese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
];

/// VOP-FR-BYZC: the longest free name, in characters.
const NAME_LIMIT: usize = 40;

/// VOP-FR-BYZC: one spoken language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpokenLanguage {
    /// The English name, which the prompt and the translation use.
    pub(crate) name: String,
    /// The code of the table, which transcription takes as a hint. `None`
    /// for a free name.
    pub(crate) code: Option<&'static str>,
}

/// VOP-FR-BYZC: a code of the table, or a free name of letters, spaces,
/// hyphens, and brackets. Each other value is no spoken language.
pub(crate) fn spoken_language(value: &str) -> Option<SpokenLanguage> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > NAME_LIMIT {
        return None;
    }
    if let Some((code, name)) = LANGUAGES
        .iter()
        .find(|(code, name)| code.eq_ignore_ascii_case(value) || name.eq_ignore_ascii_case(value))
    {
        return Some(SpokenLanguage { name: (*name).to_string(), code: Some(code) });
    }
    let allowed = value.chars().all(|c| c.is_alphabetic() || matches!(c, ' ' | '-' | '(' | ')'));
    allowed.then(|| SpokenLanguage { name: value.to_string(), code: None })
}

/// VOP-FR-TSUS: the code that a transcription request carries, or `None`.
pub(crate) fn transcription_hint(provider: &str, value: &str) -> Option<&'static str> {
    if provider == "openrouter" {
        return None;
    }
    spoken_language(value)?.code
}
