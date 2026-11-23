//! The environment dropdown the app shell draws in the app bar.

use std::path::Path;

use eframe::egui;

use crate::features::contracts::ui::state::{ContractsAction, ContractsState};
use crate::ui::base::{Text, dropdown, dropdown_option};

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
    let mut action = None;
    let current = state.send.env.clone();
    let envs = state.send.envs.clone();
    let title = format!(
        "{} {}",
        egui_phosphor::regular::GLOBE,
        current.as_deref().unwrap_or("No environment")
    );
    let response = dropdown(ui, Text::new(title), |ui| {
        let none = current.is_none();
        if dropdown_option(ui, none, Text::new("No environment")) && !none {
            action = Some(ContractsAction::ChooseEnv(None));
        }
        if envs.is_empty() {
            Text::new("Add .apic/env/<name>.toml").muted().show(ui);
        }
        for name in &envs {
            let selected = current.as_deref() == Some(name.as_str());
            if dropdown_option(ui, selected, Text::new(name)) && !selected {
                action = Some(ContractsAction::ChooseEnv(Some(name.clone())));
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
