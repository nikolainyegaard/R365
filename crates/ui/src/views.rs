//! Screens and dialogs. Layout and copy follow JW365's main window.

use crate::app::{App, FALLBACK_AFTER, Tone, VERSION};
use crate::theme::{self as t};
use crate::widgets::{self, Kind};
use egui::{Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, vec2};
use r365_core::feed::Resource;
use r365_core::rdp::{DisplayMode, SessionStatus};

enum CardAction {
    None,
    Toggle,
    Connect(DisplayMode),
    Disconnect,
    Restart,
    CopyId,
    OpenLog,
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    app.pump();
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    if app.signed_in() {
        header(app, ui);
        status_bar(app, ui);
        grid(app, ui);
    } else {
        sign_in(app, ui);
    }
    settings_window(app, ctx);
    session_auth_window(app, ctx);
    sign_out_dialog(app, ctx);
    error_dialog(app, ctx);
    if app.loading || app.signing_in() || app.pending_auth.is_some() || app.any_session_busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn sign_in(app: &mut App, root: &mut Ui) {
    egui::CentralPanel::default().frame(Frame::new().fill(t::CANVAS)).show(root, |ui| {
        let card_w = 440.0_f32.min(ui.available_width() - 40.0);
        let card_h = match &app.sign_in {
            Some(s) if s.started.elapsed() >= FALLBACK_AFTER => 400.0,
            Some(_) => 350.0,
            None => 330.0,
        };
        let rect = egui::Rect::from_center_size(ui.max_rect().center(), vec2(card_w, card_h));
        ui.painter().rect(rect, CornerRadius::same(16), t::CARD, t::hairline(), StrokeKind::Inside);
        let mut card = ui.new_child(UiBuilder::new().max_rect(rect.shrink(36.0)).layout(Layout::top_down(Align::Center)));
        card.spacing_mut().item_spacing.y = 10.0;
        widgets::strong(&mut card, "R365", 26.0, t::TEXT);
        widgets::badge(&mut card, "Windows 365 & AVD for Linux", t::BADGE_BRAND);
        widgets::text(&mut card, "Connect to your Cloud PCs and virtual desktops from Linux", 13.0, t::TEXT_SECONDARY);
        card.add_space(10.0);
        if let Some(sign_in) = app.sign_in.as_mut() {
            let show_fallback = !sign_in.exchanging && sign_in.started.elapsed() >= FALLBACK_AFTER;
            let lead = if app.handler_ready {
                "Finish signing in in the browser window that opened. If the browser asks, let it open the link with R365."
            } else {
                "Finish signing in in the browser window that opened, then copy the address of the page you land on and paste it here."
            };
            card.add(egui::Label::new(RichText::new(lead).size(12.0).color(t::TEXT_SECONDARY)).wrap());
            let mut action: Option<u8> = None;
            if show_fallback {
                if app.handler_ready {
                    widgets::text(&mut card, "Browser did not hand the sign-in back? Paste the address it ended on:", 11.0, t::TEXT_MUTED);
                }
                let edit = card.add(egui::TextEdit::singleline(&mut sign_in.paste).hint_text("...?code=...&state=...").desired_width(f32::INFINITY));
                if edit.lost_focus() && card.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = Some(0);
                }
            }
            card.add_space(6.0);
            let buttons = if show_fallback { 3.0 } else { 2.0 };
            card.horizontal(|ui| {
                ui.add_space((ui.available_width() - buttons * 118.0 - (buttons - 1.0) * 8.0).max(0.0) / 2.0);
                if show_fallback && widgets::button(ui, "Continue", Kind::Primary, 118.0).clicked() {
                    action = Some(0);
                }
                if widgets::button(ui, "Open browser", Kind::Secondary, 118.0).clicked() {
                    action = Some(1);
                }
                if widgets::button(ui, "Cancel", Kind::Ghost, 118.0).clicked() {
                    action = Some(2);
                }
            });
            match action {
                Some(0) => app.submit_pasted(),
                Some(1) => app.reopen_sign_in(),
                Some(2) => app.cancel_sign_in(),
                _ => {}
            }
        } else {
            if widgets::button_sized(&mut card, "Sign in with Microsoft", Kind::Primary, 280.0, 44.0, 14.0).clicked() {
                app.start_sign_in();
            }
            if widgets::button(&mut card, "Settings", Kind::Secondary, 280.0).clicked() {
                app.settings_draft = Some(app.config.clone());
            }
        }
        card.add_space(8.0);
        card.horizontal(|ui| {
            let w = ui.available_width();
            let galley_w = ui.painter().layout_no_wrap(app.status.clone(), egui::FontId::proportional(12.0), t::TEXT).size().x
                + if app.signing_in() { 24.0 } else { 0.0 };
            ui.add_space(((w - galley_w) / 2.0).max(0.0));
            if app.signing_in() {
                ui.add(egui::Spinner::new().size(16.0).color(t::BLUE));
            }
            widgets::text(ui, &app.status, 12.0, tone_color(app.tone));
        });
        widgets::text(&mut card, &format!("R365 v{VERSION}"), 11.0, t::TEXT_MUTED);
    });
}

fn header(app: &mut App, root: &mut Ui) {
    egui::Panel::top("header").show_separator_line(false).frame(Frame::new().fill(t::SURFACE).inner_margin(Margin::symmetric(20, 12))).show(
        root,
        |ui| {
            let line = ui.max_rect();
            ui.painter().hline(line.x_range(), line.bottom() + 12.0, t::hairline());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                widgets::strong(ui, "R365", 18.0, t::TEXT);
                widgets::badge(ui, "Cloud PC", t::BADGE_BRAND);
                let search_w = (ui.available_width() - 520.0).clamp(120.0, 260.0);
                ui.add_sized(
                    [search_w, t::CONTROL_H],
                    egui::TextEdit::singleline(&mut app.search).hint_text("Search Cloud PCs and apps").margin(Margin::symmetric(10, 7)),
                );
                if widgets::button(ui, "Refresh", Kind::Secondary, 0.0).clicked() {
                    app.refresh_workspaces();
                }
                if app.loading {
                    ui.add(egui::Spinner::new().size(16.0).color(t::BLUE));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if widgets::button(ui, "Sign out", Kind::Ghost, 0.0).clicked() {
                        app.confirm_sign_out = true;
                    }
                    if widgets::button(ui, "Settings", Kind::Ghost, 0.0).clicked() {
                        app.settings_draft = Some(app.config.clone());
                    }
                    let identity = app.tokens.as_ref().map(|t| t.claims.display_identity()).unwrap_or_default();
                    widgets::pill(ui, &identity);
                });
            });
        },
    );
}

fn status_bar(app: &mut App, root: &mut Ui) {
    egui::Panel::bottom("status").show_separator_line(false).frame(Frame::new().fill(t::SURFACE).inner_margin(Margin::symmetric(20, 8))).show(
        root,
        |ui| {
            let line = ui.max_rect();
            ui.painter().hline(line.x_range(), line.top() - 8.0, t::hairline());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                widgets::text(ui, &format!("v{VERSION}"), 11.0, t::TEXT_SECONDARY);
                let n = app.resource_count();
                widgets::text(ui, &format!("{n} resource{}", if n == 1 { "" } else { "s" }), 11.0, t::TEXT_SECONDARY);
                if app.any_session_busy() {
                    ui.add(egui::Spinner::new().size(12.0).color(t::INFO));
                }
                widgets::text(ui, &app.status, 11.0, tone_color(app.tone));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let synced = match app.last_sync {
                        None => "Not synced".to_string(),
                        Some(at) if at.elapsed().as_secs() < 60 => "Synced just now".to_string(),
                        Some(at) => format!("Synced {}m ago", at.elapsed().as_secs() / 60),
                    };
                    widgets::text(ui, &synced, 11.0, t::TEXT_SECONDARY);
                    let engine = app
                        .freerdp
                        .as_ref()
                        .map(|f| format!("RDP engine: {}", f.display_name()))
                        .unwrap_or_else(|| "RDP engine: not detected (FreeRDP required)".into());
                    widgets::text(ui, &engine, 11.0, if app.freerdp.is_some() { t::TEXT_SECONDARY } else { t::ERROR_TEXT });
                });
            });
        },
    );
}

fn grid(app: &mut App, root: &mut Ui) {
    egui::CentralPanel::default().frame(Frame::new().fill(t::CANVAS).inner_margin(Margin::same(24))).show(root, |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let query = app.search.trim().to_lowercase();
            let mut actions: Vec<(Resource, CardAction)> = Vec::new();
            let mut matched = 0;
            let workspaces = app.workspaces.clone();
            for ws in &workspaces {
                let shown: Vec<&Resource> = ws
                    .resources
                    .iter()
                    .filter(|r| {
                        query.is_empty()
                            || r.title.to_lowercase().contains(&query)
                            || r.subtitle().to_lowercase().contains(&query)
                            || r.id.to_lowercase().contains(&query)
                    })
                    .collect();
                if shown.is_empty() {
                    continue;
                }
                matched += shown.len();
                ui.add_space(8.0);
                widgets::strong(ui, &ws.tenant_name, 15.0, t::TEXT);
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
                    for r in shown {
                        let action = card(ui, r, app.status_of(&r.key()));
                        if !matches!(action, CardAction::None) {
                            actions.push((r.clone(), action));
                        }
                    }
                });
                ui.add_space(16.0);
            }
            if matched == 0 {
                ui.add_space(60.0);
                ui.vertical_centered(|ui| {
                    if query.is_empty() {
                        widgets::strong(ui, "No Cloud PCs or apps found", 20.0, t::TEXT);
                        widgets::text(ui, "Refresh to check for Windows 365 or AVD resources published to you.", 13.0, t::TEXT_SECONDARY);
                        ui.add_space(12.0);
                        if widgets::button(ui, "Refresh workspaces", Kind::Primary, 0.0).clicked() {
                            app.refresh_workspaces();
                        }
                    } else {
                        widgets::strong(ui, &format!("No matches for \"{}\"", app.search.trim()), 20.0, t::TEXT);
                        widgets::text(ui, "Try a different search.", 13.0, t::TEXT_SECONDARY);
                    }
                });
            }
            for (r, action) in actions {
                apply(app, ui.ctx(), &r, action);
            }
        });
    });
}

fn apply(app: &mut App, ctx: &egui::Context, r: &Resource, action: CardAction) {
    let key = r.key();
    match action {
        CardAction::None => {}
        CardAction::Toggle => {
            if app.status_of(&key).is_active() {
                app.disconnect(&key);
            } else {
                app.connect(r, DisplayMode::Default);
            }
        }
        CardAction::Connect(mode) => app.connect(r, mode),
        CardAction::Disconnect => app.disconnect(&key),
        CardAction::Restart => app.restart(r),
        CardAction::CopyId => ctx.copy_text(r.id.clone()),
        CardAction::OpenLog => {
            if let Some(log) = app.latest_log(&key) {
                let _ = std::process::Command::new("xdg-open").arg(log).spawn();
            } else {
                app.set_status("No session log yet for this resource", Tone::Neutral);
            }
        }
    }
}

fn card(ui: &mut Ui, r: &Resource, status: SessionStatus) -> CardAction {
    let size = vec2(220.0, 206.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
    ui.painter().rect(
        rect,
        CornerRadius::same(t::CARD_RADIUS),
        if hovered { t::CARD_HOVER } else { t::CARD },
        Stroke::new(1.0, if hovered { t::FOCUS } else { t::BORDER }),
        StrokeKind::Inside,
    );
    let inner = rect.shrink(14.0);
    let mut c = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Center)));
    c.spacing_mut().item_spacing.y = 6.0;
    widgets::monitor_glyph(&mut c, 56.0);
    c.allocate_ui_with_layout(vec2(inner.width(), 20.0), Layout::left_to_right(Align::Center).with_main_align(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        widgets::badge(ui, r.kind.label(), t::BADGE_TYPE);
        let colors = match status {
            SessionStatus::Connected => t::BADGE_CONNECTED,
            SessionStatus::Failed => t::BADGE_FAILED,
            s if s.is_busy() => t::BADGE_BUSY,
            _ => t::BADGE_IDLE,
        };
        widgets::badge(ui, status.label(), colors);
    });
    c.add(egui::Label::new(RichText::new(&r.title).size(14.0).strong().color(t::TEXT)).truncate());
    c.add(egui::Label::new(RichText::new(r.subtitle()).size(11.0).color(t::TEXT_SECONDARY)).truncate());
    let button_rect = egui::Rect::from_min_size(egui::pos2(inner.left(), inner.bottom() - t::CONTROL_H), vec2(inner.width(), t::CONTROL_H));
    let mut b = ui.new_child(UiBuilder::new().max_rect(button_rect).layout(Layout::top_down(Align::Center)));
    let (label, kind) = match status {
        SessionStatus::Connected => ("Disconnect", Kind::Secondary),
        SessionStatus::Failed => ("Retry", Kind::Primary),
        SessionStatus::Disconnecting => ("Disconnecting", Kind::Secondary),
        SessionStatus::Reconnecting => ("Reconnecting", Kind::Secondary),
        s if s.is_busy() => ("Connecting", Kind::Secondary),
        _ => ("Connect", Kind::Primary),
    };
    let mut action = CardAction::None;
    if widgets::button(&mut b, label, kind, inner.width()).clicked() && status != SessionStatus::Disconnecting {
        action = CardAction::Toggle;
    }
    if resp.double_clicked() {
        action = CardAction::Toggle;
    }
    resp.context_menu(|ui| {
        ui.set_min_width(200.0);
        let items: [(&str, CardAction); 3] = [
            ("Connect (fullscreen)", CardAction::Connect(DisplayMode::Fullscreen)),
            ("Connect (windowed)", CardAction::Connect(DisplayMode::Windowed)),
            ("Connect (multi-monitor)", CardAction::Connect(DisplayMode::MultiMonitor)),
        ];
        for (label, a) in items {
            if ui.button(label).clicked() {
                action = a;
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Restart session").clicked() {
            action = CardAction::Restart;
            ui.close();
        }
        if ui.add_enabled(status.is_active(), egui::Button::new("Disconnect")).clicked() {
            action = CardAction::Disconnect;
            ui.close();
        }
        ui.separator();
        if ui.button("View latest session log").clicked() {
            action = CardAction::OpenLog;
            ui.close();
        }
        if ui.button("Copy resource ID").clicked() {
            action = CardAction::CopyId;
            ui.close();
        }
    });
    action
}

fn dialog<R>(ctx: &egui::Context, title: &str, width: f32, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    egui::Window::new(RichText::new(title).size(16.0).strong())
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([width, 0.0])
        .frame(Frame::new().fill(t::SURFACE).stroke(t::hairline()).corner_radius(CornerRadius::same(t::CARD_RADIUS)).inner_margin(Margin::same(20)))
        .show(ctx, add)
        .and_then(|r| r.inner)
}

fn settings_window(app: &mut App, ctx: &egui::Context) {
    let Some(mut draft) = app.settings_draft.take() else { return };
    let mut outcome: Option<bool> = None;
    dialog(ctx, "Settings", 420.0, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        widgets::text(ui, "DISPLAY", 11.0, t::TEXT_MUTED);
        ui.checkbox(&mut draft.fullscreen, "Start sessions fullscreen");
        ui.checkbox(&mut draft.multi_monitor, "Use all monitors (implies fullscreen)");
        ui.checkbox(&mut draft.dynamic_resolution, "Resize the remote desktop with the window");
        ui.horizontal(|ui| {
            widgets::text(ui, "Scale", 13.0, t::TEXT);
            ui.add(egui::Slider::new(&mut draft.scale_percent, 0..=200).step_by(25.0).suffix("%"));
            widgets::text(ui, if draft.scale_percent == 0 { "(off)" } else { "" }, 11.0, t::TEXT_MUTED);
        });
        ui.add_space(6.0);
        widgets::text(ui, "DEVICES", 11.0, t::TEXT_MUTED);
        ui.checkbox(&mut draft.sound, "Sound");
        ui.checkbox(&mut draft.microphone, "Microphone");
        ui.checkbox(&mut draft.clipboard, "Clipboard");
        ui.checkbox(&mut draft.usb_redirection, "USB device redirection");
        ui.checkbox(&mut draft.smartcard, "Smart card");
        ui.add_space(6.0);
        widgets::text(ui, "CONNECTION", 11.0, t::TEXT_MUTED);
        ui.checkbox(&mut draft.auto_reconnect, "Reconnect automatically");
        ui.checkbox(&mut draft.prevent_session_lock, "Keep the session from idling out");
        ui.checkbox(&mut draft.gfx_progressive, "H.264 progressive graphics");
        ui.checkbox(&mut draft.ignore_cert, "Ignore certificate errors");
        ui.horizontal(|ui| {
            widgets::text(ui, "Refresh every", 13.0, t::TEXT);
            ui.add(egui::DragValue::new(&mut draft.auto_refresh_minutes).range(1..=240).suffix(" min"));
        });
        ui.add_space(6.0);
        widgets::text(ui, "FREERDP", 11.0, t::TEXT_MUTED);
        ui.add(
            egui::TextEdit::singleline(&mut draft.freerdp_path)
                .hint_text("Detect automatically, or a binary path, or \"flatpak\"")
                .desired_width(f32::INFINITY),
        );
        let detected = app.freerdp.as_ref().map(|f| f.display_name()).unwrap_or_else(|| "none detected".into());
        widgets::text(ui, &format!("Detected: {detected}"), 11.0, t::TEXT_MUTED);
        let mut extra = draft.extra_args.join(" ");
        if ui.add(egui::TextEdit::singleline(&mut extra).hint_text("Extra FreeRDP arguments").desired_width(f32::INFINITY)).changed() {
            draft.extra_args = extra.split_whitespace().map(str::to_string).collect();
        }
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::button(ui, "Save", Kind::Primary, 90.0).clicked() {
                outcome = Some(true);
            }
            if widgets::button(ui, "Cancel", Kind::Ghost, 90.0).clicked() {
                outcome = Some(false);
            }
        });
    });
    match outcome {
        Some(true) => app.save_settings(draft),
        Some(false) => {}
        None => app.settings_draft = Some(draft),
    }
}

fn session_auth_window(app: &mut App, ctx: &egui::Context) {
    let handler_ready = app.handler_ready;
    let Some(pending) = app.pending_auth.as_mut() else { return };
    let title = format!("Sign in to {}", pending.title);
    let mut submit = false;
    let mut cancel = false;
    let mut reopen = false;
    let show_fallback = pending.started.elapsed() >= FALLBACK_AFTER;
    dialog(ctx, &title, 520.0, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        let lead = if handler_ready {
            "FreeRDP needs an Entra ID sign-in for this session. A browser window has opened; sign in there and, if the browser asks, let it open the link with R365. This dialog closes by itself."
        } else {
            "FreeRDP needs an Entra ID sign-in for this session. A browser window has opened. Sign in there, then copy the address of the page you land on and paste it below. The page will look blank or say the site cannot be reached; only the address matters."
        };
        ui.add(egui::Label::new(RichText::new(lead).size(13.0).color(t::TEXT_SECONDARY)).wrap());
        let mut looks_right = false;
        if show_fallback {
            if handler_ready {
                widgets::text(ui, "Browser did not hand the sign-in back? Paste the address it ended on:", 11.0, t::TEXT_MUTED);
            }
            let edit = ui.add(egui::TextEdit::singleline(&mut pending.paste).hint_text("...?code=...&state=...").desired_width(f32::INFINITY));
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            looks_right = pending.paste.contains("code=");
            if !pending.paste.trim().is_empty() && !looks_right {
                widgets::text(ui, "That address has no code in it. Paste the address of the page shown after sign-in.", 11.0, t::ERROR_TEXT);
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if show_fallback && ui.add_enabled_ui(looks_right, |ui| widgets::button(ui, "Continue", Kind::Primary, 100.0)).inner.clicked() {
                submit = true;
            }
            if widgets::button(ui, "Open browser again", Kind::Secondary, 0.0).clicked() {
                reopen = true;
            }
            if widgets::button(ui, "Cancel session", Kind::Ghost, 0.0).clicked() {
                cancel = true;
            }
        });
    });
    if reopen {
        app.reopen_auth_url();
    }
    if submit {
        app.submit_pasted();
    } else if cancel {
        let key = app.pending_auth.as_ref().map(|p| p.key.clone()).unwrap_or_default();
        app.disconnect(&key);
    }
}

fn sign_out_dialog(app: &mut App, ctx: &egui::Context) {
    if !app.confirm_sign_out {
        return;
    }
    let mut choice: Option<bool> = None;
    dialog(ctx, "Sign out", 380.0, |ui| {
        widgets::text(ui, "Sign out of Windows 365 and AVD? Running sessions will be disconnected.", 13.0, t::TEXT_SECONDARY);
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::button(ui, "Sign out", Kind::Primary, 100.0).clicked() {
                choice = Some(true);
            }
            if widgets::button(ui, "Cancel", Kind::Ghost, 90.0).clicked() {
                choice = Some(false);
            }
        });
    });
    if let Some(yes) = choice {
        app.confirm_sign_out = false;
        if yes {
            app.sign_out();
        }
    }
}

fn error_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(message) = app.error_dialog.clone() else { return };
    let mut close = false;
    dialog(ctx, "Connection failed", 420.0, |ui| {
        ui.add(egui::Label::new(RichText::new(&message).size(13.0).color(t::TEXT_SECONDARY)).wrap());
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::button(ui, "OK", Kind::Primary, 90.0).clicked() {
                close = true;
            }
        });
    });
    if close {
        app.error_dialog = None;
    }
}

fn tone_color(tone: Tone) -> egui::Color32 {
    match tone {
        Tone::Neutral => t::TEXT_SECONDARY,
        Tone::Active => t::INFO,
        Tone::Error => t::ERROR_TEXT,
    }
}
