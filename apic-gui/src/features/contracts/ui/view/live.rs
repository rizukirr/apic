//! The Live side of the response pane: the Examples | Live switch, the reply
//! summary in the header row, and the reply's body or headers below it.

use eframe::egui;
use egui_extras::Column;

use super::editor::status_tone;
use crate::features::contracts::ui::send::{LiveBody, LiveReply, SendState};
use crate::features::contracts::ui::state::{ContractsAction, RespTab};
use crate::ui::base::{Text, Tone, header_label, json_editor};
use crate::ui::components::{empty_state, fill_column, grid_cell, metadata_table, table_frame};
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
    let Some(Ok(reply)) = &send.live else {
        return;
    };
    Text::new(size_label(reply.size)).muted().show(ui);
    Text::new(format!("{} ms", reply.millis)).muted().show(ui);
    let status = if reply.reason.is_empty() {
        reply.status.to_string()
    } else {
        format!("{} {}", reply.status, reply.reason)
    };
    Text::new(status)
        .tone(status_tone(&reply.status.to_string()))
        .strong()
        .show(ui);
}

/// The Live pane's body: a spinner while a send is in flight, the hint
/// before any send, the error of a failed send, or the reply's body or
/// headers by `tab`.
pub(super) fn live_body(ui: &mut egui::Ui, send: &mut SendState, tab: RespTab) {
    if send.is_pending() {
        ui.horizontal(|ui| {
            ui.spinner();
            Text::new("Sending").muted().show(ui);
        });
        return;
    }
    match &mut send.live {
        None => empty_state(ui, "Send the request to see the real response", ""),
        Some(Err(err)) => {
            Text::new(err.as_str()).tone(Tone::Error).show(ui);
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
    match &mut reply.body {
        LiveBody::Json(text) | LiveBody::Text(text) if text.is_empty() => {
            Text::new("Empty body").muted().show(ui);
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
            Text::new(format!("Binary body, {len} bytes"))
                .muted()
                .show(ui);
        }
    }
    if reply.truncated {
        Text::new("Showing the first 2 MB of the body.")
            .tone(Tone::Warning)
            .show(ui);
    }
}

/// The reply headers in the read-only grid table.
fn reply_headers(ui: &mut egui::Ui, headers: &[(String, String)]) {
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
                                Text::new(name).tone(Tone::Info).show(ui);
                            });
                        });
                        row.col(|ui| {
                            grid_cell(ui, |ui| {
                                Text::new(value).show(ui);
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
