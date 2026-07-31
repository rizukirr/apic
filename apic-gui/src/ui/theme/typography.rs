//! Text size tokens. Sizes are the literals previously inlined at call sites;
//! naming them here is what lets components stop hard-coding numbers.

/// Column-header cell labels.
pub(crate) const SIZE_HEADER_LABEL: f32 = 10.0;

/// Sub-headings, chips, and other secondary labels.
pub(crate) const SIZE_SECONDARY: f32 = 11.0;

/// The `x` glyph on the row-removal button.
pub(crate) const SIZE_DELETE_GLYPH: f32 = 14.0;
