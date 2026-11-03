//! Feature slices. Each subdirectory owns one feature's state and rendering.
//!
//! A slice may import from [`crate::ui`] and from `apic-core`. Slices must not
//! import each other; anything two features both need belongs in `crate::ui`
//! (if it renders) or in `crate::app` (if it orchestrates).
//!
//! Inside a slice, code is layered as `domain`, `data` and `ui`. [`git`] documents the rules.

pub(crate) mod contracts;
pub(crate) mod git;
