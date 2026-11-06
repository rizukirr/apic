//! The environment dropdown the app shell draws in the app bar.

use std::path::Path;

use eframe::egui;
use egui::RichText;

use crate::features::contracts::ui::state::{ContractsAction, ContractsState};
use crate::ui::theme::*;

/// The environment dropdown: "No environment" and every environment in
/// `.apic/env`, re-read each time the dropdown is clicked open, or a hint
/// when there is none.
///
/// # Arguments
///
/// * `ui` - the app bar row
/// * `state` - the contracts view model, whose environment list this
///   refreshes
/// * `apic_dir` - the project's `.apic` directory
///
/// # Returns
///
/// [`ContractsAction::ChooseEnv`] when another environment was picked.
pub(crate) fn env_menu(
    ui: &mut egui::Ui,
    state: &mut ContractsState,
    apic_dir: &Path,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    let current = state.send.env.clone();
    let envs = state.send.envs.clone();
    let title = format!(
        "{} {} {}",
        egui_phosphor::regular::GLOBE,
        current.as_deref().unwrap_or("No environment"),
        egui_phosphor::regular::CARET_DOWN
    );
    let response = ui
        .menu_button(RichText::new(title).color(p.on_surface), |ui| {
            if ui
                .selectable_label(current.is_none(), "No environment")
                .clicked()
            {
                if current.is_some() {
                    action = Some(ContractsAction::ChooseEnv(None));
                }
                ui.close();
            }
            if envs.is_empty() {
                ui.label(RichText::new("Add .apic/env/<name>.toml").color(p.on_surface_variant));
            }
            for name in &envs {
                let selected = current.as_deref() == Some(name.as_str());
                if ui.selectable_label(selected, name).clicked() {
                    if !selected {
                        action = Some(ContractsAction::ChooseEnv(Some(name.clone())));
                    }
                    ui.close();
                }
            }
        })
        .response;
    if response.clicked() {
        state.send.refresh_envs(apic_dir);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::tempdir;
    use crate::ui::theme::{BOTH_THEMES, run_in_theme};

    #[test]
    fn env_menu_renders_with_and_without_envs_in_both_themes() {
        let dir = tempdir();
        for preference in BOTH_THEMES {
            run_in_theme(preference, |ui| {
                let mut state = ContractsState::default();
                assert!(env_menu(ui, &mut state, &dir).is_none());
                state.send.envs = vec!["dev".to_string(), "staging".to_string()];
                state.send.env = Some("dev".to_string());
                assert!(env_menu(ui, &mut state, &dir).is_none());
            });
        }
    }
}
