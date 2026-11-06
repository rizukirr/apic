//! The Live side of the response pane: the Examples | Live switch, the reply
//! summary in the header row, and the reply's body or headers below it.

use eframe::egui;
use egui::RichText;
use egui_extras::Column;

use super::editor::status_color;
use crate::features::contracts::ui::send::{LiveBody, LiveReply, SendState};
use crate::features::contracts::ui::state::{ContractsAction, RespTab};
use crate::ui::components::{
    empty_state, fill_column, grid_cell, header_label, json_editor, metadata_table, table_frame,
};
use crate::ui::theme::*;

/// The Examples | Live switch at the left of the response pane's header.
///
/// # Arguments
///
/// * `ui` - the header row
/// * `live` - whether Live is showing
///
/// # Returns
///
/// [`ContractsAction::SetLive`] when the side not showing was clicked.
pub(super) fn live_switch(ui: &mut egui::Ui, live: bool) -> Option<ContractsAction> {
    let mut action = None;
    if ui.selectable_label(!live, "Examples").clicked() && live {
        action = Some(ContractsAction::SetLive(false));
    }
    if ui.selectable_label(live, "Live").clicked() && !live {
        action = Some(ContractsAction::SetLive(true));
    }
    action
}

/// The live reply's status, time and size, drawn right to left so the status
/// reads first. The status uses the examples picker's colours. Nothing
/// without a reply.
pub(super) fn live_summary(ui: &mut egui::Ui, send: &SendState) {
    let p = palette(ui.ctx());
    let Some(Ok(reply)) = &send.live else {
        return;
    };
    ui.label(RichText::new(size_label(reply.size)).color(p.on_surface_variant));
    ui.label(RichText::new(format!("{} ms", reply.millis)).color(p.on_surface_variant));
    let status = if reply.reason.is_empty() {
        reply.status.to_string()
    } else {
        format!("{} {}", reply.status, reply.reason)
    };
    ui.label(
        RichText::new(status)
            .color(status_color(&reply.status.to_string(), p))
            .strong(),
    );
}

/// The Live pane's body: a spinner while a send is in flight, the hint
/// before any send, the error of a failed send, or the reply's body or
/// headers by `tab`.
pub(super) fn live_body(ui: &mut egui::Ui, send: &mut SendState, tab: RespTab) {
    let p = palette(ui.ctx());
    if send.is_pending() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new("Sending").color(p.on_surface_variant));
        });
        return;
    }
    match &mut send.live {
        None => empty_state(ui, "Send the request to see the real response", ""),
        Some(Err(err)) => {
            ui.label(RichText::new(err.as_str()).color(p.error));
        }
        Some(Ok(reply)) => match tab {
            RespTab::Body => reply_body(ui, reply),
            RespTab::Headers => reply_headers(ui, &reply.headers),
        },
    }
}

/// A reply body: JSON in the read-only highlighted code view, other text as
/// it is, a binary body by its size, and a note when the body was cut.
fn reply_body(ui: &mut egui::Ui, reply: &mut LiveReply) {
    let p = palette(ui.ctx());
    match &mut reply.body {
        LiveBody::Json(text) | LiveBody::Text(text) if text.is_empty() => {
            ui.label(RichText::new("Empty body").color(p.on_surface_variant));
        }
        LiveBody::Json(text) => json_editor(ui, text, false),
        LiveBody::Text(text) => {
            let mut shown: &str = text;
            ui.add(
                egui::TextEdit::multiline(&mut shown)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
        }
        LiveBody::Binary(len) => {
            ui.label(
                RichText::new(format!("Binary body, {len} bytes")).color(p.on_surface_variant),
            );
        }
    }
    if reply.truncated {
        ui.label(RichText::new("Showing the first 2 MB of the body.").color(p.warning));
    }
}

/// The reply headers in the read-only grid table.
fn reply_headers(ui: &mut egui::Ui, headers: &[(String, String)]) {
    let p = palette(ui.ctx());
    if headers.is_empty() {
        empty_state(ui, "No headers", "");
        return;
    }
    table_frame(ui, |ui| {
        let stretch = fill_column(ui, &[200.0]);
        metadata_table(ui)
            .id_salt("live_headers_table")
            .column(Column::exact(200.0))
            .column(stretch)
            .header(TABLE_HEADER_H, |mut h| {
                for label in ["Header", "Value"] {
                    h.col(|ui| grid_cell(ui, |ui| header_label(ui, label)));
                }
            })
            .body(|mut body| {
                for (name, value) in headers {
                    body.row(TABLE_ROW_H, |mut row| {
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                ui.label(RichText::new(name).color(p.info));
                            });
                        });
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                ui.label(RichText::new(value).color(p.on_surface));
                            });
                        });
                    });
                }
            });
    });
}

/// `bytes` in B, KB or MB with one decimal.
fn size_label(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let n = bytes as f64;
    if n < KB {
        format!("{bytes} B")
    } else if n < KB * KB {
        format!("{:.1} KB", n / KB)
    } else {
        format!("{:.1} MB", n / (KB * KB))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_size_label_picks_a_unit() {
        assert_eq!(size_label(512), "512 B");
        assert_eq!(size_label(1536), "1.5 KB");
        assert_eq!(size_label(3 * 1024 * 1024), "3.0 MB");
    }
}
