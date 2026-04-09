use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use gnomeqs_core::channel::ChannelDirection;
use crate::bridge::ToUi;
use crate::settings;
use crate::state::AppState;
use crate::tr;
use super::cursor::set_pointer_cursor;
use super::receive_view::ReceiveView;
use super::send_view::SendView;
use super::settings_window::build_settings_window;

thread_local! {
    static APP_CSS_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
}

pub fn build_window(app: &libadwaita::Application, state: AppState) -> libadwaita::ApplicationWindow {
    let (width, height, maximized) = settings::window_state();

    apply_custom_css();
    register_debug_icon_search_path();

    let win = libadwaita::ApplicationWindow::new(app);
    win.set_title(Some("GnomeQS"));
    win.set_default_size(width, height);
    if maximized {
        win.maximize();
    }
    win.add_css_class("app-window");
    sync_theme_class(&win);
    {
        let win = win.clone();
        libadwaita::StyleManager::default().connect_dark_notify(move |_| {
            sync_theme_class(&win);
        });
    }

    // ── Toast overlay (wraps entire content) ──────────────────────────────────
    let toast_overlay = libadwaita::ToastOverlay::new();

    // ── Header bar ────────────────────────────────────────────────────────────
    let header_bar = libadwaita::HeaderBar::new();

    // Settings button
    let settings_btn = gtk4::Button::from_icon_name("preferences-system-symbolic");
    settings_btn.set_tooltip_text(Some(&tr!("Settings")));
    settings_btn.add_css_class("flat");
    set_pointer_cursor(&settings_btn);
    header_bar.pack_end(&settings_btn);

    // ── View stack (Receive / Send) ───────────────────────────────────────────
    let stack = libadwaita::ViewStack::new();

    // Build receive and send views
    let receive_view = Rc::new(ReceiveView::new(state.from_ui_tx.clone(), toast_overlay.clone()));
    let send_view = Rc::new(SendView::new(state.from_ui_tx.clone()));

    let _recv_page = stack.add_titled_with_icon(
        &receive_view.root,
        Some("receive"),
        &tr!("Receive"),
        "folder-download-symbolic",
    );

    let _send_page = stack.add_titled_with_icon(
        &send_view.root,
        Some("send"),
        &tr!("Send"),
        "share-symbolic",
    );

    // Start/stop discovery when the Send tab is activated
    {
        let send_view_clone = Rc::clone(&send_view);
        let last_page: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let last_page_ref = Rc::clone(&last_page);
        stack.connect_visible_child_notify(move |s| {
            let current_page = s.visible_child_name().map(|name| name.to_string());
            if *last_page_ref.borrow() == current_page {
                log::debug!(
                    "view stack notify ignored: visible page unchanged ({:?})",
                    current_page
                );
                return;
            }
            *last_page_ref.borrow_mut() = current_page.clone();

            match current_page.as_deref() {
                Some("send") => {
                    log::debug!("view stack changed to send");
                    send_view_clone.start_discovery();
                }
                _ => {
                    log::debug!("view stack changed away from send");
                    send_view_clone.stop_discovery();
                }
            }
        });
    }

    // ── Layout ────────────────────────────────────────────────────────────────
    // ── Bottom floating switcher ─────────────────────────────────────────────
    let bottom_switcher = libadwaita::ViewSwitcher::new();
    bottom_switcher.set_policy(libadwaita::ViewSwitcherPolicy::Wide);
    bottom_switcher.set_stack(Some(&stack));
    bottom_switcher.set_size_request(204, 43);
    set_pointer_cursor(&bottom_switcher);

    let switcher_wrap = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    switcher_wrap.add_css_class("pill-switcher");
    switcher_wrap.set_size_request(208, 45);
    switcher_wrap.append(&bottom_switcher);
    switcher_wrap.set_halign(gtk4::Align::Center);
    switcher_wrap.set_valign(gtk4::Align::End);
    switcher_wrap.set_margin_bottom(12);
    switcher_wrap.set_margin_start(16);
    switcher_wrap.set_margin_end(16);
    switcher_wrap.set_hexpand(false);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&stack));
    overlay.add_overlay(&switcher_wrap);
    overlay.set_vexpand(true);

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vbox.add_css_class("app-root");
    vbox.append(&header_bar);
    vbox.append(&overlay);

    toast_overlay.set_child(Some(&vbox));
    win.set_content(Some(&toast_overlay));

    // ── Settings button handler ───────────────────────────────────────────────
    {
        let win_clone = win.clone();
        let tx = state.from_ui_tx.clone();
        settings_btn.connect_clicked(move |_| {
            let settings_win = build_settings_window(&win_clone, tx.clone());
            settings_win.present(Some(&win_clone));
        });
    }

    // ── Close-to-tray behavior ────────────────────────────────────────────────
    win.connect_close_request(move |w| {
        settings::save_window_state(w.width(), w.height(), w.is_maximized());
        if settings::get_keep_running_on_close() {
            w.set_visible(false);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });

    // ── Async message loop: Tokio → GTK ──────────────────────────────────────
    let rx = state.to_ui_rx.clone();
    let win_weak = win.downgrade();
    let receive_view_clone = Rc::clone(&receive_view);
    let send_view_clone = Rc::clone(&send_view);
    let toast_overlay_clone = toast_overlay.clone();

    glib::MainContext::default().spawn_local(async move {
        while let Ok(msg) = rx.recv().await {
            let Some(win) = win_weak.upgrade() else { break };

            match msg {
                ToUi::TransferUpdate(cm) => {
                    match cm.direction {
                        ChannelDirection::LibToFront => {
                            if let Some(rtype) = &cm.rtype {
                                use gnomeqs_core::channel::TransferType;
                                match rtype {
                                    TransferType::Inbound => {
                                        // Show notification on WaitingForUserConsent
                                        if cm.state == Some(gnomeqs_core::State::WaitingForUserConsent) {
                                            if let Some(meta) = &cm.meta {
                                                let name = meta.source.as_ref()
                                                    .map(|s| s.name.as_str())
                                                    .unwrap_or("Unknown");
                                                let app = win
                                                    .application()
                                                    .and_then(|a| a.downcast::<libadwaita::Application>().ok());
                                                send_notification(app.as_ref(), name, &cm.id);
                                            }
                                        }
                                        receive_view_clone.handle_channel_message(cm);
                                    }
                                    TransferType::Outbound => {
                                        send_view_clone.handle_channel_message(cm);
                                    }
                                }
                            } else {
                                receive_view_clone.handle_channel_message(cm);
                            }
                        }
                        ChannelDirection::FrontToLib => {}
                    }
                }
                ToUi::EndpointUpdate(info) => {
                    send_view_clone.update_endpoint(info);
                }
                ToUi::VisibilityChanged(vis) => {
                    receive_view_clone.update_visibility(vis);
                }
                ToUi::BleNearby => {
                    // Nudge user to become temporarily visible
                    let toast = libadwaita::Toast::new(
                        "A nearby device wants to share. Tap to become visible.",
                    );
                    toast_overlay_clone.add_toast(toast);
                }
                ToUi::Toast(message) => {
                    let toast = libadwaita::Toast::new(&message);
                    toast_overlay_clone.add_toast(toast);
                }
                ToUi::WifiDirectSessionReady(ready) => {
                    send_view_clone.handle_wifi_direct_session_ready(ready);
                }
                ToUi::ShowWindow => {
                    win.set_visible(true);
                    win.present();
                }
                ToUi::Quit => {
                    if let Some(app) = win.application() {
                        app.quit();
                    }
                    break;
                }
            }
        }
    });

    win
}
pub fn apply_custom_css() {
    let Some(display) = gdk::Display::default() else { return };
    let font_size_px = settings::font_size_css_px();
    let css = r#"
.app-window.dark-mode {
  background:
    radial-gradient(circle at 22% 12%, rgba(132, 92, 255, 0.34) 0%, rgba(132, 92, 255, 0.00) 28%),
    radial-gradient(circle at 74% 18%, rgba(98, 225, 255, 0.10) 0%, rgba(98, 225, 255, 0.00) 18%),
    radial-gradient(circle at 50% 70%, rgba(255, 255, 255, 0.04) 0%, rgba(255, 255, 255, 0.00) 22%),
    linear-gradient(165deg, #24185f 0%, #15133c 46%, #090918 100%);
  color: #e9e7ff;
  font-size: __FONT_SIZE_PX__px;
}
.app-window.light-mode {
  background: linear-gradient(180deg, #f6f2ff 0%, #ece6ff 52%, #e2ddff 100%);
  color: #231942;
  font-size: __FONT_SIZE_PX__px;
}
.app-window .app-root {
  background: transparent;
}
.app-window label,
.app-window entry,
.app-window textview,
.app-window button,
.app-window row,
.app-window spinbutton,
.app-window combobox,
.app-window dropdown,
.app-window box,
.app-window listview {
  font-size: __FONT_SIZE_PX__px;
}
.app-window .headerbar,
.app-window headerbar {
  background: transparent;
  border: none;
  box-shadow: none;
}

.app-window.dark-mode .glass-card,
.app-window.dark-mode .boxed-list {
  background:
    linear-gradient(180deg, rgba(100, 79, 196, 0.32), rgba(44, 32, 91, 0.22));
  border-radius: 16px;
  border: 1px solid rgba(255,255,255,0.16);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.10),
    inset 0 -1px 0 rgba(0,0,0,0.12),
    0 18px 42px rgba(0,0,0,0.34);
}
.app-window.light-mode .glass-card,
.app-window.light-mode .boxed-list {
  background: color-mix(in srgb, #7c6acb 22%, white);
  border-radius: 16px;
  border: 1px solid color-mix(in srgb, #6d5bb3 22%, transparent);
  box-shadow: 0 14px 34px rgba(44, 27, 86, 0.16);
}
.app-window .boxed-list row,
.app-window .boxed-list listitem {
  background: transparent;
}
.app-window .send-drop-card {
  padding: 26px 20px;
  border-radius: 18px;
}
.app-window.dark-mode .send-drop-card {
  background:
    linear-gradient(180deg, rgba(83, 64, 168, 0.28), rgba(41, 31, 87, 0.20));
}
.app-window.light-mode .send-drop-card {
  background:
    linear-gradient(180deg, rgba(124, 106, 203, 0.20), rgba(255, 255, 255, 0.52));
}
.app-window .send-drop-icon {
  opacity: 0.82;
}
.app-window .send-drop-title {
  font-size: 1.14em;
  font-weight: 700;
}
.app-window.dark-mode .send-drop-title {
  color: #f2eeff;
}
.app-window.light-mode .send-drop-title {
  color: #2f235d;
}
.app-window .send-drop-subtitle {
  font-size: 0.96em;
  opacity: 0.78;
}
.app-window .send-drop-meta {
  font-size: 0.84em;
  opacity: 0.72;
}
.app-window .send-drop-card.send-drop-active {
  border: 1px solid rgba(120, 196, 255, 0.55);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.12),
    0 0 0 1px rgba(120, 196, 255, 0.10),
    0 18px 32px rgba(54, 121, 255, 0.12);
}
.app-window .send-select-button {
  border-radius: 12px;
  padding: 10px 18px;
  font-weight: 700;
}
.app-window .selected-file-overlay {
  margin: 1px 5px 5px 1px;
}
.app-window .selected-file-tile {
  border-radius: 16px;
  min-width: 52px;
  min-height: 52px;
  padding: 0;
}
.app-window.dark-mode .selected-file-tile {
  background: linear-gradient(180deg, rgba(255,255,255,0.10), rgba(74, 56, 150, 0.24));
  border: 1px solid rgba(255,255,255,0.11);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.08),
    0 10px 18px rgba(0,0,0,0.14);
}
.app-window.light-mode .selected-file-tile {
  background: linear-gradient(180deg, rgba(255,255,255,0.86), rgba(219, 210, 255, 0.70));
  border: 1px solid rgba(102, 82, 184, 0.14);
  box-shadow: 0 10px 18px rgba(76, 58, 140, 0.10);
}
.app-window .selected-file-tile-icon {
  opacity: 0.95;
}
.app-window .selected-file-preview {
  border-radius: 12px;
}
.app-window .selected-file-remove-badge {
  min-width: 13px;
  min-height: 13px;
  padding: 0;
  border-radius: 999px;
  margin-top: -1px;
  margin-right: -1px;
  box-shadow: 0 1px 4px rgba(0,0,0,0.08);
}
.app-window .selected-file-remove-badge image {
  -gtk-icon-size: 7px;
  opacity: 0.88;
}
.app-window.dark-mode .selected-file-remove-badge {
  background: rgb(111, 0, 0);
  border: 1px solid rgba(255,255,255,0.72);
  color: white;
}
.app-window.light-mode .selected-file-remove-badge {
  background: rgb(111, 0, 0);
  border: 1px solid rgba(255,255,255,0.80);
  color: white;
}
.app-window .selected-file-remove-badge:hover {
  filter: brightness(1.06);
}
.app-window.dark-mode .send-select-button {
  background: linear-gradient(180deg, rgba(103,80,210,0.40), rgba(73,52,160,0.26));
  border: 1px solid rgba(255,255,255,0.10);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.10),
    0 8px 18px rgba(0,0,0,0.20);
}
.app-window.light-mode .send-select-button {
  background: linear-gradient(180deg, rgba(111,87,225,0.18), rgba(255,255,255,0.56));
  border: 1px solid rgba(93,72,177,0.18);
  box-shadow: 0 8px 18px rgba(76, 58, 140, 0.12);
}
.app-window .clear-files-button {
  border-radius: 999px;
}
.app-window.dark-mode .clear-files-button {
  color: #ff8d86;
}
.app-window.light-mode .clear-files-button {
  color: #d85663;
}
.app-window.dark-mode .clear-files-button:hover {
  background: rgba(255, 107, 95, 0.12);
}
.app-window.light-mode .clear-files-button:hover {
  background: rgba(216, 86, 99, 0.12);
}
.app-window .devices-card {
  padding: 14px 14px 10px 14px;
  border-radius: 18px;
}
.app-window .network-summary-card {
  border-radius: 14px;
  padding: 10px 12px;
  margin-top: 2px;
  margin-bottom: 2px;
}
.app-window.dark-mode .network-summary-card {
  background: linear-gradient(180deg, rgba(255,255,255,0.08), rgba(65, 49, 132, 0.14));
  border: 1px solid rgba(255,255,255,0.08);
}
.app-window.light-mode .network-summary-card {
  background: linear-gradient(180deg, rgba(255,255,255,0.70), rgba(226, 218, 255, 0.86));
  border: 1px solid rgba(107, 86, 195, 0.14);
}
.app-window .network-summary-title {
  font-size: 0.88em;
  font-weight: 700;
  opacity: 0.82;
}
.app-window .network-summary-subtitle {
  font-size: 0.88em;
  line-height: 1.25;
  opacity: 0.84;
}
.app-window .caption-heading {
  letter-spacing: 0.08em;
  text-transform: uppercase;
  font-size: 0.9em;
  opacity: 0.74;
  font-weight: 700;
}
.app-window.dark-mode .boxed-list row:hover {
  background: color-mix(in srgb, #ffffff 8%, transparent);
}
.app-window.light-mode .boxed-list row:hover {
  background: color-mix(in srgb, #5f4bb6 10%, white);
}

.app-window .boxed-list row.transfer-row.transfer-active {
  background: color-mix(in srgb, #60a5fa 10%, transparent);
  border-radius: 14px;
}

.app-window .boxed-list row.transfer-row.transfer-success {
  background: color-mix(in srgb, #22c55e 18%, transparent);
  border-radius: 14px;
  border: 1px solid color-mix(in srgb, #86efac 26%, transparent);
}

.app-window .boxed-list row.transfer-row.transfer-success:hover {
  background: color-mix(in srgb, #22c55e 24%, transparent);
}

.app-window .boxed-list row.transfer-row.transfer-error {
  background: color-mix(in srgb, #ef4444 16%, transparent);
  border-radius: 14px;
  border: 1px solid color-mix(in srgb, #fca5a5 24%, transparent);
}

.app-window .boxed-list row.transfer-row.transfer-error:hover {
  background: color-mix(in srgb, #ef4444 22%, transparent);
}

.app-window .pin-badge {
  font-size: 0.84em;
  font-weight: 700;
  letter-spacing: 0.04em;
  padding: 4px 10px;
  border-radius: 999px;
}

.app-window.dark-mode .pin-badge {
  color: #efe9ff;
  background: linear-gradient(180deg, rgba(255,255,255,0.12), rgba(139, 92, 246, 0.18));
  border: 1px solid rgba(255,255,255,0.12);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.10),
    0 6px 14px rgba(16, 8, 44, 0.22);
}

.app-window.light-mode .pin-badge {
  color: #49357f;
  background: linear-gradient(180deg, rgba(255,255,255,0.86), rgba(192, 180, 255, 0.55));
  border: 1px solid rgba(107, 86, 195, 0.16);
  box-shadow: 0 6px 14px rgba(76, 58, 140, 0.10);
}

.app-window .visibility-visible {
  color: #5eead4;
  -gtk-icon-shadow:
    0 0 10px rgba(94, 234, 212, 0.85),
    0 0 18px rgba(94, 234, 212, 0.55),
    0 0 28px rgba(94, 234, 212, 0.30);
}

.app-window .visibility-hidden {
  color: #f87171;
  -gtk-icon-shadow:
    0 0 10px rgba(248, 113, 113, 0.75),
    0 0 18px rgba(248, 113, 113, 0.45),
    0 0 28px rgba(248, 113, 113, 0.24);
}

.app-window .visibility-temporary {
  color: #cbd5e1;
}

.app-window.dark-mode .status-page {
  color: #d9d2ff;
}
.app-window.light-mode .status-page {
  color: #4b3f72;
}

.app-window.dark-mode .pill-switcher {
  background:
    linear-gradient(180deg, rgba(255,255,255,0.12), rgba(67,49,140,0.24));
  border-radius: 16px;
  padding: 2px;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.10),
    inset 0 -1px 0 rgba(0,0,0,0.12),
    0 18px 38px rgba(0,0,0,0.36);
  border: 1px solid rgba(255,255,255,0.12);
  outline: 1px solid transparent;
}
.app-window.light-mode .pill-switcher {
  background: linear-gradient(180deg, rgba(82,56,184,0.14), rgba(255,255,255,0.5));
  border-radius: 12px;
  padding: 2px;
  box-shadow: 0 10px 26px rgba(69, 46, 146, 0.18);
  border: 1px solid rgba(93, 72, 177, 0.16);
  outline: 1px solid transparent;
}
.app-window .pill-switcher .viewswitcher {
  background: transparent;
}
.app-window .pill-switcher .viewswitcher button {
  min-height: 32px;
  min-width: 84px;
  padding: 6px 10px;
  border-radius: 9px;
  box-shadow: none;
  border: none;
  outline: none;
}
.app-window .pill-switcher .viewswitcher button:focus,
.app-window .pill-switcher .viewswitcher button:focus-visible,
.app-window .pill-switcher .viewswitcher button:focus-within {
  outline: none;
  box-shadow: none;
}
.app-window.dark-mode .pill-switcher .viewswitcher button {
  color: rgba(255,255,255,0.78);
}
.app-window.light-mode .pill-switcher .viewswitcher button {
  color: rgba(45, 30, 94, 0.84);
}
.app-window.dark-mode .pill-switcher .viewswitcher button:hover {
  background: linear-gradient(180deg, rgba(255,255,255,0.12), rgba(255,255,255,0.04));
}
.app-window.light-mode .pill-switcher .viewswitcher button:hover {
  background: linear-gradient(180deg, rgba(93,72,177,0.10), rgba(255,255,255,0.34));
}
.app-window.dark-mode .pill-switcher .viewswitcher button:checked {
  background: linear-gradient(180deg, rgba(103,80,210,0.90), rgba(73,52,160,0.92));
  color: #fff;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.14),
    inset 0 -1px 0 rgba(0,0,0,0.16),
    0 6px 14px rgba(90,61,189,0.22);
}
.app-window.light-mode .pill-switcher .viewswitcher button:checked {
  background: linear-gradient(180deg, rgba(111,87,225,0.88), rgba(77,55,171,0.92));
  color: #fff;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.22),
    inset 0 -1px 0 rgba(0,0,0,0.08),
    0 8px 18px rgba(90,61,189,0.20);
}
.app-window .pill-switcher .viewswitcher button label {
  font-size: 1.0em;
  font-weight: 600;
}
.app-window.dark-mode .device-tile {
  padding: 0;
  background: linear-gradient(180deg, rgba(90, 71, 164, 0.34), rgba(55, 41, 112, 0.28));
  border-radius: 14px;
  border: 1px solid rgba(255,255,255,0.08);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.06),
    0 10px 24px rgba(0,0,0,0.22);
}
.app-window.dark-mode .device-tile:hover {
  background: linear-gradient(180deg, rgba(102, 80, 183, 0.38), rgba(61, 47, 124, 0.32));
}
.app-window.light-mode .device-tile {
  padding: 0;
  background: linear-gradient(180deg, rgba(255,255,255,0.72), rgba(241,236,255,0.92));
  color: #2e2357;
  border-radius: 14px;
  border: 1px solid rgba(110, 87, 184, 0.14);
  box-shadow: 0 10px 24px rgba(76, 58, 140, 0.14);
}
.app-window.light-mode .device-tile:hover {
  background: linear-gradient(180deg, rgba(255,255,255,0.86), rgba(236,230,255,0.98));
}
.app-window.light-mode .device-tile image,
.app-window.light-mode .device-tile label {
  color: #2e2357;
}
.app-window.dark-mode .device-tile image,
.app-window.dark-mode .device-tile label {
  color: #f0ebff;
}
.app-window .device-tile-title {
  font-weight: 700;
}
.app-window .device-tile-meta {
  font-size: 0.80em;
  opacity: 0.72;
}
.app-window .device-transport-badge {
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 0.76em;
  font-weight: 700;
  letter-spacing: 0.02em;
}
.app-window.dark-mode .transport-wifi {
  background: rgba(94, 234, 212, 0.12);
  color: #8ef3e5;
  border: 1px solid rgba(94, 234, 212, 0.20);
}
.app-window.light-mode .transport-wifi {
  background: rgba(63, 188, 166, 0.12);
  color: #197b6e;
  border: 1px solid rgba(63, 188, 166, 0.18);
}
.app-window.dark-mode .transport-wifi-direct {
  background: rgba(96, 165, 250, 0.12);
  color: #a7d0ff;
  border: 1px solid rgba(96, 165, 250, 0.20);
}
.app-window.light-mode .transport-wifi-direct {
  background: rgba(86, 118, 235, 0.12);
  color: #3651a8;
  border: 1px solid rgba(86, 118, 235, 0.18);
}
"#
    .replace("__FONT_SIZE_PX__", &font_size_px.to_string());

    APP_CSS_PROVIDER.with(|cell| {
        let provider = cell.borrow_mut().take().unwrap_or_else(gtk4::CssProvider::new);
        provider.load_from_string(&css);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *cell.borrow_mut() = Some(provider);
    });
}

fn register_debug_icon_search_path() {
    #[cfg(debug_assertions)]
    if let Some(display) = gdk::Display::default() {
        let icon_theme = gtk4::IconTheme::for_display(&display);
        icon_theme.add_search_path(format!("{}/data/icons", env!("CARGO_MANIFEST_DIR")));
    }
}

fn sync_theme_class(win: &impl IsA<gtk4::Widget>) {
    let widget = win.as_ref();
    widget.remove_css_class("light-mode");
    widget.remove_css_class("dark-mode");
    if libadwaita::StyleManager::default().is_dark() {
        widget.add_css_class("dark-mode");
    } else {
        widget.add_css_class("light-mode");
    }
}

/// Send a native desktop notification for an inbound transfer request.
/// Action buttons on the notification route through Application-level actions.
fn send_notification(
    app: Option<&libadwaita::Application>,
    sender_name: &str,
    transfer_id: &str,
) {
    let Some(app) = app else { return };
    let n = gio::Notification::new("GnomeQS");
    n.set_body(Some(&format!("{sender_name} wants to share")));

    // Action target is the transfer id as a string variant
    let id_variant = glib::Variant::from(transfer_id);
    n.add_button_with_target_value(
        &tr!("Accept"),
        "app.accept-transfer",
        Some(&id_variant),
    );
    n.add_button_with_target_value(
        &tr!("Decline"),
        "app.reject-transfer",
        Some(&id_variant),
    );

    app.send_notification(Some(transfer_id), &n);
}
