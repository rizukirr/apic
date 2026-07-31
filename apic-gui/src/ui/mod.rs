//! Presentation layer: the neon theme tokens, the reusable component toolkit,
//! and deferred focus plumbing.
//!
//! The dependency only ever points inward (components -> theme). Nothing in
//! this module tree may import from `crate::features` or name a domain type
//! such as `EditModel`; feature-specific rendering belongs in the feature's
//! own `view.rs`.

pub(crate) mod components;
pub(crate) mod focus;
pub(crate) mod sections;
pub(crate) mod syntax_highlighting;
pub(crate) mod theme;
