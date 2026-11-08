//! The contracts sidebar: the contract tree with its filter, the header
//! icons above it, and the template list.

use eframe::egui;
use egui::RichText;

use super::editor::method_color;
use crate::features::contracts::domain::entry::DeleteTarget;
use crate::features::contracts::domain::tree::{TreeNode, build_tree};
use crate::features::contracts::ui::state::{ContractsAction, ContractsState};
use crate::ui::components::{bordered_input, icon_button, row_menu_button};
use crate::ui::theme::*;

/// The template list shown inside the Templates section: one selectable row
/// per template, with the same hover and right-click menu as contract rows.
pub(crate) fn templates_body(
    ui: &mut egui::Ui,
    state: &mut ContractsState,
) -> Option<ContractsAction> {
    let p = palette(ui.ctx());
    let mut action = None;
    if state.templates.is_empty() {
        ui.label(RichText::new("(none)").color(p.on_surface_variant));
    }
    let selected = state.selected_template;
    for (i, (name, path)) in state.templates.iter().enumerate() {
        ui.horizontal(|ui| {
            let hovered = ui.rect_contains_pointer(ui.max_rect());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                row_menu_button(ui, hovered, |ui| {
                    menu_items_ui(ui, template_menu_items(name, path), &mut action)
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let row = ui.add(
                        egui::Button::selectable(
                            selected == Some(i),
                            RichText::new(format!("{} {name}", egui_phosphor::regular::DIAMOND))
                                .color(p.warning),
                        )
                        .truncate(),
                    );
                    if row.clicked() {
                        action = Some(ContractsAction::LoadTemplate(i));
                    }
                    row.context_menu(|ui| {
                        menu_items_ui(ui, template_menu_items(name, path), &mut action)
                    });
                });
            });
        });
    }
    action
}

/// The menu entries for a template row.
fn template_menu_items(name: &str, path: &std::path::Path) -> Vec<(&'static str, ContractsAction)> {
    vec![(
        "Delete",
        ContractsAction::RequestDelete(DeleteTarget::Template {
            name: name.to_string(),
            path: path.to_path_buf(),
        }),
    )]
}

/// The Contracts view's header icons: filter toggle and new request.
pub(crate) fn header_actions(ui: &mut egui::Ui) -> Option<ContractsAction> {
    let mut action = None;
    if icon_button(ui, egui_phosphor::regular::PLUS, "New request").clicked() {
        action = Some(ContractsAction::OpenNewRequest(String::new()));
    }
    if icon_button(
        ui,
        egui_phosphor::regular::MAGNIFYING_GLASS,
        "Filter contracts",
    )
    .clicked()
    {
        action = Some(ContractsAction::ToggleSearch);
    }
    action
}

/// Left sidebar body: the filter row and the contract picker (folder tree,
/// method-badged, filtered by search).
///
/// The sidebar region above the bottom sections is owned by the app shell, not
/// by this feature; this only fills it.
pub(crate) fn sidebar_body(
    ui: &mut egui::Ui,
    state: &mut ContractsState,
) -> Option<ContractsAction> {
    let tree = build_tree(&state.entries, &state.search);
    let selected = state.selected;
    let mut action = None;
    if state.search_open {
        bordered_input(ui, &mut state.search, f32::INFINITY, "Filter contracts");
    }
    ui.add_space(SPACE_EXTRA_SMALL);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_tree(&tree, ui, "", selected, &mut action);
        });
    action
}

/// The menu entries for a folder row: a new request inside it, or deleting it.
fn folder_menu_items(folder_path: &str) -> Vec<(&'static str, ContractsAction)> {
    vec![
        (
            "New request here",
            ContractsAction::OpenNewRequest(format!("{folder_path}/")),
        ),
        (
            "Delete folder",
            ContractsAction::RequestDelete(DeleteTarget::Contract {
                rel: folder_path.to_string(),
                is_dir: true,
            }),
        ),
    ]
}

/// The menu entries for a contract row.
fn contract_menu_items(rel: &str) -> Vec<(&'static str, ContractsAction)> {
    vec![(
        "Delete",
        ContractsAction::RequestDelete(DeleteTarget::Contract {
            rel: rel.to_string(),
            is_dir: false,
        }),
    )]
}

/// Renders `items` as menu buttons, recording the clicked one in `out`.
fn menu_items_ui(
    ui: &mut egui::Ui,
    items: Vec<(&'static str, ContractsAction)>,
    out: &mut Option<ContractsAction>,
) {
    for (label, action) in items {
        if ui.button(label).clicked() {
            *out = Some(action);
            ui.close();
        }
    }
}

/// Renders the tree. `prefix` is the path accumulated so far (for folder
/// ids); a click or menu choice is recorded in `action`.
fn show_tree(
    node: &TreeNode,
    ui: &mut egui::Ui,
    prefix: &str,
    selected: Option<usize>,
    action: &mut Option<ContractsAction>,
) {
    let p = palette(ui.ctx());
    for (name, child) in &node.dirs {
        let folder_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let id = ui.make_persistent_id(("tree", &folder_path));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                let hovered = ui.rect_contains_pointer(ui.max_rect());
                // The menu button is reserved first (right-to-left) so the
                // folder name truncates to the remaining width rather than
                // forcing the side panel wider than its dragged width.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    row_menu_button(ui, hovered, |ui| {
                        menu_items_ui(ui, folder_menu_items(&folder_path), action)
                    });
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(name).color(p.on_surface_variant))
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .context_menu(|ui| {
                            menu_items_ui(ui, folder_menu_items(&folder_path), action)
                        });
                    });
                });
            })
            .body(|ui| show_tree(child, ui, &folder_path, selected, action));
    }
    for (label, idx, method, invalid) in &node.files {
        let rel = if prefix.is_empty() {
            label.clone()
        } else {
            format!("{prefix}/{label}")
        };
        ui.horizontal(|ui| {
            let hovered = ui.rect_contains_pointer(ui.max_rect());
            // Reserve the menu button on the right, then let the file name
            // truncate into whatever width is left. Without truncation a long
            // name measures wider than the panel, and egui stores that as the
            // panel width every frame, blocking resize below the longest name.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                row_menu_button(ui, hovered, |ui| {
                    menu_items_ui(ui, contract_menu_items(&rel), action)
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if *invalid {
                        ui.label(RichText::new("●").color(p.error))
                            .on_hover_text("Invalid contract, click to repair");
                    }
                    ui.label(
                        RichText::new(method)
                            .color(method_color(method, p))
                            .size(11.0),
                    );
                    let file = egui::Button::selectable(
                        selected == Some(*idx),
                        RichText::new(label).color(p.on_surface),
                    )
                    .truncate();
                    let response = ui.add(file);
                    if response.clicked() {
                        *action = Some(ContractsAction::Load(*idx));
                    }
                    response
                        .context_menu(|ui| menu_items_ui(ui, contract_menu_items(&rel), action));
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_menu_offers_delete() {
        let path = std::path::Path::new("/p/.apic/template/base.json");
        let items = template_menu_items("base", path);
        assert!(items.iter().any(|(_, a)| matches!(
            a,
            ContractsAction::RequestDelete(DeleteTarget::Template { name, .. }) if name == "base"
        )));
    }

    #[test]
    fn toggle_search_clears_the_filter_when_hidden() {
        let mut state = ContractsState::default();
        state.toggle_search();
        assert!(state.search_open);
        state.search = "login".to_string();
        state.toggle_search();
        assert!(!state.search_open);
        assert!(state.search.is_empty());
    }

    #[test]
    fn folder_menu_offers_new_request_and_delete() {
        let items = folder_menu_items("auth");
        assert!(
            items
                .iter()
                .any(|(_, a)| matches!(a, ContractsAction::OpenNewRequest(p) if p == "auth/"))
        );
        assert!(items.iter().any(|(_, a)| matches!(
            a,
            ContractsAction::RequestDelete(DeleteTarget::Contract { rel, is_dir: true }) if rel == "auth"
        )));
    }

    #[test]
    fn contract_menu_offers_delete() {
        let items = contract_menu_items("auth/login.json");
        assert!(items.iter().any(|(_, a)| matches!(
            a,
            ContractsAction::RequestDelete(DeleteTarget::Contract { rel, is_dir: false }) if rel == "auth/login.json"
        )));
    }
}
