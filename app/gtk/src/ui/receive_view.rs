use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use gtk4::prelude::*;
use libadwaita::prelude::*;

use gnomeqs_core::channel::{ChannelMessage, ChannelDirection};
use gnomeqs_core::Visibility;

use crate::bridge::FromUi;
use crate::settings;
use crate::tr;
use crate::transfer_history::{self, HistoryDirection, HistoryEntry};
use super::cursor::set_pointer_cursor;
use super::pulse::build_pulse_placeholder_sized;
use super::transfer_row::TransferRow;

pub struct ReceiveView {
    pub root: gtk4::Box,
    transfers: Rc<RefCell<HashMap<String, TransferRow>>>,
    transfer_list: gtk4::ListBox,
    recent_list: gtk4::ListBox,
    transfer_header: gtk4::Box,
    transfers_heading: gtk4::Label,
    history_button: gtk4::Button,
    empty_page: gtk4::Box,
    stack: gtk4::Stack,
    list_scroll: gtk4::ScrolledWindow,
    vis_indicator_icon: gtk4::Image,
    vis_indicator_label: gtk4::Label,
    from_ui_tx: async_channel::Sender<FromUi>,
}

impl ReceiveView {
    pub fn new(
        from_ui_tx: async_channel::Sender<FromUi>,
        _toast_overlay: libadwaita::ToastOverlay,
    ) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        // ── Ready-to-receive card ────────────────────────────────
        let ready_card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        ready_card.add_css_class("recv-ready-card");
        ready_card.set_vexpand(true);
        ready_card.set_margin_top(12);
        ready_card.set_margin_start(12);
        ready_card.set_margin_end(12);
        ready_card.set_margin_bottom(6);

        let pulse = build_pulse_placeholder_sized(None, None, false, Some(240));
        ready_card.append(&pulse);

        let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        title_box.set_halign(gtk4::Align::Center);
        title_box.set_margin_bottom(12);

        let title_line1 = gtk4::Label::new(Some(&tr!("Ready to")));
        title_line1.add_css_class("recv-ready-title-plain");
        title_line1.set_halign(gtk4::Align::Center);

        let title_line2 = gtk4::Label::new(Some(&tr!("receive")));
        title_line2.add_css_class("recv-ready-title-accent");
        title_line2.set_halign(gtk4::Align::Center);

        title_box.append(&title_line1);
        title_box.append(&title_line2);
        ready_card.append(&title_box);

        // Visibility indicator — clickable pill that toggles Visible ↔ Invisible
        let vis_indicator = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        vis_indicator.add_css_class("recv-vis-indicator");
        vis_indicator.set_halign(gtk4::Align::Center);

        let vis_indicator_icon = gtk4::Image::from_icon_name("eye-open-negative-filled-symbolic");
        vis_indicator_icon.set_pixel_size(16);
        let vis_indicator_label = gtk4::Label::new(None);
        vis_indicator_label.add_css_class("recv-vis-label");
        vis_indicator.append(&vis_indicator_icon);
        vis_indicator.append(&vis_indicator_label);

        let vis_btn = gtk4::Button::new();
        vis_btn.set_child(Some(&vis_indicator));
        vis_btn.add_css_class("flat");
        vis_btn.add_css_class("recv-vis-btn");
        vis_btn.set_halign(gtk4::Align::Center);
        vis_btn.set_margin_bottom(20);
        set_pointer_cursor(&vis_btn);

        {
            let icon = vis_indicator_icon.clone();
            let label = vis_indicator_label.clone();
            let tx = from_ui_tx.clone();
            vis_btn.connect_clicked(move |_| {
                let current = Visibility::from_raw_value(settings::get_visibility_raw() as u64);
                let next = match current {
                    Visibility::Visible    => Visibility::Invisible,
                    Visibility::Invisible  => Visibility::Visible,
                    Visibility::Temporarily => Visibility::Visible,
                };
                settings::set_visibility_raw(next as i32);
                apply_vis_indicator(&icon, &label, next);
                let _ = tx.send_blocking(FromUi::ChangeVisibility(next));
            });
        }
        ready_card.append(&vis_btn);

        // Wrapper so the card sits at top inside the vexpand stack
        let empty_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        empty_page.set_vexpand(true);
        empty_page.append(&ready_card);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);

        let transfer_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        transfer_header.set_margin_top(2);
        transfer_header.set_margin_bottom(8);
        transfer_header.set_margin_start(16);
        transfer_header.set_margin_end(16);
        transfer_header.set_visible(false);

        let transfers_heading = gtk4::Label::new(Some(&tr!("Active transfers")));
        transfers_heading.add_css_class("caption-heading");
        transfers_heading.set_halign(gtk4::Align::Start);
        transfers_heading.set_hexpand(true);

        let history_button = gtk4::Button::with_label(&tr!("History"));
        history_button.add_css_class("history-button");
        history_button.set_visible(false);
        set_pointer_cursor(&history_button);

        transfer_header.append(&transfers_heading);
        transfer_header.append(&history_button);

        let transfer_list = gtk4::ListBox::new();
        transfer_list.add_css_class("boxed-list");
        transfer_list.add_css_class("glass-card");
        transfer_list.set_selection_mode(gtk4::SelectionMode::None);
        transfer_list.set_valign(gtk4::Align::Start);
        transfer_list.set_vexpand(false);
        transfer_list.set_margin_top(6);
        transfer_list.set_margin_bottom(12);
        transfer_list.set_margin_start(12);
        transfer_list.set_margin_end(12);
        transfer_list.set_visible(false);

        scroll.set_child(Some(&transfer_list));

        let recent_list = gtk4::ListBox::new();
        recent_list.add_css_class("boxed-list");
        recent_list.add_css_class("history-list");
        recent_list.set_selection_mode(gtk4::SelectionMode::None);
        recent_list.set_margin_top(0);
        recent_list.set_margin_bottom(0);
        recent_list.set_margin_start(0);
        recent_list.set_margin_end(0);

        let history_dialog = build_receive_history_dialog(&recent_list);
        {
            let history_dialog = history_dialog.clone();
            history_button.connect_clicked(move |btn| {
                let Some(window) = btn.root().and_downcast::<gtk4::Window>() else {
                    return;
                };
                history_dialog.present(Some(&window));
            });
        }
        load_receive_history(&recent_list, &history_button, &transfer_header);

        let stack = gtk4::Stack::new();
        stack.set_vexpand(true);
        stack.add_child(&empty_page);
        stack.add_child(&scroll);
        stack.set_visible_child(&empty_page);

        root.append(&transfer_header);
        root.append(&stack);

        // Apply initial visibility state
        let init_vis = Visibility::from_raw_value(settings::get_visibility_raw() as u64);
        apply_vis_indicator(&vis_indicator_icon, &vis_indicator_label, init_vis);

        Self {
            root,
            transfers: Rc::new(RefCell::new(HashMap::new())),
            transfer_list,
            recent_list,
            transfer_header,
            transfers_heading,
            history_button,
            empty_page,
            stack,
            list_scroll: scroll,
            vis_indicator_icon,
            vis_indicator_label,
            from_ui_tx,
        }
    }

    pub fn handle_channel_message(&self, msg: ChannelMessage) {
        if msg.direction != ChannelDirection::LibToFront {
            return;
        }

        let id = msg.id.clone();
        let state = match &msg.state {
            Some(s) => s.clone(),
            None => return,
        };
        let meta = match &msg.meta {
            Some(m) => m.clone(),
            None => return,
        };

        let mut map = self.transfers.borrow_mut();

        if !map.contains_key(&id) {
            let row = TransferRow::new(id.clone(), self.from_ui_tx.clone());
            {
                let id = id.clone();
                let transfers = Rc::clone(&self.transfers);
                let list = self.transfer_list.clone();
                let recent_list = self.recent_list.clone();
                let stack = self.stack.clone();
                let scroll = self.list_scroll.clone();
                let empty_page = self.empty_page.clone();
                let transfers_heading = self.transfers_heading.clone();
                let transfer_header = self.transfer_header.clone();
                let history_button = self.history_button.clone();
                row.connect_clear(move || {
                    let mut map = transfers.borrow_mut();
                    if let Some(row) = map.remove(&id) {
                        let (title, subtitle) = row.history_snapshot();
                        let open_target = row.open_target_snapshot();
                        list.remove(&row.row);
                        prepend_receive_history_row(&recent_list, &title, &subtitle, open_target.clone());
                        transfer_history::append(HistoryEntry {
                            created_at: 0,
                            direction: HistoryDirection::Receive,
                            title,
                            subtitle,
                            open_target,
                        });
                        history_button.set_visible(true);
                    }
                    if map.is_empty() {
                        list.set_visible(false);
                        transfers_heading.set_visible(false);
                        transfer_header.set_visible(history_button.is_visible());
                        stack.set_visible_child(&empty_page);
                    } else {
                        transfers_heading.set_visible(true);
                        transfer_header.set_visible(true);
                        stack.set_visible_child(&scroll);
                    }
                });
            }
            self.transfer_list.append(&row.row);
            self.transfer_list.set_visible(true);
            self.transfers_heading.set_visible(true);
            self.transfer_header.set_visible(true);
            self.stack.set_visible_child(&self.list_scroll);
            row.update_state(&state, &meta);
            map.insert(id, row);
        } else if let Some(row) = map.get(&id) {
            row.update_state(&state, &meta);
        }
    }

    pub fn update_visibility(&self, vis: Visibility) {
        apply_vis_indicator(&self.vis_indicator_icon, &self.vis_indicator_label, vis);
    }
}

fn apply_vis_indicator(icon: &gtk4::Image, label: &gtk4::Label, vis: Visibility) {
    icon.remove_css_class("visibility-visible");
    icon.remove_css_class("visibility-hidden");
    icon.remove_css_class("visibility-temporary");

    match vis {
        Visibility::Visible => {
            icon.set_icon_name(Some("eye-open-negative-filled-symbolic"));
            icon.add_css_class("visibility-visible");
            label.set_text(&tr!("Visible"));
        }
        Visibility::Invisible => {
            icon.set_icon_name(Some("eye-not-looking-symbolic"));
            icon.add_css_class("visibility-hidden");
            label.set_text(&tr!("Hidden"));
        }
        Visibility::Temporarily => {
            icon.set_icon_name(Some("eye-open-negative-filled-symbolic"));
            icon.add_css_class("visibility-temporary");
            label.set_text(&tr!("Temporarily visible"));
        }
    }
}

fn build_receive_history_dialog(list: &gtk4::ListBox) -> libadwaita::PreferencesDialog {
    let dialog = libadwaita::PreferencesDialog::new();
    dialog.set_title(&tr!("Receive history"));
    dialog.set_search_enabled(false);

    let page = libadwaita::PreferencesPage::new();
    let group = libadwaita::PreferencesGroup::new();
    group.set_description(Some(&tr!("Transfer history is stored locally for up to {} days by default, unless changed in Settings.")
        .replace("{}", &settings::get_history_retention_days().to_string())));

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_min_content_width(300);
    scroll.set_min_content_height(220);
    scroll.set_max_content_height(420);
    scroll.set_child(Some(list));

    group.add(&scroll);
    page.add(&group);
    dialog.add(&page);
    dialog
}

fn load_receive_history(
    list: &gtk4::ListBox,
    history_button: &gtk4::Button,
    transfer_header: &gtk4::Box,
) {
    let entries = transfer_history::load(HistoryDirection::Receive);
    for entry in entries.into_iter().rev() {
        prepend_receive_history_row(list, &entry.title, &entry.subtitle, entry.open_target);
    }
    let has_history = list.first_child().is_some();
    history_button.set_visible(has_history);
    transfer_header.set_visible(has_history);
}

fn prepend_receive_history_row(
    list: &gtk4::ListBox,
    title: &str,
    subtitle: &str,
    open_target: Option<String>,
) {
    let row = gtk4::ListBoxRow::new();
    row.add_css_class("history-row");
    let row_title = if title.is_empty() {
        tr!("Recent transfer")
    } else {
        title.to_string()
    };

    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    body.set_width_request(300);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    body.set_margin_start(10);
    body.set_margin_end(10);

    let icon = gtk4::Image::from_icon_name("folder-download-symbolic");
    icon.set_pixel_size(22);
    body.append(&icon);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text_box.set_hexpand(true);

    let title_label = gtk4::Label::new(Some(&row_title));
    title_label.add_css_class("history-title");
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let subtitle_label = gtk4::Label::new(Some(subtitle));
    subtitle_label.add_css_class("history-subtitle");
    subtitle_label.set_halign(gtk4::Align::Start);
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    text_box.append(&title_label);
    text_box.append(&subtitle_label);
    body.append(&text_box);

    if let Some(path) = open_target {
        let show_btn = gtk4::Button::from_icon_name("folder-open-symbolic");
        show_btn.set_tooltip_text(Some(&tr!("Show folder")));
        show_btn.add_css_class("history-icon-button");
        set_pointer_cursor(&show_btn);
        show_btn.connect_clicked(move |_| {
            let folder = std::path::Path::new(&path)
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::Path::new(&path).to_path_buf());
            let uri = gio::File::for_path(folder).uri().to_string();
            if let Err(e) =
                gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
            {
                log::warn!("Receive history show folder failed: {e}");
            }
        });
        body.append(&show_btn);
    }

    row.set_child(Some(&body));
    list.insert(&row, 0);
    list.set_visible(true);

    while let Some(last) = list.row_at_index(6) {
        list.remove(&last);
    }
}
