//! Reusable egui primitives. Each consumes tokens from [`crate::ui::theme`] and
//! knows nothing about contracts, git, or any other feature.
//!
//! The dependency direction is one-way: components import theme, never the
//! reverse, and nothing here may import from `crate::features`.

pub(crate) mod button;
pub(crate) mod chip;
pub(crate) mod input;
pub(crate) mod layout;
pub(crate) mod sidebar;
pub(crate) mod table;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "no view uses the tab strip yet")
)]
pub(crate) mod tabs;
pub(crate) mod text;

pub(crate) use button::*;
pub(crate) use chip::*;
pub(crate) use input::*;
pub(crate) use layout::*;
pub(crate) use sidebar::*;
pub(crate) use table::*;
#[expect(unused_imports, reason = "no view uses the tab strip yet")]
pub(crate) use tabs::*;
pub(crate) use text::*;
