//! Feature slices. Each subdirectory owns one feature's state and rendering.
//!
//! A slice may import from [`crate::ui`] and from `apic-core`. Slices must not
//! import each other; anything two features both need belongs in `crate::ui`
//! (if it renders) or in `crate::app` (if it orchestrates).

pub(crate) mod contracts;
