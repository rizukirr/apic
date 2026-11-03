//! Minimal JSON syntax highlighter for the GUI.
//!
//! Produces an egui `LayoutJob` colored from the shared theme palette. JSON is
//! the only format the GUI displays (request bodies, schema examples), so this
//! deliberately supports nothing else and pulls in no dependency beyond egui.

use eframe::egui;
use egui::cache::{ComputerMut, FrameCache};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

use super::theme::Palette;

/// Highlight a (pretty-printed) JSON string into a `LayoutJob`.
///
/// `font_id` is the monospace font every token renders in and the colours come
/// from `p`. The job has no wrap
/// width set; callers (or [`highlight_json_cached`]) apply that afterward.
pub(crate) fn highlight_json(text: &str, font_id: FontId, p: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();

    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2, // skip the escaped character
                        b'"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                i = i.min(bytes.len());
                // A string is a key when the next non-space byte is ':'.
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let color = if j < bytes.len() && bytes[j] == b':' {
                    p.syntax_key
                } else {
                    p.syntax_string
                };
                append(&mut job, &text[start..i], &font_id, color);
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                append(&mut job, &text[start..i], &font_id, p.syntax_number);
            }
            b't' | b'f' | b'n' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                // Only the JSON literals are coloured as literals; any other bare word (e.g. the
                // "(no example)" placeholder) falls back to default text.
                let word = &text[start..i];
                let color = if matches!(word, "true" | "false" | "null") {
                    p.syntax_literal
                } else {
                    p.on_surface
                };
                append(&mut job, word, &font_id, color);
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                append(&mut job, &text[i..i + 1], &font_id, p.syntax_punctuation);
                i += 1;
            }
            _ => {
                let start = i;
                i += 1;
                while i < bytes.len() && !is_significant(bytes[i]) {
                    i += 1;
                }
                append(&mut job, &text[start..i], &font_id, p.on_surface);
            }
        }
    }

    job
}

#[derive(Default)]
struct JsonHighlighter;

impl ComputerMut<(&str, &FontId, egui::Theme), LayoutJob> for JsonHighlighter {
    fn compute(&mut self, (text, font_id, theme): (&str, &FontId, egui::Theme)) -> LayoutJob {
        highlight_json(text, font_id.clone(), super::theme::palette_for(theme))
    }
}

type JsonHighlightCache = FrameCache<LayoutJob, JsonHighlighter>;

/// Memoized [`highlight_json`]: caches the `LayoutJob` keyed by `(text, font, theme)`,
/// so a theme switch re-colours on the next frame and re-rendering unchanged
/// JSON every frame skips re-tokenizing. Wrap width is
/// not part of the key — the caller sets `job.wrap.max_width` on the result.
pub(crate) fn highlight_json_cached(
    ctx: &egui::Context,
    text: &str,
    font_id: &FontId,
) -> LayoutJob {
    let theme = ctx.theme();
    ctx.memory_mut(|mem| {
        mem.caches
            .cache::<JsonHighlightCache>()
            .get((text, font_id, theme))
            .clone()
    })
}

/// Bytes that begin a distinctly-colored token; a default run stops before them.
/// This set must mirror the non-`_` arms of the `match` in `highlight_json`.
fn is_significant(b: u8) -> bool {
    matches!(
        b,
        b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n' | b'{' | b'}' | b'[' | b']' | b':' | b','
    )
}

fn append(job: &mut LayoutJob, text: &str, font_id: &FontId, color: Color32) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font_id.clone(),
            color,
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::TOKYO_NIGHT_STORM as PALETTE;

    fn colors(job: &LayoutJob) -> Vec<Color32> {
        job.sections.iter().map(|s| s.format.color).collect()
    }

    #[test]
    fn colors_each_json_token_type() {
        let json =
            "{\n  \"name\": \"apic\",\n  \"count\": 3,\n  \"ok\": true,\n  \"extra\": null\n}";
        let job = highlight_json(json, FontId::monospace(12.0), &PALETTE);
        let cs = colors(&job);
        assert!(
            cs.contains(&PALETTE.syntax_key),
            "object keys should be syntax_key"
        );
        assert!(
            cs.contains(&PALETTE.syntax_string),
            "string values should be syntax_string"
        );
        assert!(
            cs.contains(&PALETTE.syntax_number),
            "numbers should be syntax_number"
        );
        assert!(
            cs.contains(&PALETTE.syntax_literal),
            "true/false/null should be syntax_literal"
        );
    }

    #[test]
    fn preserves_the_original_text_exactly() {
        let json = "{\n  \"nested\": [1, 2, {\"a\": false}]\n}";
        let job = highlight_json(json, FontId::monospace(12.0), &PALETTE);
        assert_eq!(job.text, json);
    }

    #[test]
    fn handles_non_json_without_panicking() {
        // Malformed / placeholder input must never panic and must round-trip the
        // text unchanged (byte indices are clamped, slices land on boundaries).
        for input in [
            "(no example)",
            "",
            "not json at all",
            "{\"a\": ",
            "\"unterminated",
            "1.5e-3 plus 字",
        ] {
            let job = highlight_json(input, FontId::monospace(12.0), &PALETTE);
            assert_eq!(job.text, input, "text must round-trip for {input:?}");
        }
    }

    #[test]
    fn bare_words_that_are_not_literals_are_not_red() {
        let job = highlight_json("(no example)", FontId::monospace(12.0), &PALETTE);
        assert!(
            !colors(&job).contains(&PALETTE.syntax_literal),
            "non-literal words must not be colored as JSON literals"
        );
    }

    #[test]
    fn highlighting_follows_the_palette() {
        let ctx = egui::Context::default();
        let font = FontId::monospace(12.0);
        let json = r#"{"a": "b"}"#;
        crate::ui::theme::install_themes(&ctx, egui::ThemePreference::Dark);
        let dark = colors(&highlight_json_cached(&ctx, json, &font));
        crate::ui::theme::install_themes(&ctx, egui::ThemePreference::Light);
        let light = colors(&highlight_json_cached(&ctx, json, &font));
        assert_ne!(dark, light);
    }
}
