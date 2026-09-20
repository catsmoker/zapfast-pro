//! Experimental tools page: activity tracker and call diagnostics.
//!
//! Views only draw and push [`Action`]s; [`crate::app::App`] applies them
//! after the frame. Both tools are opt-in and session-only.

use egui::{CornerRadius, Frame, Margin};

use crate::activity::{ActivityState, MIN_DIGITS};
use crate::app::App;
use crate::call_diagnostics::{CallDiagnostics, DiagnosticValue};
use crate::model::{Action, Page};
use crate::theme::{self, Icon};

use super::widgets;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    super::standalone_header(app, ui);
    if theme::macos_chrome(ui.ctx()) {
        super::banner(app, ui);
    }
    if app.call_diag.is_none() {
        app.actions.push(Action::RefreshCallDiagnostics);
    }
    let palette = app.palette;
    egui::ScrollArea::vertical()
        .id_salt("advanced")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(32, 24))
                .show(ui, |ui| {
                    ui.set_max_width(640.0);
                    ui.horizontal(|ui| {
                        if theme::icon_button(
                            ui,
                            Icon::ArrowLeft,
                            20.0,
                            palette.secondary,
                            palette.text,
                            "Back (Esc)",
                        )
                        .clicked()
                        {
                            app.actions.push(Action::Open(Page::Chats));
                        }
                        theme::text(ui, "Advanced Tools", theme::bold(24.0), palette.text);
                        widgets::chip(ui, &palette, "Experimental");
                    });
                    ui.add_space(18.0);
                    tracker(app, ui);
                    ui.add_space(18.0);
                    diagnostics(app, ui);
                });
        });
}

fn tracker(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    section(ui, app, "Device Activity Tracker");
    theme::text(
        ui,
        "Experimental. Reports only the presence WhatsApp shares for the number: online state, last seen, and typing. Contacts who hide their presence stay Unknown. Monitoring runs only while started and stops when ZapFast closes.",
        theme::regular(12.5),
        palette.secondary,
    );
    ui.add_space(8.0);
    if let Some(target) = app.activity.target.clone() {
        let state = app.activity.last_state;
        let state_color = match state {
            ActivityState::Active => palette.accent,
            ActivityState::Inactive => palette.secondary,
            ActivityState::Unknown => palette.dim,
        };
        widgets::setting_row(ui, &palette, "Contact / Number", &target.label, |_| {});
        widgets::setting_row(ui, &palette, "Current Status", state.detail(), |ui| {
            theme::text(ui, state.label(), theme::semibold(14.0), state_color);
        });
        let last = app
            .activity
            .last_typing_at
            .into_iter()
            .chain(app.activity.last_change_at)
            .max()
            .map(crate::util::moment_stamp)
            .unwrap_or_else(|| "No activity observed yet.".to_owned());
        widgets::setting_row(ui, &palette, "Last Activity", &last, |_| {});
        if let Some(seen) = app
            .presence
            .get(&target.id)
            .and_then(|presence| presence.last_seen)
        {
            widgets::setting_row(
                ui,
                &palette,
                "Last Seen on WhatsApp",
                &crate::util::moment_stamp(seen),
                |_| {},
            );
        }
        let mut notify = app.activity.notify_on_change;
        widgets::setting_row(
            ui,
            &palette,
            "State-change notifications",
            "Show a desktop notification when the state changes.",
            |ui| {
                if widgets::switch(ui, &palette, &mut notify).changed() {
                    app.actions.push(Action::SetActivityNotify(notify));
                }
            },
        );
        ui.horizontal(|ui| {
            if theme::pill_button(ui, &palette, "Stop monitoring", false).clicked() {
                app.actions.push(Action::StopActivityMonitor);
            }
        });
        ui.add_space(8.0);
        theme::text(ui, "Activity History", theme::semibold(14.0), palette.text);
        if app.activity.history.is_empty() {
            theme::text(
                ui,
                "No events yet.",
                theme::regular(12.5),
                palette.secondary,
            );
        }
        for event in app.activity.history.iter().rev() {
            ui.horizontal(|ui| {
                theme::text(
                    ui,
                    &crate::util::moment_stamp(event.at),
                    theme::regular(12.5),
                    palette.secondary,
                );
                theme::text(ui, event.kind.label(), theme::regular(12.5), palette.text);
            });
        }
    } else {
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut app.activity.input)
                    .hint_text("Phone number, e.g. +39 333 123 4567")
                    .desired_width(280.0)
                    .id(egui::Id::new("activity-number")),
            );
            let digits = app
                .activity
                .input
                .chars()
                .filter(char::is_ascii_digit)
                .count();
            let ready = digits >= MIN_DIGITS;
            if theme::pill_button(ui, &palette, "Start monitoring", ready).clicked() && ready {
                app.actions.push(Action::StartActivityMonitor);
            }
            if response.lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                && ready
            {
                app.actions.push(Action::StartActivityMonitor);
            }
        });
        theme::text(
            ui,
            "At least 7 digits. Nothing is monitored until you press Start.",
            theme::regular(12.5),
            palette.secondary,
        );
    }
}

fn diagnostics(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    section(ui, app, "Call Network Diagnostics");
    theme::text(
        ui,
        "Experimental. ZapFast cannot place calls, so call-specific rows are Unavailable by design. Only the local address below is observed from this device; nothing is sent anywhere.",
        theme::regular(12.5),
        palette.secondary,
    );
    ui.add_space(8.0);
    let snapshot = app
        .call_diag
        .clone()
        .unwrap_or_else(CallDiagnostics::snapshot);
    diagnostic_row(ui, &palette, "Call state", &snapshot.call, None);
    diagnostic_row(
        ui,
        &palette,
        "Local address",
        &snapshot.local,
        Some("Observed from this device's own routing table. No traffic was sent to learn it."),
    );
    diagnostic_row(
        ui,
        &palette,
        "Connection type",
        &snapshot.connection_type,
        Some("Cannot be determined without platform network APIs, so it is not guessed."),
    );
    diagnostic_row(ui, &palette, "Public IP", &snapshot.public_ip, None);
    diagnostic_row(ui, &palette, "ASN / ISP", &snapshot.asn_isp, None);
    diagnostic_row(
        ui,
        &palette,
        "Approximate location (GeoIP)",
        &snapshot.geo,
        None,
    );
    diagnostic_row(ui, &palette, "STUN", &snapshot.stun, None);
    diagnostic_row(ui, &palette, "TURN / relay", &snapshot.turn, None);
    diagnostic_row(
        ui,
        &palette,
        "Remote peer",
        &snapshot.remote_peer,
        Some(CallDiagnostics::remote_peer_detail()),
    );
    ui.horizontal(|ui| {
        if theme::soft_button(ui, &palette, Some(Icon::Refresh), "Refresh", false).clicked() {
            app.actions.push(Action::RefreshCallDiagnostics);
        }
        theme::text(
            ui,
            &format!("Updated {}", crate::util::clock(snapshot.updated_at)),
            theme::regular(12.5),
            palette.secondary,
        );
    });
}

fn diagnostic_row(
    ui: &mut egui::Ui,
    palette: &theme::Palette,
    title: &str,
    value: &DiagnosticValue,
    extra: Option<&str>,
) {
    let mut detail = value.label().to_owned();
    let mut notes: Vec<&str> = Vec::new();
    if let Some(reason) = value.detail() {
        notes.push(reason);
    }
    if let Some(extra) = extra {
        notes.push(extra);
    }
    // The relayed peer row carries its own longer explanation.
    if matches!(value, DiagnosticValue::Relayed) && extra.is_none() {
        notes.push(CallDiagnostics::remote_peer_detail());
    }
    for note in notes {
        detail.push_str(": ");
        detail.push_str(note);
    }
    widgets::setting_row(ui, palette, title, &detail, |_| {});
}

fn section(ui: &mut egui::Ui, app: &App, label: &str) {
    let palette = app.palette;
    ui.add_space(10.0);
    Frame::new()
        .fill(palette.panel)
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::text(ui, label, theme::semibold(12.5), palette.accent);
                widgets::chip(ui, &palette, "Experimental");
            });
        });
    ui.add_space(8.0);
}
