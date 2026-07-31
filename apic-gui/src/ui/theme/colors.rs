//! Color tokens. The whole GUI draws from these so the look stays uniform.
//! Nothing here knows about contracts, git, or any other feature.

use eframe::egui::Color32;

// Terminal-green identity, toned down for a calmer, cleaner surface.
pub(crate) const BG: Color32 = Color32::from_rgb(15, 20, 17);
pub(crate) const PANEL_BG: Color32 = Color32::from_rgb(20, 27, 23);
pub(crate) const BORDER: Color32 = Color32::from_rgb(45, 66, 54);
pub(crate) const GREEN: Color32 = Color32::from_rgb(78, 201, 138);
pub(crate) const CYAN: Color32 = Color32::from_rgb(120, 190, 230);
pub(crate) const DIM: Color32 = Color32::from_rgb(128, 150, 138);
pub(crate) const TEXT: Color32 = Color32::from_rgb(205, 225, 213);
pub(crate) const RED: Color32 = Color32::from_rgb(230, 110, 110);
pub(crate) const AMBER: Color32 = Color32::from_rgb(224, 176, 60);

/// The darkest surface, used behind code and JSON editors.
pub(crate) const EXTREME_BG: Color32 = Color32::from_rgb(11, 15, 13);

/// Chip backgrounds for the REQUIREMENT column.
pub(crate) const CHIP_BG: Color32 = Color32::from_rgb(40, 50, 45);
pub(crate) const CHIP_REQUIRED_BG: Color32 = Color32::from_rgb(120, 60, 30);
