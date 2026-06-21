//! Presentation layer: the neon theme, the reusable widget toolkit, and the
//! panelled sections built from them. `main.rs` (the `App`) drives these; the
//! dependency only ever points inward (sections -> widgets -> theme), never
//! back out to the application state.

pub(crate) mod sections;
pub(crate) mod theme;
pub(crate) mod widgets;
