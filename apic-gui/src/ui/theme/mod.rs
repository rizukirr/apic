//! Colour roles, spacing scale, and the egui theme installation. Tokens live in the child modules and are re-exported here. Nothing in this module may import from `crate::features`.

pub(crate) mod colors;
pub(crate) mod spacing;
pub(crate) mod typography;

pub(crate) use colors::*;
pub(crate) use spacing::*;
pub(crate) use typography::*;

use eframe::egui;
use egui::Stroke;

/// The palette for `theme`: Tokyo Night Storm for dark, Tokyo Night Day for light.
pub(crate) fn palette_for(theme: egui::Theme) -> &'static Palette {
    match theme {
        egui::Theme::Dark => &TOKYO_NIGHT_STORM,
        egui::Theme::Light => &TOKYO_NIGHT_DAY,
    }
}

/// The palette egui is drawing with this frame, resolved from the user's
/// preference and, for System, the OS setting.
#[allow(dead_code, reason = "no widget draws from the palette yet")]
pub(crate) fn palette(ctx: &egui::Context) -> &'static Palette {
    palette_for(ctx.theme())
}

/// Installs one egui style per theme slot from its palette, then applies
/// `preference`.
// vibekit: one theme per egui slot (dark, light). A third theme needs the palette stored in ctx data.
pub(crate) fn install_themes(ctx: &egui::Context, preference: egui::ThemePreference) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut style = theme.default_style();
        apply_palette(&mut style, palette_for(theme));
        ctx.set_style_of(theme, style);
    }
    ctx.set_theme(preference);
}

/// Writes `p` into `style`'s visuals and sets the app's spacing.
fn apply_palette(style: &mut egui::Style, p: &Palette) {
    let v = &mut style.visuals;
    v.panel_fill = p.surface;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface_container_lowest;
    v.faint_bg_color = p.surface_container;
    v.override_text_color = Some(p.on_surface);
    v.hyperlink_color = p.info;
    v.selection.bg_fill = p.primary.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.primary);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.outline_variant);
    v.widgets.inactive.bg_fill = p.surface_container;
    v.widgets.inactive.weak_bg_fill = p.surface_container;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.primary);
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.primary);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
}

/// Both theme preferences a render test runs under.
#[cfg(test)]
#[allow(dead_code, reason = "no widget draws from the palette yet")]
pub(crate) const BOTH_THEMES: [egui::ThemePreference; 2] =
    [egui::ThemePreference::Dark, egui::ThemePreference::Light];

/// Runs `add_contents` in a headless egui frame with both themes installed
/// and `preference` applied, for render tests.
#[cfg(test)]
#[allow(dead_code, reason = "no widget draws from the palette yet")]
pub(crate) fn run_in_theme(
    preference: egui::ThemePreference,
    mut add_contents: impl FnMut(&mut egui::Ui),
) {
    egui::__run_test_ui(|ui| {
        install_themes(ui.ctx(), preference);
        add_contents(ui);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Color32;

    /// WCAG 2.1 contrast ratio between two opaque colours.
    fn contrast_ratio(a: Color32, b: Color32) -> f32 {
        fn channel(c: u8) -> f32 {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        fn luminance(c: Color32) -> f32 {
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn palette_follows_the_theme_preference() {
        let ctx = egui::Context::default();
        install_themes(&ctx, egui::ThemePreference::Dark);
        assert_eq!(palette(&ctx), &TOKYO_NIGHT_STORM);
        install_themes(&ctx, egui::ThemePreference::Light);
        assert_eq!(palette(&ctx), &TOKYO_NIGHT_DAY);
    }

    #[test]
    fn install_themes_fills_each_slot_from_its_palette() {
        let ctx = egui::Context::default();
        install_themes(&ctx, egui::ThemePreference::System);
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.panel_fill,
            TOKYO_NIGHT_STORM.surface
        );
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            TOKYO_NIGHT_DAY.surface
        );
    }

    #[test]
    fn text_roles_meet_the_contrast_floor_in_both_themes() {
        for p in [&TOKYO_NIGHT_STORM, &TOKYO_NIGHT_DAY] {
            for (name, fg, bg) in [
                ("on_surface on surface", p.on_surface, p.surface),
                ("on_primary on primary", p.on_primary, p.primary),
                (
                    "on_surface_variant on surface",
                    p.on_surface_variant,
                    p.surface,
                ),
            ] {
                let ratio = contrast_ratio(fg, bg);
                assert!(ratio >= 4.5, "{name} is {ratio:.2}:1 in {p:?}");
            }
        }
    }
}
