use super::*;

fn comment(text: &str) -> Vec<InputPart> {
    vec![InputPart::Text(text.to_string()), InputPart::Text("\n".to_string())]
}

fn comments(n: usize, size: usize) -> Vec<Vec<InputPart>> {
    (0..n).map(|i| comment(&format!("c{i:02}:{}", "x".repeat(size - 4)))).collect()
}

// AGC-FR-HOND, AGC-FR-YQVB: comments that fit the bound less the tail stand
// whole, with no omitted line, and the section is not truncated.
#[test]
fn comments_within_the_room_stand_whole() {
    let section = bounded_history(String::new(), comments(3, 100), (String::new(), false), 1_000);
    assert!(!section.truncated);
    assert!(!section.body.contains("[Earlier comments omitted"));
    assert!(section.body.starts_with("c00:"));
    assert!(section.body.contains("c02:"));
}

// AGC-FR-XOZQ: above the bound, whole comments go oldest first, the newest
// stay, and one line counts what went.
#[test]
fn above_the_bound_the_oldest_comments_go_first() {
    // Ten comments of 101 characters each (with the line break), in 600.
    let section = bounded_history(String::new(), comments(10, 100), (String::new(), false), 600);
    assert!(section.truncated);
    // 600 less the omitted line leaves room for five comments.
    let line = omitted_comments_line(5);
    assert!(section.body.starts_with(&line), "{}", section.body);
    for kept in ["c05:", "c06:", "c07:", "c08:", "c09:"] {
        assert!(section.body.contains(kept), "{kept} stays");
    }
    assert!(!section.body.contains("c04:"));
    assert!(section.body.chars().count() <= 600);
}

// AGC-FR-XOZQ: the fragment anchor stays first, before the omitted line.
#[test]
fn the_anchor_stays_first() {
    let anchor = "This conversation is anchored to characters 1..5 of the file:\n\nquote\n\n";
    let section =
        bounded_history(anchor.to_string(), comments(10, 100), (String::new(), false), 600);
    assert!(section.body.starts_with(anchor));
    assert!(section.body[anchor.len()..].starts_with("[Earlier comments omitted:"));
    assert!(section.body.chars().count() <= 600);
}

// AGC-FR-XOZQ: where the newest comment alone does not fit, the section keeps
// the end of its text.
#[test]
fn a_newest_comment_too_long_keeps_its_end() {
    let mut list = comments(2, 100);
    list.push(comment(&format!("START{}END", "y".repeat(2_000))));
    let section = bounded_history(String::new(), list, (String::new(), false), 500);
    assert!(section.truncated);
    assert!(section
        .body
        .starts_with(&format!("{}{PARTIAL_COMMENT_LINE}", omitted_comments_line(2))));
    assert!(section.body.trim_end().ends_with("END"));
    assert!(!section.body.contains("START"));
    assert!(section.body.chars().count() <= 500);
}

// AGC-FR-YQVB: the comments fill the bound less the tail, and the tail ends
// the section.
#[test]
fn the_tail_ends_the_section_and_takes_its_room() {
    let tail = format!("\nTAIL{}\n", "t".repeat(195));
    let section = bounded_history(String::new(), comments(10, 100), (tail.clone(), false), 600);
    assert!(section.body.ends_with(&tail));
    // 600 less a 201-character tail leaves room for three comments.
    assert!(section.body.starts_with(&omitted_comments_line(7)));
    assert!(section.body.chars().count() <= 600);
}

// AGC-FR-MATQ: a tail that was cut makes the section truncated, also when
// every comment fits.
#[test]
fn a_cut_tail_marks_the_section_truncated() {
    let section = bounded_history(String::new(), comments(1, 10), ("\ntail\n".into(), true), 1_000);
    assert!(section.truncated);
}

// AGC-FR-HOND: the bound is the greater of `history_tokens × 4` and 40,000
// characters. AGC-FR-CLRX: `history_tokens` is the whole part of
// `window × history_percent / 100`.
#[test]
fn the_history_bound_follows_the_window_with_a_floor() {
    let budget = |window, history_percent| DocumentBudget { window, percent: 30, history_percent };
    assert_eq!(budget(1_000_000, 20).history_tokens(), 200_000);
    assert_eq!(budget(1_000_000, 20).history_bound_chars(), 800_000);
    assert_eq!(budget(999, 33).history_tokens(), 329);
    // 128,000 × 20 % × 4 = 102,400 characters.
    assert_eq!(budget(128_000, 20).history_bound_chars(), 102_400);
    // Below the floor the bound is 40,000 characters.
    assert_eq!(budget(10_000, 20).history_bound_chars(), SECTION_MAX_CHARS);
    assert_eq!(budget(50_000, 20).history_bound_chars(), SECTION_MAX_CHARS);
    assert_eq!(budget(50_005, 20).history_bound_chars(), 40_004);
}

// AGC-FR-CLRX: the turn resolves the History size limit setting at dispatch.
#[test]
fn the_budget_reads_the_history_percent() {
    let store = crate::global_settings::GlobalSettingsStore::in_memory();
    assert_eq!(DocumentBudget::resolve(&store, "custom", "m").history_percent, 20);
    store.save_history_limit_percent(35).unwrap();
    assert_eq!(DocumentBudget::resolve(&store, "custom", "m").history_percent, 35);
}

// AGC-FR-11: an anchor above the bound keeps its start, says it is partial,
// and the section stays within the bound.
#[test]
fn an_anchor_above_the_bound_is_cut() {
    let anchor = format!("ANCHOR{}", "a".repeat(50_000));
    let section = bounded_history(anchor, Vec::new(), (String::new(), false), 40_000);
    assert!(section.truncated);
    assert!(section.body.starts_with("ANCHOR"));
    assert!(section.body.contains("[This material is partial: it was truncated here.]"));
    assert!(section.body.chars().count() <= 40_000);
}

// AGC-FR-XOZQ, AGC-FR-YQVB: a tail that fills the whole bound leaves no room,
// and the line counts every comment as omitted.
#[test]
fn no_room_omits_every_comment() {
    let tail = "t".repeat(1_000);
    let section = bounded_history(String::new(), comments(3, 100), (tail.clone(), false), 1_000);
    assert!(section.truncated);
    assert!(section.body.starts_with(&omitted_comments_line(3)));
    assert!(!section.body.contains("c02:"));
    assert!(section.body.ends_with(&tail));
}

// AGC-FR-HOND: comments that fill the room exactly stand whole.
#[test]
fn comments_that_fill_the_room_exactly_stand_whole() {
    let list = comments(3, 100);
    let exact: usize = list.iter().map(|c| text_chars(c)).sum();
    let section = bounded_history(String::new(), list.clone(), (String::new(), false), exact);
    assert!(!section.truncated);
    let short = bounded_history(String::new(), list, (String::new(), false), exact - 1);
    assert!(short.truncated);
    assert!(short.body.starts_with("[Earlier comments omitted:"));
}

// AGC-FR-11, AGC-FR-XOZQ: one comment too long for the bound keeps its end and
// says in its own text that it is partial.
#[test]
fn a_single_comment_too_long_says_it_is_partial() {
    let list = vec![comment(&format!("START{}END", "y".repeat(2_000)))];
    let section = bounded_history(String::new(), list, (String::new(), false), 500);
    assert!(section.truncated);
    assert!(section.body.starts_with(PARTIAL_COMMENT_LINE));
    assert!(section.body.trim_end().ends_with("END"));
    assert!(section.body.chars().count() <= 500);
}
