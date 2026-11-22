//! Text: the [`Text`] builder every label in the app is drawn with, the two
//! standard labels built on it, and the variable-coloured layout for URLs.

use eframe::egui;

use crate::ui::theme::{
    Palette, SIZE_CAPTION, SIZE_DISPLAY, SIZE_HEADING, SIZE_LABEL, SIZE_SMALL, SIZE_TINY,
    SIZE_TITLE, SPACE_EXTRA_SMALL, palette,
};

/// A step on the type scale. Each maps to a token in
/// [`crate::ui::theme::typography`]; `Body` is egui's default size.
#[expect(
    dead_code,
    reason = "the full scale is the design; features use the rest as they move onto base"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Size {
    /// Badges and status dots.
    Tiny,
    /// Sub-headings, chips, counts and other secondary labels.
    Caption,
    /// Inline notes.
    Small,
    /// Ordinary text.
    #[default]
    Body,
    /// Column-header cell labels.
    Label,
    /// Dialog titles.
    Title,
    /// The title of an empty pane.
    Heading,
    /// The welcome screen's title.
    Display,
}

impl Size {
    /// The size in points, or `None` for egui's default body size.
    fn points(self) -> Option<f32> {
        match self {
            Size::Tiny => Some(SIZE_TINY),
            Size::Caption => Some(SIZE_CAPTION),
            Size::Small => Some(SIZE_SMALL),
            Size::Body => None,
            Size::Label => Some(SIZE_LABEL),
            Size::Title => Some(SIZE_TITLE),
            Size::Heading => Some(SIZE_HEADING),
            Size::Display => Some(SIZE_DISPLAY),
        }
    }
}

/// What a piece of text means, which picks its colour from the palette.
#[expect(
    dead_code,
    reason = "the full scale is the design; features use the rest as they move onto base"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tone {
    /// Ordinary text, `on_surface`.
    #[default]
    Default,
    /// Secondary text such as hints and labels, `on_surface_variant`.
    Muted,
    /// The accent, `primary`.
    Primary,
    /// Text on a `primary` fill.
    OnPrimary,
    /// Text on an `error` fill, `surface`.
    Surface,
    /// Errors and destructive actions.
    Error,
    /// Warnings and unsaved state.
    Warning,
    /// Success.
    Success,
    /// Information.
    Info,
}

impl Tone {
    /// The palette colour for this tone.
    pub(crate) fn color(self, p: &Palette) -> egui::Color32 {
        match self {
            Tone::Default => p.on_surface,
            Tone::Muted => p.on_surface_variant,
            Tone::Primary => p.primary,
            Tone::OnPrimary => p.on_primary,
            Tone::Surface => p.surface,
            Tone::Error => p.error,
            Tone::Warning => p.warning,
            Tone::Success => p.success,
            Tone::Info => p.info,
        }
    }
}

/// A piece of text styled only through theme tokens: a [`Size`], a
/// [`Tone`], and optionally bold or monospace. Build it, then draw it with
/// [`Text::show`] or turn it into a [`egui::RichText`] for a widget with
/// [`Text::rich`].
///
/// # Examples
///
/// ```ignore
/// Text::new("No contract open").size(Size::Heading).muted().show(ui);
/// ```
#[must_use]
#[derive(Debug, Clone)]
pub(crate) struct Text {
    text: String,
    size: Size,
    tone: Tone,
    strong: bool,
    mono: bool,
    hidden: bool,
}

impl Text {
    /// Body-sized text in the default tone.
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: Size::Body,
            tone: Tone::Default,
            strong: false,
            mono: false,
            hidden: false,
        }
    }

    /// Sets the size.
    pub(crate) fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Sets the tone.
    pub(crate) fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Shorthand for the [`Tone::Muted`] tone.
    pub(crate) fn muted(self) -> Self {
        self.tone(Tone::Muted)
    }

    /// Draws the text bold.
    pub(crate) fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    /// Draws the text in the monospace font.
    pub(crate) fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// Lays the text out but paints it transparent, so it keeps its space
    /// (and any menu it opens stays put) while it is not shown.
    pub(crate) fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// The text as a [`egui::RichText`], for a button or any widget that
    /// takes one.
    pub(crate) fn rich(self, ctx: &egui::Context) -> egui::RichText {
        self.rich_in(palette(ctx))
    }

    /// [`Text::rich`] with an explicit palette, for code that lays text out
    /// without a [`egui::Context`].
    pub(crate) fn rich_in(self, p: &Palette) -> egui::RichText {
        let color = if self.hidden {
            egui::Color32::TRANSPARENT
        } else {
            self.tone.color(p)
        };
        let mut rich = egui::RichText::new(self.text).color(color);
        if let Some(points) = self.size.points() {
            rich = rich.size(points);
        }
        if self.strong {
            rich = rich.strong();
        }
        if self.mono {
            rich = rich.monospace();
        }
        rich
    }

    /// Draws the text as a label.
    pub(crate) fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let rich = self.rich(ui.ctx());
        ui.label(rich)
    }
}

/// A muted caption-sized sub-heading with the standard trailing gap. Used
/// for every labelled block (`QUERY PARAMS`, `SCHEMA DEFINITION`, ...) so
/// they are all spaced identically.
pub(crate) fn section_label(ui: &mut egui::Ui, text: &str) {
    Text::new(text).size(Size::Caption).muted().show(ui);
    ui.add_space(SPACE_EXTRA_SMALL);
}

/// A muted, bold column-header cell label, drawn in the case the caller
/// passes.
pub(crate) fn header_label(ui: &mut egui::Ui, text: &str) {
    Text::new(text).size(Size::Label).muted().strong().show(ui);
}

/// Lays out `text` in `font` with every variable reference found by
/// [`apic_core::vars::tokens`] coloured: `$name`, `${name}`, `{{name}}` and
/// `{name}`. A reference whose name `defined` accepts is drawn in
/// `syntax_number`, one it rejects in `warning`, and the rest of the text in
/// `on_surface`. Only the text itself is drawn, never a variable's value.
///
/// # Arguments
///
/// * `text` - the URL or value to lay out
/// * `p` - the palette to colour from
/// * `font` - the font of every section
/// * `defined` - whether the selected environment defines a name
///
/// # Returns
///
/// The coloured layout job.
pub(crate) fn url_layout_job(
    text: &str,
    p: &Palette,
    font: egui::FontId,
    defined: &dyn Fn(&str) -> bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let format = |color| egui::TextFormat::simple(font.clone(), color);
    let mut at = 0;
    for token in apic_core::vars::tokens(text) {
        if token.range.start > at {
            job.append(&text[at..token.range.start], 0.0, format(p.on_surface));
        }
        let color = if defined(&token.name) {
            p.syntax_number
        } else {
            p.warning
        };
        job.append(&text[token.range.clone()], 0.0, format(color));
        at = token.range.end;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, format(p.on_surface));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour of the section covering `needle`, the first match in the
    /// job's text.
    fn color_of(job: &egui::text::LayoutJob, needle: &str) -> egui::Color32 {
        let start = job.text.find(needle).expect("needle is in the job text");
        let section = job
            .sections
            .iter()
            .find(|s| s.byte_range.contains(&egui::text::ByteIndex(start)))
            .expect("a section covers the needle");
        assert!(
            section.byte_range.end >= egui::text::ByteIndex(start + needle.len()),
            "{needle}"
        );
        section.format.color
    }

    #[test]
    fn every_variable_form_is_coloured_when_defined() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        let job = url_layout_job(
            "$a/${b}/{{c}}/{d}/auth/login",
            p,
            egui::FontId::monospace(12.0),
            &|_: &str| true,
        );
        for needle in ["$a", "${b}", "{{c}}", "{d}"] {
            assert_eq!(color_of(&job, needle), p.syntax_number, "{needle}");
        }
        assert_eq!(color_of(&job, "/auth/login"), p.on_surface);
    }

    #[test]
    fn an_undefined_name_is_drawn_in_the_warning_colour() {
        let p = &crate::ui::theme::TOKYO_NIGHT_STORM;
        let job = url_layout_job(
            "{{baseUrl}}/u/{id}",
            p,
            egui::FontId::monospace(12.0),
            &|name: &str| name == "baseUrl",
        );
        assert_eq!(color_of(&job, "{{baseUrl}}"), p.syntax_number);
        assert_eq!(color_of(&job, "{id}"), p.warning);
        assert_eq!(color_of(&job, "/u/"), p.on_surface);
    }
}
