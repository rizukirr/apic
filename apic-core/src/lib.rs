//! Frontend-free core for `apic`: the contract model and the parse, validate,
//! load/save, template, and Postman-convert logic shared by the CLI and
//! future frontends.

pub mod config;
pub mod convert;
pub mod converter;
pub mod edit;
pub mod env;
pub mod file;
pub mod fuzzy;
pub mod json;
pub mod template;
pub mod validate;
pub mod vars;

pub use validate::validate_dir;
