//! Frontend-free core for `apic`: the contract model and the parse, validate,
//! load/save, template, and Postman and OpenAPI import and Postman export logic shared by the CLI and
//! future frontends.

pub mod config;
pub mod convert;
pub mod converter;
pub mod edit;
pub mod env;
pub mod export;
pub mod file;
pub mod fuzzy;
pub mod http;
pub mod json;
mod openapi;
pub mod template;
pub mod validate;
pub mod vars;

pub use validate::validate_dir;
