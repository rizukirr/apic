//! Merge conflict markers as plain data, and the parser and renderer that
//! move between marked-up text and that data.
//!
//! No git, no egui, no filesystem. `parse` reads what `git merge` wrote to a
//! file, `render` writes a file back out from resolved choices. Getting
//! `render` wrong corrupts a user's file in a way nothing downstream would
//! notice, so every byte outside a conflict block is passed through
//! unchanged, and every byte inside a chosen side is passed through
//! unchanged too.

/// One piece of a conflicted file: plain text that survives untouched, or a
/// conflict block with two labeled sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    Text(String),
    Conflict {
        /// The label git wrote after `<<<<<<<`, normally `HEAD`.
        ours_label: String,
        /// The label git wrote after `>>>>>>>`, normally the branch name.
        theirs_label: String,
        ours: String,
        theirs: String,
    },
}

/// A file split into text and conflict segments, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConflictFile {
    pub(crate) segments: Vec<Segment>,
}

/// Which side to keep when rendering one conflict block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    Ours,
    Theirs,
    Both,
}

/// The parser's state as it walks the file line by line.
enum State {
    Outside,
    InOurs,
    InTheirs,
}

/// Splits `text` into text and conflict segments.
///
/// Returns `None` when there are no conflict markers at all, or when the
/// markers are unbalanced: a `<<<<<<<` without a matching `=======` and
/// `>>>>>>>`, or a `=======` or `>>>>>>>` with no `<<<<<<<` open. A malformed
/// file is declined rather than guessed at, so the caller can fall back to
/// the read-only view instead of writing a wrong file.
pub(crate) fn parse(text: &str) -> Option<ConflictFile> {
    let mut segments = Vec::new();
    let mut saw_marker = false;

    let mut state = State::Outside;
    let mut text_buf = String::new();
    let mut ours_label = String::new();
    let mut ours_buf = String::new();
    let mut theirs_buf = String::new();

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        match state {
            State::Outside => {
                if let Some(label) = trimmed.strip_prefix("<<<<<<<") {
                    saw_marker = true;
                    if !text_buf.is_empty() {
                        segments.push(Segment::Text(std::mem::take(&mut text_buf)));
                    }
                    ours_label = label.trim().to_string();
                    state = State::InOurs;
                } else if trimmed.starts_with("=======") || trimmed.starts_with(">>>>>>>") {
                    return None; // end marker without a matching start
                } else {
                    text_buf.push_str(line);
                }
            }
            State::InOurs => {
                if trimmed == "=======" {
                    state = State::InTheirs;
                } else if trimmed.starts_with("<<<<<<<") {
                    return None; // nested start before this block ended
                } else {
                    ours_buf.push_str(line);
                }
            }
            State::InTheirs => {
                if let Some(label) = trimmed.strip_prefix(">>>>>>>") {
                    segments.push(Segment::Conflict {
                        ours_label: std::mem::take(&mut ours_label),
                        theirs_label: label.trim().to_string(),
                        ours: std::mem::take(&mut ours_buf),
                        theirs: std::mem::take(&mut theirs_buf),
                    });
                    state = State::Outside;
                } else if trimmed.starts_with("<<<<<<<") {
                    return None; // nested start before this block ended
                } else {
                    theirs_buf.push_str(line);
                }
            }
        }
    }

    if !matches!(state, State::Outside) {
        return None; // start marker with no matching end
    }
    if !saw_marker {
        return None;
    }
    if !text_buf.is_empty() {
        segments.push(Segment::Text(text_buf));
    }

    Some(ConflictFile { segments })
}

/// Rebuilds a file from `file`, resolving each conflict block in order with
/// the matching entry in `choices`. This is the only function that writes a
/// resolved file, and the only function that renders a preview of one, so the
/// preview shown before Resolve is pressed can never drift from what Resolve
/// actually writes.
///
/// Text segments pass through unchanged. A decided conflict becomes the
/// chosen side, or for `Choice::Both` the ours text followed by the theirs
/// text, in that order, not interleaved and not deduplicated. An undecided
/// block (`None`, or a block past the end of `choices`) renders as its
/// original conflict, markers included, so a preview built from a partially
/// decided file shows plainly what is still unresolved. Line endings inside a
/// segment are whatever `parse` captured, so they are preserved exactly.
pub(crate) fn render(file: &ConflictFile, choices: &[Option<Choice>]) -> String {
    let mut out = String::new();
    let mut choice_idx = 0;

    for segment in &file.segments {
        match segment {
            Segment::Text(text) => out.push_str(text),
            Segment::Conflict {
                ours_label,
                theirs_label,
                ours,
                theirs,
            } => {
                let choice = choices.get(choice_idx).copied().flatten();
                choice_idx += 1;
                match choice {
                    Some(Choice::Ours) => out.push_str(ours),
                    Some(Choice::Theirs) => out.push_str(theirs),
                    Some(Choice::Both) => {
                        out.push_str(ours);
                        out.push_str(theirs);
                    }
                    None => {
                        out.push_str("<<<<<<< ");
                        out.push_str(ours_label);
                        out.push('\n');
                        out.push_str(ours);
                        out.push_str("=======\n");
                        out.push_str(theirs);
                        out.push_str(">>>>>>> ");
                        out.push_str(theirs_label);
                        out.push('\n');
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_markers_gives_none() {
        let text = "{\n  \"name\": \"main\"\n}\n";
        assert_eq!(parse(text), None);
    }

    #[test]
    fn single_block_round_trips_ours() {
        let text = "{\n<<<<<<< HEAD\n  \"name\": \"main\",\n=======\n  \"name\": \"side\",\n>>>>>>> side\n  \"method\": \"GET\"\n}\n";
        let file = parse(text).expect("markers present");
        let rendered = render(&file, &[Some(Choice::Ours)]);
        assert_eq!(
            rendered,
            "{\n  \"name\": \"main\",\n  \"method\": \"GET\"\n}\n"
        );
    }

    #[test]
    fn single_block_round_trips_theirs() {
        let text = "{\n<<<<<<< HEAD\n  \"name\": \"main\",\n=======\n  \"name\": \"side\",\n>>>>>>> side\n  \"method\": \"GET\"\n}\n";
        let file = parse(text).expect("markers present");
        let rendered = render(&file, &[Some(Choice::Theirs)]);
        assert_eq!(
            rendered,
            "{\n  \"name\": \"side\",\n  \"method\": \"GET\"\n}\n"
        );
    }

    #[test]
    fn single_block_round_trips_both() {
        let text = "{\n<<<<<<< HEAD\n  \"name\": \"main\",\n=======\n  \"name\": \"side\",\n>>>>>>> side\n  \"method\": \"GET\"\n}\n";
        let file = parse(text).expect("markers present");
        let rendered = render(&file, &[Some(Choice::Both)]);
        assert_eq!(
            rendered,
            "{\n  \"name\": \"main\",\n  \"name\": \"side\",\n  \"method\": \"GET\"\n}\n"
        );
    }

    #[test]
    fn undecided_block_round_trips_to_its_original_marker_text() {
        let text = "{\n<<<<<<< HEAD\n  \"name\": \"main\",\n=======\n  \"name\": \"side\",\n>>>>>>> side\n  \"method\": \"GET\"\n}\n";
        let file = parse(text).expect("markers present");
        let rendered = render(&file, &[None]);
        assert_eq!(rendered, text);
    }

    #[test]
    fn two_blocks_resolve_independently() {
        let text = "a\n<<<<<<< HEAD\nours-1\n=======\ntheirs-1\n>>>>>>> side\nb\n<<<<<<< HEAD\nours-2\n=======\ntheirs-2\n>>>>>>> side\nc\n";
        let file = parse(text).expect("markers present");
        let rendered = render(&file, &[Some(Choice::Ours), Some(Choice::Theirs)]);
        assert_eq!(rendered, "a\nours-1\nb\ntheirs-2\nc\n");
    }

    #[test]
    fn unbalanced_start_without_end_gives_none() {
        let text = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n";
        assert_eq!(parse(text), None);
    }

    #[test]
    fn unbalanced_end_without_start_gives_none() {
        let text = "a\n=======\ntheirs\n>>>>>>> side\n";
        assert_eq!(parse(text), None);
    }

    #[test]
    fn labels_are_preserved() {
        let text = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature/side-branch\n";
        let file = parse(text).expect("markers present");
        match &file.segments[0] {
            Segment::Conflict {
                ours_label,
                theirs_label,
                ..
            } => {
                assert_eq!(ours_label, "HEAD");
                assert_eq!(theirs_label, "feature/side-branch");
            }
            other => panic!("expected a conflict segment, got {other:?}"),
        }
    }

    #[test]
    fn text_outside_blocks_survives_byte_for_byte() {
        let text = "line one\n\n  line three with trailing spaces   \n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> side\n\nlast line, no newline at end";
        let file = parse(text).expect("markers present");
        let rendered = render(&file, &[Some(Choice::Ours)]);
        assert_eq!(
            rendered,
            "line one\n\n  line three with trailing spaces   \nours\n\nlast line, no newline at end"
        );
    }

    #[test]
    fn render_of_unmodified_parse_taking_ours_reproduces_ours_side() {
        let text = "{\n<<<<<<< HEAD\n  \"name\": \"main\",\n=======\n  \"name\": \"side\",\n>>>>>>> side\n  \"method\": \"GET\"\n}\n";
        let file = parse(text).expect("markers present");
        let choices: Vec<Option<Choice>> = file
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Conflict { .. }))
            .map(|_| Some(Choice::Ours))
            .collect();
        let rendered = render(&file, &choices);
        assert_eq!(
            rendered,
            "{\n  \"name\": \"main\",\n  \"method\": \"GET\"\n}\n"
        );
    }
}
