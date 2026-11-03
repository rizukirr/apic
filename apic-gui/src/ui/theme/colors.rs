//! Colour roles and the two themes that fill them. The whole GUI draws from these so the look stays uniform. Nothing here knows about contracts, git, or any other feature.

use eframe::egui::Color32;

/// The colour roles every widget draws from, named after Material Design 3
/// roles plus a few extended ones (warning, success, info, HTTP methods, JSON
/// syntax). One instance per theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Palette {
    /// Accent: primary buttons, focus, selection.
    pub(crate) primary: Color32,
    /// Text drawn on a `primary` fill.
    pub(crate) on_primary: Color32,
    /// Window and central panel background.
    pub(crate) surface: Color32,
    /// Sidebar, inputs and inactive widget backgrounds.
    pub(crate) surface_container: Color32,
    /// The recessed background behind code and JSON editors.
    pub(crate) surface_container_lowest: Color32,
    /// Body text.
    pub(crate) on_surface: Color32,
    /// Secondary text: hints, column headers, muted labels.
    pub(crate) on_surface_variant: Color32,
    /// Borders and separators.
    pub(crate) outline_variant: Color32,
    /// Background of a neutral chip.
    pub(crate) secondary_container: Color32,
    /// Errors, destructive actions, conflicts, removed lines.
    pub(crate) error: Color32,
    /// Background of the Required chip.
    pub(crate) error_container: Color32,
    /// Text drawn on `error_container`.
    pub(crate) on_error_container: Color32,
    /// Warnings and cautions.
    pub(crate) warning: Color32,
    /// Added lines and ok states.
    pub(crate) success: Color32,
    /// Links and the request URL.
    pub(crate) info: Color32,
    /// GET and HEAD method labels.
    pub(crate) method_get: Color32,
    /// POST method labels.
    pub(crate) method_post: Color32,
    /// PUT and PATCH method labels.
    pub(crate) method_put: Color32,
    /// DELETE method labels.
    pub(crate) method_delete: Color32,
    /// JSON object keys.
    pub(crate) syntax_key: Color32,
    /// JSON string values.
    pub(crate) syntax_string: Color32,
    /// JSON numbers.
    pub(crate) syntax_number: Color32,
    /// JSON `true`, `false` and `null`.
    pub(crate) syntax_literal: Color32,
    /// JSON braces, brackets, colons and commas.
    pub(crate) syntax_punctuation: Color32,
}

/// The dark theme, from Tokyo Night Storm (`folke/tokyonight.nvim`, MIT).
pub(crate) const TOKYO_NIGHT_STORM: Palette = Palette {
    primary: Color32::from_rgb(0x7a, 0xa2, 0xf7),
    on_primary: Color32::from_rgb(0x1f, 0x23, 0x35),
    surface: Color32::from_rgb(0x24, 0x28, 0x3b),
    surface_container: Color32::from_rgb(0x1f, 0x23, 0x35),
    surface_container_lowest: Color32::from_rgb(0x1d, 0x20, 0x2f),
    on_surface: Color32::from_rgb(0xc0, 0xca, 0xf5),
    on_surface_variant: Color32::from_rgb(0xa9, 0xb1, 0xd6),
    outline_variant: Color32::from_rgb(0x3b, 0x42, 0x61),
    secondary_container: Color32::from_rgb(0x29, 0x2e, 0x42),
    error: Color32::from_rgb(0xf7, 0x76, 0x8e),
    error_container: Color32::from_rgb(0xdb, 0x4b, 0x4b),
    on_error_container: Color32::from_rgb(0x1f, 0x23, 0x35),
    warning: Color32::from_rgb(0xe0, 0xaf, 0x68),
    success: Color32::from_rgb(0x9e, 0xce, 0x6a),
    info: Color32::from_rgb(0x7d, 0xcf, 0xff),
    method_get: Color32::from_rgb(0x9e, 0xce, 0x6a),
    method_post: Color32::from_rgb(0x7a, 0xa2, 0xf7),
    method_put: Color32::from_rgb(0xe0, 0xaf, 0x68),
    method_delete: Color32::from_rgb(0xf7, 0x76, 0x8e),
    syntax_key: Color32::from_rgb(0x7a, 0xa2, 0xf7),
    syntax_string: Color32::from_rgb(0x9e, 0xce, 0x6a),
    syntax_number: Color32::from_rgb(0xff, 0x9e, 0x64),
    syntax_literal: Color32::from_rgb(0xbb, 0x9a, 0xf7),
    syntax_punctuation: Color32::from_rgb(0xa9, 0xb1, 0xd6),
};

/// The light theme, from Tokyo Night Day (`folke/tokyonight.nvim`, MIT).
/// `on_surface_variant` is the palette's `dark5` darkened so secondary text
/// stays readable on `surface`.
pub(crate) const TOKYO_NIGHT_DAY: Palette = Palette {
    primary: Color32::from_rgb(0x37, 0x60, 0xbf),
    on_primary: Color32::from_rgb(0xe1, 0xe2, 0xe7),
    surface: Color32::from_rgb(0xe1, 0xe2, 0xe7),
    surface_container: Color32::from_rgb(0xd0, 0xd5, 0xe3),
    surface_container_lowest: Color32::from_rgb(0xd0, 0xd5, 0xe3),
    on_surface: Color32::from_rgb(0x37, 0x60, 0xbf),
    on_surface_variant: Color32::from_rgb(0x5b, 0x62, 0x87),
    outline_variant: Color32::from_rgb(0xa8, 0xae, 0xcb),
    secondary_container: Color32::from_rgb(0xc4, 0xc8, 0xda),
    error: Color32::from_rgb(0xf5, 0x2a, 0x65),
    error_container: Color32::from_rgb(0xc6, 0x43, 0x43),
    on_error_container: Color32::from_rgb(0xe1, 0xe2, 0xe7),
    warning: Color32::from_rgb(0x8c, 0x6c, 0x3e),
    success: Color32::from_rgb(0x58, 0x75, 0x39),
    info: Color32::from_rgb(0x00, 0x71, 0x97),
    method_get: Color32::from_rgb(0x58, 0x75, 0x39),
    method_post: Color32::from_rgb(0x2e, 0x7d, 0xe9),
    method_put: Color32::from_rgb(0x8c, 0x6c, 0x3e),
    method_delete: Color32::from_rgb(0xf5, 0x2a, 0x65),
    syntax_key: Color32::from_rgb(0x2e, 0x7d, 0xe9),
    syntax_string: Color32::from_rgb(0x58, 0x75, 0x39),
    syntax_number: Color32::from_rgb(0xb1, 0x5c, 0x00),
    syntax_literal: Color32::from_rgb(0x98, 0x54, 0xf1),
    syntax_punctuation: Color32::from_rgb(0x61, 0x72, 0xb0),
};
