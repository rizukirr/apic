//! Frontend-free core for `apic`: the contract model and the parse, validate,
//! load/save, template, and Postman-convert logic shared by the CLI/TUI and
//! future frontends.

pub mod config;
pub mod convert;
pub mod converter;
pub mod file;
pub mod fuzzy;
pub mod json;
pub mod template;
