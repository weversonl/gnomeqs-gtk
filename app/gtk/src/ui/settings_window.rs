use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gio::prelude::SettingsExt;
use libadwaita::prelude::*;

use crate::bridge::FromUi;
use crate::settings;
use crate::tr;
use crate::config::VERSION;
use crate::ui::cursor::set_pointer_cursor;
use crate::ui::window::apply_custom_css;
use gnomeqs_core::{WifiDirectStatus, detect_wifi_direct_capability};

pub fn build_settings_window(
    parent: &impl gtk4::prelude::IsA<gtk4::Window>,
    from_ui_tx: async_channel::Sender<FromUi>,
) -> libadwaita::PreferencesDialog {
    let win = libadwaita::PreferencesDialog::new();
    let app = parent
        .application()
        .and_then(|app| app.downcast::<libadwaita::Application>().ok());
    win.set_title(&tr!("Settings"));
    win.set_search_enabled(false);
    register_window_actions(&win, app);

    win.add(&build_general_page(&from_ui_tx, &win));
    win.add(&build_about_page());

    win
}

fn build_general_page(
    from_ui_tx: &async_channel::Sender<FromUi>,
    win: &libadwaita::PreferencesDialog,
) -> libadwaita::PreferencesPage {
    let page = libadwaita::PreferencesPage::new();
    page.set_title(&tr!("General"));
    page.set_icon_name(Some("preferences-system-symbolic"));

    page.add(&build_behavior_group(from_ui_tx));
    page.add(&build_appearance_group(win));
    page.add(&build_files_group(from_ui_tx, win));
    page.add(&build_history_group());
    page.add(&build_network_group(win));

    page
}

fn build_behavior_group(_from_ui_tx: &async_channel::Sender<FromUi>) -> libadwaita::PreferencesGroup {
    let group = libadwaita::PreferencesGroup::new();
    group.set_title(&tr!("Behavior"));
    let gsettings = settings();

    let autostart = libadwaita::SwitchRow::new();
    autostart.set_title(&tr!("Start on boot"));
    autostart.set_active(settings::get_autostart());
    set_pointer_cursor(&autostart);
    {
        let gsettings = gsettings.clone();
        autostart.connect_active_notify(move |row| {
            let enabled = row.is_active();
            let _ = gsettings.set_boolean("autostart", enabled);
            if let Err(e) = settings::set_autostart(enabled) {
                log::warn!("set_autostart failed: {e}");
            }
        });
    }
    group.add(&autostart);

    let keep_running = libadwaita::SwitchRow::new();
    keep_running.set_title(&tr!("Keep running on close"));
    keep_running.set_active(settings::get_keep_running_on_close());
    set_pointer_cursor(&keep_running);
    gsettings.bind("keep-running-on-close", &keep_running, "active").build();
    group.add(&keep_running);

    let start_min = libadwaita::SwitchRow::new();
    start_min.set_title(&tr!("Start minimized"));
    start_min.set_active(gsettings.boolean("start-minimized"));
    set_pointer_cursor(&start_min);
    gsettings.bind("start-minimized", &start_min, "active").build();
    group.add(&start_min);

    let mono_tray = libadwaita::SwitchRow::new();
    mono_tray.set_title(&tr!("Monochrome tray icon"));
    mono_tray.set_active(gsettings.boolean("tray-monochrome"));
    set_pointer_cursor(&mono_tray);
    gsettings.bind("tray-monochrome", &mono_tray, "active").build();
    group.add(&mono_tray);

    group
}

fn build_appearance_group(win: &libadwaita::PreferencesDialog) -> libadwaita::PreferencesGroup {
    let group = libadwaita::PreferencesGroup::new();
    group.set_title(&tr!("Appearance"));
    let gsettings = settings();

    let theme_items = vec![tr!("System"), tr!("Light"), tr!("Dark")];
    let theme_item_refs: Vec<&str> = theme_items.iter().map(String::as_str).collect();
    let theme_row = libadwaita::ComboRow::new();
    theme_row.set_title(&tr!("Theme"));
    set_pointer_cursor(&theme_row);
    let theme_model = gtk4::StringList::new(&theme_item_refs);
    theme_row.set_model(Some(&theme_model));
    let current_scheme = gsettings.string("color-scheme");
    theme_row.set_selected(match current_scheme.as_str() {
        "light"  => 1,
        "dark"   => 2,
        _        => 0,
    });
    {
        let gsettings = gsettings.clone();
        theme_row.connect_selected_notify(move |row| {
            let scheme = match row.selected() {
                1 => "light",
                2 => "dark",
                _ => "system",
            };
            let _ = gsettings.set_string("color-scheme", scheme);
            settings::apply_color_scheme();
        });
    }
    group.add(&theme_row);

    let lang_items = vec![tr!("English"), tr!("Portuguese (Brazil)")];
    let lang_item_refs: Vec<&str> = lang_items.iter().map(String::as_str).collect();
    let lang_row = libadwaita::ComboRow::new();
    lang_row.set_title(&tr!("Language"));
    lang_row.set_subtitle(&tr!("Restart required to apply"));
    set_pointer_cursor(&lang_row);
    let lang_model = gtk4::StringList::new(&lang_item_refs);
    lang_row.set_model(Some(&lang_model));
    let current_lang = gsettings.string("language");
    lang_row.set_selected(match current_lang.as_str() {
        "pt_BR" => 1,
        _       => 0,
    });
    {
        let gsettings = gsettings.clone();
        let win = win.clone();
        lang_row.connect_selected_notify(move |row| {
            let lang = match row.selected() {
                1 => "pt_BR",
                _ => "en",
            };
            let _ = gsettings.set_string("language", lang);
            show_restart_toast(&win, &tr!("Restart required to apply"));
        });
    }
    group.add(&lang_row);

    let font_items = vec![tr!("Small"), tr!("Normal"), tr!("Large"), tr!("Extra large")];
    let font_item_refs: Vec<&str> = font_items.iter().map(String::as_str).collect();
    let font_row = libadwaita::ComboRow::new();
    font_row.set_title(&tr!("Font size"));
    set_pointer_cursor(&font_row);
    let font_model = gtk4::StringList::new(&font_item_refs);
    font_row.set_model(Some(&font_model));
    font_row.set_selected(gsettings.int("font-size") as u32);
    {
        let gsettings = gsettings.clone();
        font_row.connect_selected_notify(move |row| {
            let size = row.selected() as i32;
            let _ = gsettings.set_int("font-size", size);
            apply_custom_css();
        });
    }
    group.add(&font_row);

    group
}

fn build_files_group(
    from_ui_tx: &async_channel::Sender<FromUi>,
    _win: &libadwaita::PreferencesDialog,
) -> libadwaita::PreferencesGroup {
    let group = libadwaita::PreferencesGroup::new();
    group.set_title(&tr!("Files"));

    let folder_row = libadwaita::ActionRow::new();
    folder_row.set_title(&tr!("Download folder"));
    let current = settings::get_download_folder();
    folder_row.set_subtitle(
        current.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .as_deref()
            .unwrap_or("Default"),
    );

    let pick_btn = gtk4::Button::from_icon_name("folder-open-symbolic");
    pick_btn.add_css_class("flat");
    pick_btn.set_valign(gtk4::Align::Center);
    set_pointer_cursor(&pick_btn);
    folder_row.add_suffix(&pick_btn);
    folder_row.set_activatable_widget(Some(&pick_btn));

    {
        let tx = from_ui_tx.clone();
        let row = folder_row.clone();
        pick_btn.connect_clicked(move |btn| {
            let window = btn.root().and_downcast::<gtk4::Window>();
            let dialog = gtk4::FileDialog::new();
            dialog.set_title(&tr!("Change download folder"));
            let tx2 = tx.clone();
            let row2 = row.clone();
            dialog.select_folder(window.as_ref(), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let path_str = path.to_string_lossy().into_owned();
                        row2.set_subtitle(&path_str);
                        let _ = settings().set_string("download-folder", &path_str);
                        if let Err(e) = tx2.try_send(FromUi::ChangeDownloadPath(Some(path))) {
                            log::warn!("ChangeDownloadPath: {e}");
                        }
                    }
                }
            });
        });
    }

    group.add(&folder_row);
    group
}

fn build_history_group() -> libadwaita::PreferencesGroup {
    let group = libadwaita::PreferencesGroup::new();
    group.set_title(&tr!("History"));
    group.set_description(Some(&tr!(
        "Transfer history is stored locally and is automatically removed after the configured time. Default: 50 items for 7 days."
    )));
    let gsettings = settings();

    let retention_row = libadwaita::SpinRow::new(
        Some(&gtk4::Adjustment::new(
            settings::get_history_retention_days() as f64,
            1.0,
            365.0,
            1.0,
            7.0,
            0.0,
        )),
        1.0,
        0,
    );
    retention_row.set_title(&tr!("Keep history for"));
    retention_row.set_subtitle(&tr!("days"));
    set_pointer_cursor(&retention_row);
    gsettings
        .bind("history-retention-days", &retention_row, "value")
        .build();
    group.add(&retention_row);

    let max_items_row = libadwaita::SpinRow::new(
        Some(&gtk4::Adjustment::new(
            settings::get_history_max_items() as f64,
            1.0,
            500.0,
            1.0,
            25.0,
            0.0,
        )),
        1.0,
        0,
    );
    max_items_row.set_title(&tr!("History size"));
    max_items_row.set_subtitle(&tr!("maximum items"));
    set_pointer_cursor(&max_items_row);
    gsettings
        .bind("history-max-items", &max_items_row, "value")
        .build();
    group.add(&max_items_row);

    group
}

fn build_network_group(win: &libadwaita::PreferencesDialog) -> libadwaita::PreferencesGroup {
    let group = libadwaita::PreferencesGroup::new();
    group.set_title(&tr!("Network"));
    let gsettings = settings();

    let port_row = libadwaita::SpinRow::new(
        Some(&gtk4::Adjustment::new(
            gsettings.int("port") as f64,
            0.0,
            65535.0,
            1.0,
            100.0,
            0.0,
        )),
        1.0,
        0,
    );
    port_row.set_title(&tr!("Port"));
    port_row.set_subtitle(&tr!("Restart required to apply"));
    set_pointer_cursor(&port_row);
    gsettings.bind("port", &port_row, "value").build();

    {
        let win_ref = win.clone();
        let pending_toast: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        port_row.connect_value_notify(move |_| {
            if let Some(source) = pending_toast.borrow_mut().take() {
                source.remove();
            }

            let win_ref = win_ref.clone();
            let pending_toast = pending_toast.clone();
            let pending_toast_for_timeout = pending_toast.clone();
            let source = glib::timeout_add_local(Duration::from_millis(700), move || {
                *pending_toast_for_timeout.borrow_mut() = None;
                show_restart_toast(&win_ref, &tr!("Restart required to apply"));
                glib::ControlFlow::Break
            });

            *pending_toast.borrow_mut() = Some(source);
        });
    }

    group.add(&port_row);

    let wifi_direct = detect_wifi_direct_capability();
    let wifi_row = libadwaita::SwitchRow::new();
    wifi_row.set_title(&tr!("Wi-Fi Direct (experimental)"));
    wifi_row.set_subtitle(&wifi_direct_subtitle(&wifi_direct));
    wifi_row.set_active(gsettings.boolean("wifi-direct-enabled") && wifi_direct.available);
    wifi_row.set_sensitive(wifi_direct.available);

    if wifi_direct.available {
        set_pointer_cursor(&wifi_row);
        gsettings.bind("wifi-direct-enabled", &wifi_row, "active").build();
    } else {
        let _ = gsettings.set_boolean("wifi-direct-enabled", false);
    }

    group.add(&wifi_row);
    group
}

fn build_about_page() -> libadwaita::PreferencesPage {
    let page = libadwaita::PreferencesPage::new();
    page.set_title(&tr!("About"));
    page.set_icon_name(Some("help-about-symbolic"));

    let group = libadwaita::PreferencesGroup::new();

    let version_row = libadwaita::ActionRow::new();
    version_row.set_title(&tr!("Version"));
    version_row.set_subtitle(VERSION);
    group.add(&version_row);

    page.add(&group);
    page
}

fn settings() -> gio::Settings {
    crate::settings::settings()
}

fn wifi_direct_subtitle(capability: &gnomeqs_core::WifiDirectCapability) -> String {
    match capability.status {
        WifiDirectStatus::BackendMissing => {
            tr!("Your device is not compatible with Wi-Fi Direct via NetworkManager.")
        }
        WifiDirectStatus::BackendNotRunning => {
            tr!("Your device is not compatible with Wi-Fi Direct right now because NetworkManager is not running.")
        }
        WifiDirectStatus::BackendQueryFailed => {
            tr!("Your device compatibility with Wi-Fi Direct could not be verified right now.")
        }
        WifiDirectStatus::NoWifiInterface => {
            tr!("Your device is not compatible with Wi-Fi Direct because no Wi-Fi interface was detected.")
        }
        WifiDirectStatus::WifiInterfaceUnavailable => {
            tr!("Your device is not compatible with Wi-Fi Direct right now because the Wi-Fi interface is unavailable.")
        }
        WifiDirectStatus::NoP2pInterface => {
            tr!("Your device is not compatible with Wi-Fi Direct because no P2P interface was detected.")
        }
        WifiDirectStatus::P2pInterfaceUnavailable => {
            tr!("Your device is compatible with Wi-Fi Direct through NetworkManager.")
        }
        WifiDirectStatus::P2pInterfaceAvailable => {
            tr!("Your device is compatible with Wi-Fi Direct through NetworkManager.")
        }
    }
}

fn show_restart_toast(win: &libadwaita::PreferencesDialog, message: &str) {
    let toast = libadwaita::Toast::new(message);
    toast.set_button_label(Some(&tr!("Restart now")));
    toast.set_action_name(Some("win.restart"));
    win.add_toast(toast);
}

fn register_window_actions(
    win: &libadwaita::PreferencesDialog,
    app: Option<libadwaita::Application>,
) {
    let group = gio::SimpleActionGroup::new();
    let action = gio::SimpleAction::new("restart", None);
    let app_for_action = app.clone();
    action.connect_activate(move |_, _| {
        if let Some(app) = app_for_action.as_ref() {
            if let Some(action) = app.lookup_action("restart") {
                action.activate(None);
            } else {
                log::warn!("settings restart toast: app.restart action not found");
            }
        } else {
            log::warn!("settings restart toast: application not found");
        }
    });
    group.add_action(&action);
    win.insert_action_group("win", Some(&group));
}
