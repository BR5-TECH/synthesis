//! The advice an advisor returns (AGC-FR-GNMP): important points and key
//! questions.

/// AGC-FR-GNMP: what one advisor offers to the Meta turn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Advice {
    pub points: Vec<String>,
    pub questions: Vec<String>,
}

/// One advisor and its advice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdviceEntry {
    pub nickname: String,
    pub advice: Advice,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Points,
    Questions,
}

fn heading_of(line: &str) -> Option<Part> {
    let plain: String = line
        .trim()
        .trim_start_matches('#')
        .trim_matches(|c: char| c == '*' || c == '_' || c == ':' || c.is_whitespace())
        .to_lowercase();
    match plain.as_str() {
        "important points" | "points" | "key points" => Some(Part::Points),
        "key questions" | "questions" | "open questions" => Some(Part::Questions),
        _ => None,
    }
}

fn item_of(line: &str) -> String {
    let line = line.trim();
    let line = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("• "))
        .unwrap_or(line);
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    let line =
        if digits > 0 && line[digits..].starts_with(". ") { &line[digits + 2..] } else { line };
    line.trim().to_string()
}

/// AGC-FR-GNMP: read the reply of an advisor. The reply names its two lists with
/// the headings "Important points" and "Key questions". A reply without those
/// headings is read whole as important points, so no advice is lost.
pub fn parse_advice(text: &str) -> Advice {
    let mut advice = Advice::default();
    let mut part = Part::Points;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(found) = heading_of(line) {
            part = found;
            continue;
        }
        let item = item_of(line);
        if item.is_empty() {
            continue;
        }
        match part {
            Part::Points => advice.points.push(item),
            Part::Questions => advice.questions.push(item),
        }
    }
    advice
}

/// AGC-FR-MWYP: the body of the `advisors` section a Meta turn receives.
pub fn render_advisors_section(entries: &[AdviceEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&format!("@{}\n", entry.nickname));
        out.push_str("Important points:\n");
        for point in &entry.advice.points {
            out.push_str(&format!("- {point}\n"));
        }
        out.push_str("Key questions:\n");
        for question in &entry.advice.questions {
            out.push_str(&format!("- {question}\n"));
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // AGC-FR-GNMP: the two lists are read from their headings.
    #[test]
    fn reads_points_and_questions_from_their_headings() {
        let advice = parse_advice(
            "Important points:\n- Cost is high\n- Risk of delay\n\n**Key questions**\n1. Who owns it?\n* When?\n",
        );
        assert_eq!(advice.points, vec!["Cost is high", "Risk of delay"]);
        assert_eq!(advice.questions, vec!["Who owns it?", "When?"]);
    }

    // AGC-FR-GNMP: a reply with no heading is kept whole as points.
    #[test]
    fn a_reply_without_headings_becomes_points() {
        let advice = parse_advice("This is risky.\nCheck the budget.");
        assert_eq!(advice.points, vec!["This is risky.", "Check the budget."]);
        assert!(advice.questions.is_empty());
    }

    // AGC-FR-GNMP: an empty reply holds no advice.
    #[test]
    fn an_empty_reply_holds_nothing() {
        assert_eq!(parse_advice("  \n"), Advice::default());
    }

    // AGC-FR-MWYP: the section names each advisor by handle.
    #[test]
    fn the_section_names_each_advisor() {
        let body = render_advisors_section(&[AdviceEntry {
            nickname: "arch".into(),
            advice: Advice { points: vec!["p".into()], questions: vec!["q".into()] },
        }]);
        assert_eq!(body, "@arch\nImportant points:\n- p\nKey questions:\n- q");
    }
}
