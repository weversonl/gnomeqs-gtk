use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::cursor::set_pointer_cursor;
use super::receive_view::ReceiveView;
use super::send_view::SendView;
use super::settings_window::build_settings_window;
use crate::bridge::ToUi;
use crate::settings;
use crate::state::AppState;
use crate::tr;
use gnomeqs_core::channel::ChannelDirection;

thread_local! {
    static APP_CSS_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
}

pub fn build_window(
    app: &libadwaita::Application,
    state: AppState,
) -> libadwaita::ApplicationWindow {
    let (width, height, maximized) = settings::window_state();
    let windowed_width = Rc::new(Cell::new(width));
    let windowed_height = Rc::new(Cell::new(height));

    apply_custom_css();
    register_debug_icon_search_path();

    let win = libadwaita::ApplicationWindow::new(app);
    win.set_title(Some("GnomeQS"));
    win.set_default_size(width, height);
    if maximized {
        win.maximize();
    }
    win.add_css_class("app-window");
    install_window_state_tracking(&win, &windowed_width, &windowed_height);
    install_window_actions(&win, &windowed_width, &windowed_height);
    install_responsive_classes(&win);
    sync_theme_class(&win);
    {
        let win = win.clone();
        libadwaita::StyleManager::default().connect_dark_notify(move |_| {
            sync_theme_class(&win);
        });
    }

    let toast_overlay = libadwaita::ToastOverlay::new();

    let header_bar = libadwaita::HeaderBar::new();

    let back_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
    back_btn.set_tooltip_text(Some(&tr!("Back")));
    back_btn.add_css_class("flat");
    back_btn.add_css_class("send-back-button");
    back_btn.set_visible(false);
    set_pointer_cursor(&back_btn);
    header_bar.pack_start(&back_btn);

    let header_title = gtk4::Label::new(Some("GnomeQS"));
    header_title.add_css_class("app-header-title");
    header_bar.set_title_widget(Some(&header_title));

    let settings_btn = gtk4::Button::from_icon_name("preferences-system-symbolic");
    settings_btn.set_tooltip_text(Some(&tr!("Settings")));
    settings_btn.add_css_class("flat");
    set_pointer_cursor(&settings_btn);
    header_bar.pack_end(&settings_btn);

    let stack = libadwaita::ViewStack::new();

    let receive_view = Rc::new(ReceiveView::new(
        state.from_ui_tx.clone(),
        toast_overlay.clone(),
    ));
    let send_view = Rc::new(SendView::new(
        state.from_ui_tx.clone(),
        toast_overlay.clone(),
    ));

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

    // ── Bottom nav bar ──────────────────────────────────────────
    let nav_bar = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    nav_bar.add_css_class("nav-bar");
    nav_bar.set_margin_start(14);
    nav_bar.set_margin_end(14);
    nav_bar.set_margin_bottom(16);
    nav_bar.set_margin_top(4);
    nav_bar.set_hexpand(true);

    // "Enviar" pill button
    let send_nav_btn = gtk4::Button::new();
    send_nav_btn.add_css_class("nav-send-btn");
    send_nav_btn.set_hexpand(true);
    {
        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        inner.set_halign(gtk4::Align::Center);
        let icon = gtk4::Image::from_icon_name("io.github.weversonl.GnomeQuickShare-send-symbolic");
        icon.set_pixel_size(18);
        let lbl = gtk4::Label::new(Some(&tr!("Send")));
        lbl.add_css_class("nav-btn-label");
        inner.append(&icon);
        inner.append(&lbl);
        send_nav_btn.set_child(Some(&inner));
    }
    set_pointer_cursor(&send_nav_btn);

    // "Receber" row: label button + visibility toggle
    let recv_nav_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    recv_nav_row.add_css_class("nav-recv-row");
    recv_nav_row.add_css_class("nav-btn-active"); // receive is default active page
    recv_nav_row.set_hexpand(true);

    let recv_nav_btn = gtk4::Button::new();
    recv_nav_btn.add_css_class("flat");
    recv_nav_btn.add_css_class("nav-recv-btn");
    recv_nav_btn.set_hexpand(true);
    {
        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        inner.set_halign(gtk4::Align::Center);
        let icon =
            gtk4::Image::from_icon_name("io.github.weversonl.GnomeQuickShare-receive-symbolic");
        icon.set_pixel_size(18);
        let lbl = gtk4::Label::new(Some(&tr!("Receive")));
        lbl.add_css_class("nav-btn-label");
        inner.append(&icon);
        inner.append(&lbl);
        recv_nav_btn.set_child(Some(&inner));
    }
    set_pointer_cursor(&recv_nav_btn);
    recv_nav_row.append(&recv_nav_btn);

    nav_bar.append(&send_nav_btn);
    nav_bar.append(&recv_nav_row);

    // ── Stack + nav signal handlers ──────────────────────────────
    {
        let send_view_clone = Rc::clone(&send_view);
        let send_nav_btn_c = send_nav_btn.clone();
        let recv_nav_row_c = recv_nav_row.clone();
        let nav_bar_c = nav_bar.clone();
        let back_btn_c = back_btn.clone();
        let settings_btn_c = settings_btn.clone();
        let header_title_c = header_title.clone();
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
                    send_nav_btn_c.add_css_class("nav-btn-active");
                    recv_nav_row_c.remove_css_class("nav-btn-active");
                    nav_bar_c.set_visible(false);
                    back_btn_c.set_visible(true);
                    settings_btn_c.set_visible(false);
                    header_title_c.set_text(&tr!("Send Files"));
                    log::debug!("view stack changed to send");
                    send_view_clone.start_discovery();
                }
                _ => {
                    recv_nav_row_c.add_css_class("nav-btn-active");
                    send_nav_btn_c.remove_css_class("nav-btn-active");
                    nav_bar_c.set_visible(true);
                    back_btn_c.set_visible(false);
                    settings_btn_c.set_visible(true);
                    header_title_c.set_text("GnomeQS");
                    log::debug!("view stack changed away from send");
                    send_view_clone.stop_discovery();
                }
            }
        });
    }

    // Nav button click handlers
    {
        let stack = stack.clone();
        send_nav_btn.connect_clicked(move |_| {
            stack.set_visible_child_name("send");
        });
    }
    {
        let stack = stack.clone();
        recv_nav_btn.connect_clicked(move |_| {
            stack.set_visible_child_name("receive");
        });
    }
    {
        let stack = stack.clone();
        back_btn.connect_clicked(move |_| {
            stack.set_visible_child_name("receive");
        });
    }

    stack.set_vexpand(true);

    // Clamp constrains content + nav to a max width and centers them on wide screens
    let inner_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    inner_content.set_vexpand(true);
    inner_content.append(&stack);
    inner_content.append(&nav_bar);

    let clamp = libadwaita::Clamp::new();
    clamp.set_maximum_size(860);
    clamp.set_tightening_threshold(520);
    clamp.set_vexpand(true);
    clamp.set_child(Some(&inner_content));

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    vbox.add_css_class("app-root");
    vbox.append(&header_bar);
    vbox.append(&clamp);

    toast_overlay.set_child(Some(&vbox));
    win.set_content(Some(&toast_overlay));

    {
        let win_clone = win.clone();
        let tx = state.from_ui_tx.clone();
        let receive_view = Rc::clone(&receive_view);
        let send_view = Rc::clone(&send_view);
        settings_btn.connect_clicked(move |_| {
            let receive_view = Rc::clone(&receive_view);
            let send_view = Rc::clone(&send_view);
            let on_history_cleared: Rc<dyn Fn()> = Rc::new(move || {
                receive_view.clear_history();
                send_view.clear_history();
            });
            let settings_win = build_settings_window(&win_clone, tx.clone(), on_history_cleared);
            settings_win.present(Some(&win_clone));
        });
    }

    {
        let windowed_width = Rc::clone(&windowed_width);
        let windowed_height = Rc::clone(&windowed_height);
        win.connect_close_request(move |w| {
            persist_window_state(w, &windowed_width, &windowed_height);
            if settings::get_keep_running_on_close() {
                w.set_visible(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }

    // Stop all discovery when the window leaves the screen (tray, minimized, hidden).
    {
        let send_view_c = Rc::clone(&send_view);
        win.connect_unmap(move |_| {
            send_view_c.stop_discovery();
        });
    }
    // Restart discovery if the send page is active when the window comes back.
    {
        let send_view_c = Rc::clone(&send_view);
        let stack_c = stack.clone();
        win.connect_map(move |_| {
            if stack_c.visible_child_name().as_deref() == Some("send") {
                send_view_c.start_discovery();
            }
        });
    }

    let rx = state.to_ui_rx.clone();
    let win_weak = win.downgrade();
    let receive_view_clone = Rc::clone(&receive_view);
    let send_view_clone = Rc::clone(&send_view);
    let toast_overlay_clone = toast_overlay.clone();
    let stack_clone = stack.clone();
    let from_ui_tx_clone = state.from_ui_tx.clone();

    glib::MainContext::default().spawn_local(async move {
        while let Ok(msg) = rx.recv().await {
            let Some(win) = win_weak.upgrade() else { break };

            match msg {
                ToUi::TransferUpdate(cm) => match cm.direction {
                    ChannelDirection::LibToFront => {
                        if let Some(rtype) = &cm.rtype {
                            use gnomeqs_core::channel::TransferType;
                            match rtype {
                                TransferType::Inbound => {
                                    if cm.state == Some(gnomeqs_core::State::WaitingForUserConsent)
                                    {
                                        if let Some(meta) = &cm.meta {
                                            let name = meta
                                                .source
                                                .as_ref()
                                                .map(|s| s.name.as_str())
                                                .unwrap_or("Unknown");
                                            let app = win.application().and_then(|a| {
                                                a.downcast::<libadwaita::Application>().ok()
                                            });
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
                },
                ToUi::EndpointUpdate(info) => {
                    send_view_clone.update_endpoint(info);
                }
                ToUi::VisibilityChanged(vis) => {
                    receive_view_clone.update_visibility(vis);
                }
                ToUi::BleNearby => {
                    let toast = libadwaita::Toast::new(
                        &tr!("A nearby device wants to share. Tap to become visible."),
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
                ToUi::ShowWindowOnPage(page) => {
                    stack_clone.set_visible_child_name(&page);
                    win.set_visible(true);
                    win.present();
                }
                ToUi::ShowSettings => {
                    win.set_visible(true);
                    win.present();
                    let receive_view = Rc::clone(&receive_view_clone);
                    let send_view = Rc::clone(&send_view_clone);
                    let on_history_cleared: Rc<dyn Fn()> = Rc::new(move || {
                        receive_view.clear_history();
                        send_view.clear_history();
                    });
                    let settings_win =
                        build_settings_window(&win, from_ui_tx_clone.clone(), on_history_cleared);
                    settings_win.present(Some(&win));
                }
                ToUi::Quit => {
                    persist_window_state(&win, &windowed_width, &windowed_height);
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

fn install_window_actions(
    win: &libadwaita::ApplicationWindow,
    windowed_width: &Rc<Cell<i32>>,
    windowed_height: &Rc<Cell<i32>>,
) {
    let reset_action = gio::SimpleAction::new("reset-window-size", None);
    {
        let win = win.clone();
        let windowed_width = Rc::clone(windowed_width);
        let windowed_height = Rc::clone(windowed_height);
        reset_action.connect_activate(move |_, _| {
            reset_window_size(&win, &windowed_width, &windowed_height);
        });
    }
    win.add_action(&reset_action);
}

fn reset_window_size(
    win: &impl gtk4::prelude::IsA<gtk4::Window>,
    windowed_width: &Cell<i32>,
    windowed_height: &Cell<i32>,
) {
    let win = win.as_ref().clone();
    windowed_width.set(settings::DEFAULT_WINDOW_WIDTH);
    windowed_height.set(settings::DEFAULT_WINDOW_HEIGHT);
    settings::reset_window_state();

    apply_hidden_default_window_size(&win);
}

fn apply_hidden_default_window_size(win: &gtk4::Window) {
    win.set_visible(false);
    win.set_default_size(
        settings::DEFAULT_WINDOW_WIDTH,
        settings::DEFAULT_WINDOW_HEIGHT,
    );
    win.unmaximize();

    let win = win.clone();
    glib::idle_add_local_once(move || {
        win.present();
    });
}

fn install_window_state_tracking(
    win: &libadwaita::ApplicationWindow,
    windowed_width: &Rc<Cell<i32>>,
    windowed_height: &Rc<Cell<i32>>,
) {
    {
        let windowed_width = Rc::clone(windowed_width);
        let windowed_height = Rc::clone(windowed_height);
        win.connect_default_width_notify(move |w| {
            if !w.is_maximized() {
                remember_current_windowed_size(w, &windowed_width, &windowed_height);
            }
        });
    }
    {
        let windowed_width = Rc::clone(windowed_width);
        let windowed_height = Rc::clone(windowed_height);
        win.connect_default_height_notify(move |w| {
            if !w.is_maximized() {
                remember_current_windowed_size(w, &windowed_width, &windowed_height);
            }
        });
    }
    {
        let windowed_width = Rc::clone(windowed_width);
        let windowed_height = Rc::clone(windowed_height);
        win.connect_maximized_notify(move |w| {
            persist_window_state(w, &windowed_width, &windowed_height);
        });
    }
}

fn remember_current_windowed_size(
    win: &impl gtk4::prelude::IsA<gtk4::Window>,
    windowed_width: &Cell<i32>,
    windowed_height: &Cell<i32>,
) {
    let (width, height) = current_windowed_size(win, windowed_width.get(), windowed_height.get());
    windowed_width.set(width);
    windowed_height.set(height);
}

fn persist_window_state(
    win: &impl gtk4::prelude::IsA<gtk4::Window>,
    windowed_width: &Cell<i32>,
    windowed_height: &Cell<i32>,
) {
    if !win.is_maximized() {
        remember_current_windowed_size(win, windowed_width, windowed_height);
    }

    settings::save_window_state(
        windowed_width.get(),
        windowed_height.get(),
        win.is_maximized(),
    );
}

fn current_windowed_size(
    win: &impl gtk4::prelude::IsA<gtk4::Window>,
    fallback_width: i32,
    fallback_height: i32,
) -> (i32, i32) {
    let win = win.as_ref();
    let (default_width, default_height) = win.default_size();
    let width = valid_window_size(default_width)
        .or_else(|| valid_window_size(win.width()))
        .unwrap_or(fallback_width);
    let height = valid_window_size(default_height)
        .or_else(|| valid_window_size(win.height()))
        .unwrap_or(fallback_height);

    (width, height)
}

fn valid_window_size(value: i32) -> Option<i32> {
    (value > 0).then_some(value)
}
pub fn apply_custom_css() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let font_size_px = settings::font_size_css_px();
    let css = r#"
/* ── Window backgrounds ────────────────────────────────────── */
.app-window.dark-mode {
  background:
    radial-gradient(ellipse 64% 42% at 16% 6%,  rgba(110,99,232,0.30) 0%, transparent 100%),
    radial-gradient(ellipse 40% 28% at 84% 14%, rgba(94,86,201,0.18) 0%, transparent 100%),
    radial-gradient(ellipse 70% 55% at 50% 95%, rgba(167,165,255,0.08) 0%, transparent 100%),
    linear-gradient(162deg, #0D1030 0%, #151A45 55%, #0D1030 100%);
  color: #F2F4FF;
  font-size: __FONT_SIZE_PX__px;
}
.app-window.light-mode {
  background:
    radial-gradient(ellipse 60% 50% at 20% 0%,  rgba(207,203,255,0.30) 0%, transparent 60%),
    radial-gradient(ellipse 40% 30% at 86% 96%, rgba(183,174,238,0.16) 0%, transparent 100%),
    linear-gradient(175deg, #F8FAFF 0%, #EEF2FF 55%, #F8FAFF 100%);
  color: #1E2447;
  font-size: __FONT_SIZE_PX__px;
}

/* ── Global font size ───────────────────────────────────────── */
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

/* ── Root & HeaderBar ───────────────────────────────────────── */
.app-window .app-root { background: transparent; }
.app-window .headerbar,
.app-window headerbar {
  background: transparent;
  border: none;
  box-shadow: none;
}
.app-window .app-header-title {
  font-weight: 800;
  letter-spacing: 0;
}
.app-window .send-back-button {
  min-width: 34px;
  min-height: 34px;
  border-radius: 999px;
  padding: 0;
  opacity: 0.82;
}
.app-window .send-back-button:hover {
  opacity: 1;
  background: rgba(255,255,255,0.08);
}

/* ── Width buckets: compact phones, tablet/default, wide desktop ── */
.app-window.size-compact .headerbar,
.app-window.size-compact headerbar {
  min-height: 42px;
}
.app-window.size-wide .headerbar,
.app-window.size-wide headerbar {
  min-height: 56px;
}
.app-window.size-compact .nav-send-btn,
.app-window.size-compact .nav-recv-btn {
  padding: 11px 16px;
}
.app-window.size-wide .nav-send-btn,
.app-window.size-wide .nav-recv-btn {
  padding: 16px 24px;
}

/* ── Glass Cards ────────────────────────────────────────────── */
.app-window.dark-mode .glass-card,
.app-window.dark-mode .boxed-list {
  background: linear-gradient(150deg, rgba(255,255,255,0.08) 0%, rgba(110,99,232,0.16) 100%);
  border-radius: 18px;
  border: 1px solid rgba(167,165,255,0.14);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.10),
    inset 0 -1px 0 rgba(0,0,0,0.16),
    0 16px 44px rgba(0,0,0,0.48);
}
.app-window.light-mode .glass-card,
.app-window.light-mode .boxed-list {
  background: linear-gradient(150deg, rgba(255,255,255,0.88) 0%, rgba(242,238,255,0.78) 100%);
  border-radius: 18px;
  border: 1px solid #D9DFF3;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.95),
    0 12px 34px rgba(123,130,168,0.12);
}
.app-window .boxed-list row,
.app-window .boxed-list listitem { background: transparent; }

/* ── Drop Zone ──────────────────────────────────────────────── */
.app-window .send-drop-frame {
  padding: 0;
}
.app-window .send-drop-card {
  padding: 20px 20px;
  border-radius: 18px;
  transition: box-shadow 220ms ease, border-color 220ms ease;
}
.app-window.size-compact .send-drop-card {
  padding: 16px 14px;
  border-radius: 16px;
}
.app-window.size-wide .send-drop-card {
  padding: 34px 34px;
  border-radius: 24px;
  min-height: 270px;
}
.app-window.dark-mode .send-drop-card {
  background:
    radial-gradient(ellipse 72% 54% at 18% 18%, rgba(167,165,255,0.16) 0%, transparent 70%),
    linear-gradient(165deg, rgba(255,255,255,0.08) 0%, rgba(29,34,85,0.30) 100%);
  border: 1px solid rgba(167,165,255,0.14);
}
.app-window.light-mode .send-drop-card {
  background:
    radial-gradient(ellipse 72% 54% at 18% 18%, rgba(207,203,255,0.28) 0%, transparent 70%),
    linear-gradient(165deg, rgba(255,255,255,0.82) 0%, rgba(238,242,255,0.72) 100%);
  border: 1px solid rgba(183,174,238,0.34);
}
.app-window .send-drop-icon {
  opacity: 0.88;
  transition: transform 220ms cubic-bezier(0.34,1.56,0.64,1), opacity 200ms ease;
}
.app-window .send-drop-card.send-drop-active .send-drop-icon {
  transform: scale(1.14);
  opacity: 1.0;
}
.app-window .send-drop-title {
  font-size: 1.12em;
  font-weight: 700;
  letter-spacing: 0;
}
.app-window.dark-mode .send-drop-title { color: #F2F4FF; }
.app-window.light-mode .send-drop-title { color: #1E2447; }
.app-window .send-drop-subtitle { font-size: 0.95em; opacity: 0.76; }
.app-window .send-drop-meta     { font-size: 0.86em; opacity: 0.70; }
.app-window .send-drop-card.send-drop-active {
  border: 1.5px solid rgba(167,165,255,0.72);
  box-shadow:
    0 0 0 3px rgba(167,165,255,0.14),
    inset 0 1px 0 rgba(255,255,255,0.14),
    0 22px 44px rgba(110,99,232,0.24);
}

/* ── Select / Clear buttons ─────────────────────────────────── */
.app-window .send-select-button {
  border-radius: 999px;
  padding: 10px 28px;
  font-weight: 700;
  outline: none;
  text-decoration: none;
  text-shadow: none;
  border-style: solid;
  transition: transform 130ms cubic-bezier(0.34,1.56,0.64,1), box-shadow 160ms ease, filter 140ms ease;
}
.app-window.size-compact .send-select-button {
  padding: 8px 22px;
}
.app-window.size-wide .send-select-button {
  padding: 11px 34px;
}
.app-window .send-select-button:hover  { transform: translateY(-1px); filter: brightness(1.08); outline: none; }
.app-window .send-select-button:active { transform: translateY(0px);  filter: brightness(0.94); outline: none; }
.app-window.dark-mode .send-select-button {
  background: linear-gradient(180deg, rgba(242,244,255,0.98) 0%, rgba(217,216,255,0.96) 100%);
  border: 1px solid rgba(255,255,255,0.62);
  color: #191C45;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.72),
    0 0 18px rgba(217,216,255,0.55),
    0 5px 18px rgba(0,0,0,0.28);
}
.app-window.dark-mode .send-select-button:hover {
  background: linear-gradient(180deg, rgba(255,255,255,1.0) 0%, rgba(229,228,255,0.98) 100%);
  border: 1px solid rgba(255,255,255,0.72);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.78),
    0 0 20px rgba(217,216,255,0.60),
    0 5px 18px rgba(0,0,0,0.30);
}
.app-window.light-mode .send-select-button {
  background: linear-gradient(180deg, rgba(255,255,255,0.98) 0%, rgba(238,242,255,0.96) 100%);
  border: 1px solid rgba(183,174,238,0.52);
  color: #1E2447;
  box-shadow: 0 4px 18px rgba(123,130,168,0.22);
}
.app-window.light-mode .send-select-button:hover {
  background: linear-gradient(180deg, rgba(255,255,255,1.0) 0%, rgba(246,248,255,0.98) 100%);
  border: 1px solid rgba(183,174,238,0.64);
  box-shadow: 0 5px 20px rgba(123,130,168,0.24);
}
.app-window .clear-files-button {
  border-radius: 999px;
  min-width: 34px;
  min-height: 34px;
  padding: 2px;
  opacity: 0.78;
  transition: background 160ms ease, transform 130ms cubic-bezier(0.34,1.56,0.64,1);
}
.app-window.dark-mode  .clear-files-button { color: #ff8d86; }
.app-window.light-mode .clear-files-button { color: #c53030; }
.app-window.dark-mode  .clear-files-button:hover { background: rgba(255,107,95,0.14); transform: scale(1.06); opacity: 1; }
.app-window.light-mode .clear-files-button:hover { background: rgba(197,48,48,0.12);  transform: scale(1.06); opacity: 1; }

/* ── Selected file tiles ────────────────────────────────────── */
.app-window .selected-file-overlay  { margin: 1px 5px 5px 1px; }
.app-window .selected-file-tile {
  border-radius: 16px;
  min-width: 52px;
  min-height: 52px;
  padding: 0;
  transition: transform 160ms cubic-bezier(0.34,1.56,0.64,1);
}
.app-window.dark-mode .selected-file-tile {
  background: linear-gradient(180deg, rgba(255,255,255,0.10) 0%, rgba(110,99,232,0.22) 100%);
  border: 1px solid rgba(167,165,255,0.14);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.10), 0 8px 18px rgba(0,0,0,0.26);
}
.app-window.light-mode .selected-file-tile {
  background: linear-gradient(180deg, rgba(255,255,255,0.95) 0%, rgba(242,238,255,0.84) 100%);
  border: 1px solid #D9DFF3;
  box-shadow: 0 8px 18px rgba(123,130,168,0.12);
}
.app-window .selected-file-tile-icon { opacity: 0.95; }
.app-window .selected-file-preview   { border-radius: 12px; }
.app-window .selected-file-remove-badge {
  min-width: 14px;
  min-height: 14px;
  padding: 0;
  border-radius: 999px;
  margin-top: -1px;
  margin-right: -1px;
  box-shadow: 0 1px 4px rgba(0,0,0,0.12);
  transition: transform 130ms cubic-bezier(0.34,1.56,0.64,1), filter 120ms ease;
}
.app-window .selected-file-remove-badge image { -gtk-icon-size: 7px; opacity: 0.90; }
.app-window .selected-file-remove-badge:hover { transform: scale(1.14); filter: brightness(1.12); }
.app-window.dark-mode  .selected-file-remove-badge { background: rgb(185,28,28);  border: 1px solid rgba(255,255,255,0.72); color: white; }
.app-window.light-mode .selected-file-remove-badge { background: rgb(185,28,28);  border: 1px solid rgba(255,255,255,0.80); color: white; }

/* ── Devices Card ───────────────────────────────────────────── */
.app-window .devices-card {
  padding: 14px 14px 10px 14px;
  border-radius: 20px;
}

/* ── History ────────────────────────────────────────────────── */
.app-window .history-list {
  margin-top: 6px;
  background: transparent;
}
.app-window .history-list row {
  background: transparent;
  padding: 0;
}
.app-window .history-button {
  border-radius: 999px;
  padding: 4px 12px;
  font-size: 0.88em;
  font-weight: 700;
  transition: transform 130ms cubic-bezier(0.34,1.56,0.64,1), box-shadow 160ms ease;
}
.app-window .history-button:hover { transform: translateY(-1px); }
.app-window .history-title    { font-weight: 700; font-size: 0.98em; }
.app-window .history-subtitle { opacity: 0.70; font-size: 0.87em; }
.app-window .history-icon-button {
  border-radius: 999px;
  min-width: 34px;
  min-height: 34px;
  padding: 0;
  transition: transform 130ms cubic-bezier(0.34,1.56,0.64,1);
}
.app-window .history-icon-button image {
  -gtk-icon-size: 16px;
  opacity: 0.88;
}
.app-window .history-icon-button:hover { transform: scale(1.10); }
.app-window.dark-mode .history-icon-button {
  background: rgba(255,255,255,0.06);
  color: #D9D8FF;
  border: 1px solid rgba(167,165,255,0.12);
}
.app-window.light-mode .history-icon-button {
  background: rgba(255,255,255,0.84);
  color: #1E2447;
  border: 1px solid rgba(217,223,243,0.90);
}
.app-window .history-icon-chip {
  border-radius: 10px;
  min-width: 48px;
  min-height: 48px;
}
.app-window.dark-mode .history-icon-chip {
  background: rgba(94,86,201,0.28);
  border: 1px solid rgba(167,165,255,0.16);
}
.app-window.light-mode .history-icon-chip {
  background: rgba(255,255,255,0.92);
  border: 1px solid #D9DFF3;
}
.app-window row.history-row {
  border-radius: 10px;
  margin-bottom: 12px;
  min-height: 88px;
  transition: background 140ms ease, transform 140ms ease, box-shadow 160ms ease;
}
.app-window.dark-mode row.history-row {
  background:
    radial-gradient(ellipse 80% 90% at 10% 20%, rgba(255,255,255,0.13) 0%, transparent 70%),
    linear-gradient(145deg, rgba(255,255,255,0.14) 0%, rgba(29,34,85,0.82) 100%);
  border: 1px solid rgba(167,165,255,0.16);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.12),
    0 10px 24px rgba(0,0,0,0.20);
}
.app-window.light-mode row.history-row {
  background:
    radial-gradient(ellipse 80% 90% at 10% 20%, rgba(255,255,255,0.90) 0%, transparent 70%),
    linear-gradient(145deg, rgba(255,255,255,0.96) 0%, rgba(238,242,255,0.88) 100%);
  border: 1px solid rgba(217,223,243,0.94);
  box-shadow: 0 9px 22px rgba(123,130,168,0.14);
}
.app-window.dark-mode row.history-row:hover { transform: translateY(-1px); background: rgba(45,52,120,0.82); }
.app-window.light-mode row.history-row:hover { transform: translateY(-1px); background: rgba(248,250,255,1.0); }

/* ── Caption & section headings ─────────────────────────────── */
.app-window .caption-heading {
  letter-spacing: 0.02em;
  font-size: 0.92em;
  font-weight: 700;
  opacity: 0.72;
  margin-top: 6px;
  margin-bottom: 6px;
}

/* ── Generic row hover ──────────────────────────────────────── */
.app-window.dark-mode  .boxed-list row:hover { background: color-mix(in srgb, #A7A5FF 9%, transparent); }
.app-window.light-mode .boxed-list row:hover { background: color-mix(in srgb, #CFCBFF 22%, white); }

/* ── Transfer states ────────────────────────────────────────── */
.app-window .boxed-list row.transfer-row.transfer-active {
  background: color-mix(in srgb, #5b8ef8 12%, transparent);
  border-radius: 14px;
  border: 1px solid color-mix(in srgb, #93bafd 15%, transparent);
}
.app-window .boxed-list row.transfer-row.transfer-success {
  background: color-mix(in srgb, #22c55e 16%, transparent);
  border-radius: 14px;
  border: 1px solid color-mix(in srgb, #86efac 24%, transparent);
}
.app-window .boxed-list row.transfer-row.transfer-success:hover {
  background: color-mix(in srgb, #22c55e 22%, transparent);
}
.app-window .boxed-list row.transfer-row.transfer-error {
  background: color-mix(in srgb, #ef4444 14%, transparent);
  border-radius: 14px;
  border: 1px solid color-mix(in srgb, #fca5a5 22%, transparent);
}
.app-window .boxed-list row.transfer-row.transfer-error:hover {
  background: color-mix(in srgb, #ef4444 20%, transparent);
}

/* ── Progress bar ───────────────────────────────────────────── */
.app-window progressbar { border-radius: 6px; }
.app-window progressbar trough {
  background: rgba(255,255,255,0.11);
  border-radius: 6px;
  min-height: 5px;
  border: none;
  box-shadow: none;
}
.app-window.light-mode progressbar trough {
  background: #D9DFF3;
}
.app-window progressbar trough progress {
  background: linear-gradient(90deg, #7c52f0, #4ac5f5);
  border-radius: 6px;
  border: none;
  box-shadow: 0 0 8px rgba(122,82,240,0.42);
  transition: min-width 120ms ease;
}

/* ── PIN badge ──────────────────────────────────────────────── */
.app-window .pin-badge {
  font-size: 0.83em;
  font-weight: 700;
  letter-spacing: 0.08em;
  padding: 4px 11px;
  border-radius: 999px;
}
.app-window.dark-mode .pin-badge {
  color: #D9D8FF;
  background: linear-gradient(180deg, rgba(255,255,255,0.12) 0%, rgba(110,99,232,0.26) 100%);
  border: 1px solid rgba(167,165,255,0.20);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.14), 0 4px 12px rgba(13,16,48,0.36);
}
.app-window.light-mode .pin-badge {
  color: #1E2447;
  background: linear-gradient(180deg, rgba(255,255,255,0.95) 0%, rgba(207,203,255,0.44) 100%);
  border: 1px solid #D9DFF3;
  box-shadow: 0 4px 12px rgba(123,130,168,0.14);
}

.app-window .risk-badge {
  font-size: 0.78em;
  font-weight: 800;
  letter-spacing: 0.02em;
  padding: 3px 9px;
  border-radius: 999px;
}
.app-window.dark-mode .risk-badge {
  color: #FFE7A3;
  background: linear-gradient(180deg, rgba(245,158,11,0.34) 0%, rgba(120,53,15,0.40) 100%);
  border: 1px solid rgba(251,191,36,0.44);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.14), 0 4px 12px rgba(13,16,48,0.28);
}
.app-window.light-mode .risk-badge {
  color: #78350F;
  background: linear-gradient(180deg, rgba(254,243,199,0.98) 0%, rgba(251,191,36,0.42) 100%);
  border: 1px solid rgba(217,119,6,0.32);
  box-shadow: 0 4px 12px rgba(146,64,14,0.12);
}
.app-window.dark-mode .risk-badge.high-risk-badge {
  color: #FFE4E6;
  background: linear-gradient(180deg, rgba(239,68,68,0.42) 0%, rgba(127,29,29,0.48) 100%);
  border: 1px solid rgba(252,165,165,0.48);
}
.app-window.light-mode .risk-badge.high-risk-badge {
  color: #7F1D1D;
  background: linear-gradient(180deg, rgba(254,226,226,0.98) 0%, rgba(248,113,113,0.38) 100%);
  border: 1px solid rgba(185,28,28,0.30);
}

/* ── Visibility icons ───────────────────────────────────────── */
.app-window .visibility-visible {
  color: #4de8c2;
  -gtk-icon-shadow:
    0 0 8px  rgba(77,232,194,0.95),
    0 0 18px rgba(77,232,194,0.62),
    0 0 32px rgba(77,232,194,0.34);
}
.app-window .visibility-hidden {
  color: #f87171;
  -gtk-icon-shadow:
    0 0 8px  rgba(248,113,113,0.85),
    0 0 18px rgba(248,113,113,0.50),
    0 0 28px rgba(248,113,113,0.26);
}
.app-window .visibility-temporary { color: #cbd5e1; }

/* ── Status page ────────────────────────────────────────────── */
.app-window.dark-mode  .status-page { color: #D9D8FF; }
.app-window.light-mode .status-page { color: #7B82A8; }

/* ── Pill Switcher ──────────────────────────────────────────── */
.app-window.dark-mode .pill-switcher {
  background: linear-gradient(180deg, rgba(255,255,255,0.08) 0%, rgba(110,99,232,0.22) 100%);
  border-radius: 16px;
  padding: 2px;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.10),
    inset 0 -1px 0 rgba(0,0,0,0.18),
    0 16px 38px rgba(0,0,0,0.50);
  border: 1px solid rgba(167,165,255,0.13);
  outline: 1px solid transparent;
}
.app-window.light-mode .pill-switcher {
  background: linear-gradient(180deg, rgba(207,203,255,0.32) 0%, rgba(255,255,255,0.78) 100%);
  border-radius: 12px;
  padding: 2px;
  box-shadow: 0 10px 28px rgba(123,130,168,0.16);
  border: 1px solid #D9DFF3;
  outline: 1px solid transparent;
}
.app-window .pill-switcher .viewswitcher { background: transparent; }
.app-window .pill-switcher .viewswitcher button {
  min-height: 32px;
  min-width: 84px;
  padding: 6px 10px;
  border-radius: 10px;
  box-shadow: none;
  border: none;
  outline: none;
  transition: background 160ms ease, color 160ms ease, box-shadow 160ms ease;
}
.app-window .pill-switcher .viewswitcher button:focus,
.app-window .pill-switcher .viewswitcher button:focus-visible,
.app-window .pill-switcher .viewswitcher button:focus-within { outline: none; box-shadow: none; }
.app-window.dark-mode  .pill-switcher .viewswitcher button { color: rgba(217,216,255,0.72); }
.app-window.light-mode .pill-switcher .viewswitcher button { color: rgba(30,36,71,0.75); }
.app-window.dark-mode  .pill-switcher .viewswitcher button:hover {
  background: linear-gradient(180deg, rgba(167,165,255,0.14) 0%, rgba(167,165,255,0.05) 100%);
  color: #D9D8FF;
}
.app-window.light-mode .pill-switcher .viewswitcher button:hover {
  background: linear-gradient(180deg, rgba(207,203,255,0.24) 0%, rgba(255,255,255,0.60) 100%);
  color: #1E2447;
}
.app-window.dark-mode .pill-switcher .viewswitcher button:checked {
  background: linear-gradient(165deg, rgba(110,99,232,0.96) 0%, rgba(94,86,201,0.98) 100%);
  color: #fff;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.22),
    inset 0 -1px 0 rgba(0,0,0,0.16),
    0 4px 14px rgba(94,86,201,0.48);
}
.app-window.light-mode .pill-switcher .viewswitcher button:checked {
  background: linear-gradient(165deg, rgba(110,99,232,0.92) 0%, rgba(94,86,201,0.96) 100%);
  color: #fff;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.30),
    inset 0 -1px 0 rgba(0,0,0,0.06),
    0 6px 18px rgba(110,99,232,0.26);
}
.app-window .pill-switcher .viewswitcher button label { font-size: 1.0em; font-weight: 600; }

/* ── Device Tiles ───────────────────────────────────────────── */
.app-window.dark-mode .device-tile {
  padding: 0;
  background:
    radial-gradient(ellipse 84% 70% at 16% 10%, rgba(167,165,255,0.18) 0%, transparent 74%),
    linear-gradient(155deg, rgba(94,86,201,0.24) 0%, rgba(29,34,85,0.42) 100%);
  border-radius: 16px;
  border: 1px solid rgba(167,165,255,0.16);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.10), 0 10px 28px rgba(0,0,0,0.36);
  transition:
    background 200ms ease,
    box-shadow 220ms ease,
    transform  200ms cubic-bezier(0.34,1.56,0.64,1),
    border-color 200ms ease;
}
.app-window.dark-mode .device-tile:hover {
  background:
    radial-gradient(ellipse 86% 72% at 16% 10%, rgba(167,165,255,0.26) 0%, transparent 74%),
    linear-gradient(155deg, rgba(110,99,232,0.36) 0%, rgba(45,52,120,0.46) 100%);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.13),
    0 18px 44px rgba(0,0,0,0.42),
    0 0 0 1px rgba(167,165,255,0.16);
  transform: translateY(-3px);
}
.app-window.dark-mode .device-tile:active { transform: translateY(-1px); }
.app-window.light-mode .device-tile {
  padding: 0;
  background:
    radial-gradient(ellipse 84% 70% at 16% 10%, rgba(207,203,255,0.36) 0%, transparent 74%),
    linear-gradient(155deg, rgba(255,255,255,0.92) 0%, rgba(244,246,253,0.97) 100%);
  color: #1E2447;
  border-radius: 16px;
  border: 1px solid #D9DFF3;
  box-shadow: 0 8px 26px rgba(123,130,168,0.14);
  transition:
    background 200ms ease,
    box-shadow 220ms ease,
    transform  200ms cubic-bezier(0.34,1.56,0.64,1);
}
.app-window.light-mode .device-tile:hover {
  background: linear-gradient(155deg, rgba(255,255,255,0.98) 0%, rgba(248,250,255,1.0) 100%);
  box-shadow: 0 20px 44px rgba(123,130,168,0.20);
  transform: translateY(-3px);
}
.app-window.light-mode .device-tile:active { transform: translateY(-1px); }
.app-window.light-mode .device-tile image,
.app-window.light-mode .device-tile label { color: #1E2447; }
.app-window.dark-mode  .device-tile image,
.app-window.dark-mode  .device-tile label { color: #F2F4FF; }

/* icon circle background */
.app-window.dark-mode .device-icon-circle {
  background: linear-gradient(145deg, rgba(255,255,255,0.14) 0%, rgba(110,99,232,0.38) 100%);
  border-radius: 999px;
  border: 1px solid rgba(167,165,255,0.20);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.18),
    0 6px 18px rgba(0,0,0,0.38);
}
.app-window.light-mode .device-icon-circle {
  background: linear-gradient(145deg, rgba(255,255,255,0.97) 0%, rgba(207,203,255,0.52) 100%);
  border-radius: 999px;
  border: 1px solid #D9DFF3;
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,1.0),
    0 6px 18px rgba(123,130,168,0.14);
}

.app-window .device-tile-title {
  font-weight: 800;
  letter-spacing: 0;
}
.app-window .device-tile-meta  { font-size: 0.80em; opacity: 0.68; }
.app-window .device-transport-row {
  opacity: 0.74;
  margin-top: 0;
}
.app-window .device-transport-label {
  font-size: 0.86em;
  font-weight: 600;
  letter-spacing: 0;
}
.app-window.dark-mode  .transport-wifi,
.app-window.dark-mode  .transport-wifi image,
.app-window.dark-mode  .transport-wifi label { color: #D9D8FF; }
.app-window.light-mode .transport-wifi,
.app-window.light-mode .transport-wifi image,
.app-window.light-mode .transport-wifi label { color: rgba(30,36,71,0.72); }
.app-window.dark-mode  .transport-wifi-direct,
.app-window.dark-mode  .transport-wifi-direct image,
.app-window.dark-mode  .transport-wifi-direct label { color: #80f5e0; }
.app-window.light-mode .transport-wifi-direct,
.app-window.light-mode .transport-wifi-direct image,
.app-window.light-mode .transport-wifi-direct label { color: #0f7c65; }
.app-window.size-compact .device-tile {
  border-radius: 14px;
}

/* ── Bottom Navigation Bar ──────────────────────────────────── */
.app-window .nav-bar { /* transparent container */ }

.app-window .nav-send-btn {
  border-radius: 16px;
  padding: 14px 20px;
  font-size: 1.02em;
  transition: background 180ms ease, box-shadow 200ms ease, transform 130ms cubic-bezier(0.34,1.56,0.64,1);
}
.app-window .nav-send-btn:hover  { transform: translateY(-1px); }
.app-window .nav-send-btn:active { transform: translateY(0px); filter: brightness(0.92); }
.app-window.dark-mode .nav-send-btn {
  background: rgba(255,255,255,0.05);
  border: 1px solid rgba(167,165,255,0.13);
  color: rgba(217,216,255,0.82);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.07), 0 4px 14px rgba(0,0,0,0.28);
}
.app-window.light-mode .nav-send-btn {
  background: rgba(255,255,255,0.82);
  border: 1px solid #D9DFF3;
  color: rgba(30,36,71,0.80);
  box-shadow: 0 4px 14px rgba(123,130,168,0.12);
}
.app-window.dark-mode .nav-send-btn.nav-btn-active {
  background: linear-gradient(145deg, rgba(110,99,232,0.94) 0%, rgba(94,86,201,0.98) 100%);
  border: 1px solid rgba(167,165,255,0.26);
  color: #fff;
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.22), 0 8px 24px rgba(94,86,201,0.50);
}
.app-window.light-mode .nav-send-btn.nav-btn-active {
  background: linear-gradient(145deg, rgba(110,99,232,0.92) 0%, rgba(94,86,201,0.96) 100%);
  border: 1px solid rgba(207,203,255,0.50);
  color: #fff;
  box-shadow: 0 8px 24px rgba(110,99,232,0.28);
}
.app-window .nav-btn-label { font-weight: 700; }

/* Receive row (pill container) */
.app-window .nav-recv-row {
  border-radius: 16px;
  transition: background 180ms ease, border-color 180ms ease, box-shadow 200ms ease;
}
.app-window.dark-mode .nav-recv-row {
  background: rgba(255,255,255,0.05);
  border: 1px solid rgba(167,165,255,0.13);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.06), 0 4px 14px rgba(0,0,0,0.28);
}
.app-window.light-mode .nav-recv-row {
  background: rgba(255,255,255,0.82);
  border: 1px solid #D9DFF3;
  box-shadow: 0 4px 14px rgba(123,130,168,0.12);
}
.app-window.dark-mode .nav-recv-row.nav-btn-active {
  background: linear-gradient(145deg, rgba(110,99,232,0.94) 0%, rgba(94,86,201,0.98) 100%);
  border: 1px solid rgba(167,165,255,0.26);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.20), 0 8px 24px rgba(94,86,201,0.50);
}
.app-window.light-mode .nav-recv-row.nav-btn-active {
  background: linear-gradient(145deg, rgba(110,99,232,0.92) 0%, rgba(94,86,201,0.96) 100%);
  border: 1px solid rgba(207,203,255,0.50);
  box-shadow: 0 8px 24px rgba(110,99,232,0.28);
}
/* Flat button inside recv row */
.app-window .nav-recv-btn {
  border-radius: 14px;
  padding: 14px 20px;
  font-size: 1.02em;
  border: none;
  box-shadow: none;
  transition: background 150ms ease;
}
.app-window .nav-recv-btn:hover { background: rgba(255,255,255,0.06); }
.app-window.dark-mode  .nav-recv-row .nav-recv-btn { color: rgba(217,216,255,0.82); }
.app-window.light-mode .nav-recv-row .nav-recv-btn { color: rgba(30,36,71,0.80); }
.app-window.dark-mode  .nav-recv-row.nav-btn-active .nav-recv-btn { color: #fff; }
.app-window.light-mode .nav-recv-row.nav-btn-active .nav-recv-btn { color: #fff; }

/* ── Receive ready card ─────────────────────────────────────── */
.app-window .recv-ready-card {
  padding: 8px 18px 24px 18px;
  border-radius: 26px;
  border: 2px solid rgba(167,165,255,0.16);
}
.app-window.size-compact .recv-ready-card {
  padding: 4px 12px 18px 12px;
  border-radius: 22px;
}
.app-window.size-wide .recv-ready-card {
  padding: 18px 36px 38px 36px;
  border-radius: 30px;
  min-height: 390px;
}
.app-window.dark-mode .recv-ready-card {
  background:
    radial-gradient(ellipse 70% 52% at 18% 12%, rgba(167,165,255,0.08) 0%, transparent 72%),
    linear-gradient(175deg, rgba(39,33,66,0.68) 0%, rgba(20,17,38,0.88) 100%);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.06),
    0 24px 64px rgba(0,0,0,0.46);
}
.app-window.light-mode .recv-ready-card {
  background:
    radial-gradient(ellipse 70% 52% at 18% 12%, rgba(207,203,255,0.20) 0%, transparent 72%),
    linear-gradient(175deg, rgba(248,250,255,0.96) 0%, rgba(238,242,255,0.88) 100%);
  border: 2px solid #D9DFF3;
  box-shadow:
    0 16px 48px rgba(123,130,168,0.16);
}
.app-window .recv-ready-title-plain {
  font-size: 2.55em;
  font-weight: 900;
  letter-spacing: 0;
  line-height: 1.10;
}
.app-window.size-compact .recv-ready-title-plain { font-size: 2.08em; }
.app-window.size-wide .recv-ready-title-plain { font-size: 2.9em; }
.app-window.dark-mode  .recv-ready-title-plain { color: #F2F4FF; }
.app-window.light-mode .recv-ready-title-plain { color: #1E2447; }
.app-window .recv-ready-title-accent {
  font-size: 2.55em;
  font-weight: 900;
  letter-spacing: 0;
  line-height: 1.10;
}
.app-window.size-compact .recv-ready-title-accent { font-size: 2.08em; }
.app-window.size-wide .recv-ready-title-accent { font-size: 2.9em; }
.app-window.dark-mode  .recv-ready-title-accent { color: #A7A5FF; }
.app-window.light-mode .recv-ready-title-accent { color: #5E56C9; }
.app-window .recv-vis-indicator {
  margin-top: 2px;
}
.app-window .recv-vis-label { font-size: 0.92em; font-weight: 600; }
.app-window .recv-vis-btn {
  border-radius: 999px;
  padding: 7px 15px;
  opacity: 0.80;
  transition: opacity 160ms ease, background 160ms ease, transform 130ms cubic-bezier(0.34,1.56,0.64,1);
}
.app-window .recv-vis-btn:hover  { opacity: 1.0; transform: scale(1.04); }
.app-window .recv-vis-btn:active { transform: scale(0.97); }
.app-window.dark-mode .recv-vis-btn:hover  { background: rgba(167,165,255,0.12); }
.app-window.light-mode .recv-vis-btn:hover { background: rgba(207,203,255,0.32); }

.app-window .send-drop-card.send-drop-active {
  border: 1.5px solid rgba(118,194,255,0.68);
}

/* ── File icon chips inside file rows ───────────────────────────── */
.app-window .send-file-icon-chip { border-radius: 10px; }
.app-window.dark-mode .send-file-icon-chip {
  background: rgba(94,86,201,0.28);
  border: 1px solid rgba(167,165,255,0.16);
}
.app-window.light-mode .send-file-icon-chip {
  background: rgba(255,255,255,0.92);
  border: 1px solid #D9DFF3;
}

/* ── Selected files list rows ───────────────────────────────── */
.app-window .selected-file-row {
  border-radius: 10px;
  margin-bottom: 12px;
  transition: background 140ms ease, transform 140ms ease, box-shadow 160ms ease;
}
.app-window.size-compact .selected-file-row {
  margin-bottom: 9px;
}
.app-window.size-wide .selected-file-row {
  border-radius: 12px;
  margin-bottom: 14px;
}
.app-window.dark-mode .selected-file-row {
  background:
    radial-gradient(ellipse 80% 90% at 10% 20%, rgba(255,255,255,0.13) 0%, transparent 70%),
    linear-gradient(145deg, rgba(255,255,255,0.14) 0%, rgba(29,34,85,0.82) 100%);
  border: 1px solid rgba(167,165,255,0.16);
  box-shadow:
    inset 0 1px 0 rgba(255,255,255,0.12),
    0 10px 24px rgba(0,0,0,0.28);
}
.app-window.light-mode .selected-file-row {
  background:
    radial-gradient(ellipse 80% 90% at 10% 20%, rgba(255,255,255,0.90) 0%, transparent 70%),
    linear-gradient(145deg, rgba(255,255,255,0.96) 0%, rgba(238,242,255,0.88) 100%);
  border: 1px solid rgba(217,223,243,0.94);
  box-shadow: 0 9px 22px rgba(123,130,168,0.16);
}
.app-window.dark-mode  .selected-file-row:hover { transform: translateY(-1px); background: rgba(45,52,120,0.82); }
.app-window.light-mode .selected-file-row:hover { transform: translateY(-1px); background: rgba(248,250,255,1.0); }
.app-window .selected-files-heading {
  font-size: 1.1em;
  font-weight: 800;
}
.app-window.size-compact .selected-files-heading { font-size: 1.0em; }
.app-window.size-wide .selected-files-heading { font-size: 1.2em; }
.app-window .selected-file-row-name {
  font-weight: 600;
  font-size: 0.98em;
}
.app-window .selected-file-row-size {
  font-size: 0.82em;
  opacity: 0.64;
}
.app-window .selected-files-list {
  border-radius: 10px;
  background: transparent;
}
.app-window .selected-files-list row {
  background: transparent;
  padding: 0;
}
.app-window .selected-file-remove-button {
  min-width: 34px;
  min-height: 34px;
  padding: 0;
  border-radius: 999px;
  opacity: 0.72;
}
.app-window .selected-file-remove-button:hover {
  opacity: 1;
  background: rgba(255,255,255,0.08);
}
"#
    .replace("__FONT_SIZE_PX__", &font_size_px.to_string());

    APP_CSS_PROVIDER.with(|cell| {
        let provider = cell
            .borrow_mut()
            .take()
            .unwrap_or_else(gtk4::CssProvider::new);
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

fn install_responsive_classes(win: &libadwaita::ApplicationWindow) {
    let current_bucket = Rc::new(RefCell::new(String::new()));
    let current_bucket_ref = Rc::clone(&current_bucket);

    win.add_tick_callback(move |widget, _| {
        let width = widget.width();
        if width <= 0 {
            return glib::ControlFlow::Continue;
        }

        let next_bucket = if width < 430 {
            "size-compact"
        } else if width < 760 {
            "size-tablet"
        } else {
            "size-wide"
        };

        let mut current = current_bucket_ref.borrow_mut();
        if current.as_str() != next_bucket {
            widget.remove_css_class("size-compact");
            widget.remove_css_class("size-tablet");
            widget.remove_css_class("size-wide");
            widget.add_css_class(next_bucket);
            *current = next_bucket.to_string();
        }

        glib::ControlFlow::Continue
    });
}

fn send_notification(app: Option<&libadwaita::Application>, sender_name: &str, transfer_id: &str) {
    let Some(app) = app else { return };
    let n = gio::Notification::new("GnomeQS");
    n.set_body(Some(&format!("{sender_name} {}", tr!("wants to share"))));

    let id_variant = glib::Variant::from(transfer_id);
    n.add_button_with_target_value(&tr!("Accept"), "app.accept-transfer", Some(&id_variant));
    n.add_button_with_target_value(&tr!("Decline"), "app.reject-transfer", Some(&id_variant));

    app.send_notification(Some(transfer_id), &n);
}
