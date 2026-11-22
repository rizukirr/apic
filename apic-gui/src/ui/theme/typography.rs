//! Text size tokens: the type scale every [`crate::ui::base::Text`] size maps
//! to. Body text uses egui's default size, so it has no token here.

/// Badges and status dots.
pub(crate) const SIZE_TINY: f32 = 10.0;

/// Sub-headings, chips, counts and other secondary labels.
pub(crate) const SIZE_CAPTION: f32 = 11.0;

/// Inline notes such as a load error under the URL bar.
pub(crate) const SIZE_SMALL: f32 = 12.0;

/// Column-header cell labels.
pub(crate) const SIZE_LABEL: f32 = 13.0;

/// Dialog titles.
pub(crate) const SIZE_TITLE: f32 = 15.0;

/// The title of an empty pane.
pub(crate) const SIZE_HEADING: f32 = 16.0;

/// The welcome screen's title.
pub(crate) const SIZE_DISPLAY: f32 = 22.0;

/// The `x` glyph on the row-removal button.
pub(crate) const SIZE_DELETE_GLYPH: f32 = 14.0;
