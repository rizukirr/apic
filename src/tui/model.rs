//! The editable working model now lives in [`apic_core::edit`] so every
//! front-end (this TUI and a future GUI) shares one definition. This module
//! re-exports it under the path the TUI has always used.

pub(crate) use apic_core::edit::model::*;
