//! Deferred keyboard-focus plumbing.
//!
//! These are not components: they stash and claim a focus target in egui's temp
//! data so a row created on one frame can grab focus on the next.

use eframe::egui;

/// Records that the row identified by `ident` (under list `key`) should grab
/// keyboard focus once it renders. The new row does not exist on the click
/// frame, so we stash the target in egui's temp data and claim it next frame in
/// [`take_pending_focus`]. This is what lets `+ query` / `+ field` drop the
/// caret straight into the new name box without a second click.
pub(crate) fn request_new_row_focus(ui: &egui::Ui, key: &str, ident: impl ToString) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(key), ident.to_string()));
}

/// If `ident` matches the focus target stashed under `key`, focus `resp` (the
/// row's name input) and clear the marker so it fires exactly once. Call right
/// after rendering each row's name field.
pub(crate) fn take_pending_focus(
    ui: &egui::Ui,
    key: &str,
    ident: impl ToString,
    resp: &egui::Response,
) {
    let id = egui::Id::new(key);
    let pending = ui.data(|d| d.get_temp::<String>(id));
    if pending.as_deref() == Some(ident.to_string().as_str()) {
        resp.request_focus();
        ui.data_mut(|d| d.remove::<String>(id));
    }
}
