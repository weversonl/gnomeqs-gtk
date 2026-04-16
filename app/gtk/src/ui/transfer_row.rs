use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use gnomeqs_core::TransferMetadata;
use gnomeqs_core::{DeviceType, State};

use crate::bridge::FromUi;
use crate::tr;
use super::cursor::set_pointer_cursor;

pub struct TransferRow {
    pub row: libadwaita::ActionRow,
    pub icon: gtk4::Image,
    pub progress_bar: gtk4::ProgressBar,
    pub pin_label: gtk4::Label,
    pub button_stack: gtk4::Box,
    pub accept_btn: gtk4::Button,
    pub decline_btn: gtk4::Button,
    pub cancel_btn: gtk4::Button,
    pub open_btn: gtk4::Button,
    pub show_folder_btn: gtk4::Button,
    pub copy_btn: gtk4::Button,
    pub retry_btn: gtk4::Button,
    pub clear_btn: gtk4::Button,
    open_target: Rc<RefCell<Option<String>>>,
    copy_text: Rc<RefCell<Option<String>>>,
    pending_cancel: Rc<Cell<bool>>,
    last_title: Rc<RefCell<String>>,
    last_subtitle: Rc<RefCell<String>>,
}

impl TransferRow {
    pub fn new(
        id: String,
        from_ui_tx: async_channel::Sender<FromUi>,
    ) -> Self {
        let row = libadwaita::ActionRow::new();
        row.set_activatable(false);
        row.add_css_class("transfer-row");
        row.set_title_lines(3);
        row.set_subtitle_lines(3);

        let icon = gtk4::Image::from_icon_name("computer-symbolic");
        icon.set_icon_size(gtk4::IconSize::Large);
        row.add_prefix(&icon);

        let pin_label = gtk4::Label::new(None);
        pin_label.add_css_class("pin-badge");
        pin_label.set_halign(gtk4::Align::End);
        pin_label.set_valign(gtk4::Align::Center);
        pin_label.set_visible(false);

        let progress_bar = gtk4::ProgressBar::new();
        progress_bar.set_show_text(false);
        progress_bar.set_visible(false);
        progress_bar.set_valign(gtk4::Align::Center);
        progress_bar.set_width_request(92);
        progress_bar.set_hexpand(false);

        let accept_btn = gtk4::Button::with_label(&tr!("Accept"));
        accept_btn.add_css_class("suggested-action");
        accept_btn.set_visible(false);
        accept_btn.set_valign(gtk4::Align::Center);
        accept_btn.set_hexpand(false);
        set_pointer_cursor(&accept_btn);

        let decline_btn = gtk4::Button::with_label(&tr!("Decline"));
        decline_btn.add_css_class("destructive-action");
        decline_btn.set_visible(false);
        decline_btn.set_valign(gtk4::Align::Center);
        decline_btn.set_hexpand(false);
        set_pointer_cursor(&decline_btn);

        let cancel_btn = gtk4::Button::with_label(&tr!("Cancel"));
        cancel_btn.set_visible(false);
        cancel_btn.set_valign(gtk4::Align::Center);
        cancel_btn.set_hexpand(false);
        set_pointer_cursor(&cancel_btn);

        let open_btn = gtk4::Button::with_label(&tr!("Open"));
        open_btn.set_visible(false);
        open_btn.set_valign(gtk4::Align::Center);
        open_btn.set_hexpand(false);
        set_pointer_cursor(&open_btn);

        let show_folder_btn = gtk4::Button::with_label(&tr!("Show folder"));
        show_folder_btn.set_visible(false);
        show_folder_btn.set_valign(gtk4::Align::Center);
        show_folder_btn.set_hexpand(false);
        set_pointer_cursor(&show_folder_btn);

        let copy_btn = gtk4::Button::with_label(&tr!("Copy"));
        copy_btn.set_visible(false);
        copy_btn.set_valign(gtk4::Align::Center);
        copy_btn.set_hexpand(false);
        set_pointer_cursor(&copy_btn);

        let retry_btn = gtk4::Button::with_label(&tr!("Retry"));
        retry_btn.add_css_class("suggested-action");
        retry_btn.set_visible(false);
        retry_btn.set_valign(gtk4::Align::Center);
        retry_btn.set_hexpand(false);
        set_pointer_cursor(&retry_btn);

        let clear_btn = gtk4::Button::with_label(&tr!("Clear"));
        clear_btn.set_visible(false);
        clear_btn.set_valign(gtk4::Align::Center);
        clear_btn.set_hexpand(false);
        set_pointer_cursor(&clear_btn);

        let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        info_box.set_halign(gtk4::Align::Center);
        info_box.set_valign(gtk4::Align::Center);
        info_box.append(&progress_bar);
        info_box.append(&pin_label);

        let button_stack = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        button_stack.set_halign(gtk4::Align::Center);
        button_stack.set_valign(gtk4::Align::Center);
        button_stack.append(&accept_btn);
        button_stack.append(&decline_btn);
        button_stack.append(&cancel_btn);
        button_stack.append(&open_btn);
        button_stack.append(&show_folder_btn);
        button_stack.append(&copy_btn);
        button_stack.append(&retry_btn);
        button_stack.append(&clear_btn);

        let action_stack = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        action_stack.set_halign(gtk4::Align::Center);
        action_stack.set_valign(gtk4::Align::Center);
        action_stack.set_hexpand(false);
        action_stack.set_width_request(104);
        action_stack.append(&info_box);
        action_stack.append(&button_stack);
        row.add_suffix(&action_stack);

        {
            let tx = from_ui_tx.clone();
            let id2 = id.clone();
            accept_btn.connect_clicked(move |_| {
                if let Err(e) = tx.try_send(FromUi::Accept(id2.clone())) {
                    log::warn!("Accept send failed: {e}");
                }
            });
        }

        {
            let tx = from_ui_tx.clone();
            let id2 = id.clone();
            decline_btn.connect_clicked(move |_| {
                if let Err(e) = tx.try_send(FromUi::Reject(id2.clone())) {
                    log::warn!("Reject send failed: {e}");
                }
            });
        }

        let pending_cancel = Rc::new(Cell::new(false));

        {
            let tx = from_ui_tx.clone();
            let id2 = id.clone();
            let row_ref = row.clone();
            let cancel_btn_ref = cancel_btn.clone();
            let progress_bar_ref = progress_bar.clone();
            let pending_cancel_ref = pending_cancel.clone();
            cancel_btn.connect_clicked(move |_| {
                log::info!("ui cancel requested for transfer_id={}", id2);
                pending_cancel_ref.set(true);
                cancel_btn_ref.set_sensitive(false);
                row_ref.remove_css_class("transfer-active");
                row_ref.add_css_class("transfer-error");
                row_ref.set_subtitle(&tr!("Cancelling..."));
                progress_bar_ref.set_visible(false);
                if let Err(e) = tx.try_send(FromUi::Cancel(id2.clone())) {
                    log::warn!("Cancel send failed: {e}");
                    pending_cancel_ref.set(false);
                    cancel_btn_ref.set_sensitive(true);
                }
            });
        }

        let open_target: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let copy_text: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let last_title = Rc::new(RefCell::new(String::new()));
        let last_subtitle = Rc::new(RefCell::new(String::new()));

        {
            let open_target = Rc::clone(&open_target);
            open_btn.connect_clicked(move |_| {
                let binding = open_target.borrow();
                let Some(path) = binding.as_ref() else { return };

                let uri = if path.starts_with("file://") {
                    path.clone()
                } else {
                    let p = Path::new(path);
                    if p.exists() {
                        gio::File::for_path(p).uri().to_string()
                    } else {
                        gio::File::for_path(path).uri().to_string()
                    }
                };

                if let Err(e) =
                    gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
                {
                    log::warn!("Open failed: {e}");
                }
            });
        }

        {
            let open_target = Rc::clone(&open_target);
            show_folder_btn.connect_clicked(move |_| {
                let binding = open_target.borrow();
                let Some(path) = binding.as_ref() else { return };

                let target_path = PathBuf::from(path);
                let folder = if target_path.is_dir() {
                    target_path
                } else {
                    target_path
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or(target_path)
                };

                let uri = gio::File::for_path(folder).uri().to_string();
                if let Err(e) =
                    gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
                {
                    log::warn!("Show folder failed: {e}");
                }
            });
        }

        {
            let copy_text = Rc::clone(&copy_text);
            copy_btn.connect_clicked(move |_| {
                let binding = copy_text.borrow();
                let Some(text) = binding.as_ref() else { return };
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(text);
                }
            });
        }

        Self {
            row,
            icon,
            progress_bar,
            pin_label,
            button_stack,
            accept_btn,
            decline_btn,
            cancel_btn,
            open_btn,
            show_folder_btn,
            copy_btn,
            retry_btn,
            clear_btn,
            open_target,
            copy_text,
            pending_cancel,
            last_title,
            last_subtitle,
        }
    }

    pub fn connect_clear<F: Fn() + 'static>(&self, f: F) {
        self.clear_btn.connect_clicked(move |_| {
            f();
        });
    }

    pub fn connect_retry<F: Fn() + 'static>(&self, f: F) {
        self.retry_btn.connect_clicked(move |_| {
            f();
        });
    }

    pub fn history_snapshot(&self) -> (String, String) {
        (
            self.last_title.borrow().clone(),
            self.last_subtitle.borrow().clone(),
        )
    }

    pub fn open_target_snapshot(&self) -> Option<String> {
        self.open_target.borrow().clone()
    }

    pub fn update_state(&self, state: &State, meta: &TransferMetadata) {
        if matches!(
            state,
            State::Finished | State::Rejected | State::Cancelled | State::Disconnected
        ) {
            self.pending_cancel.set(false);
        }

        self.row.remove_css_class("transfer-active");
        self.row.remove_css_class("transfer-success");
        self.row.remove_css_class("transfer-error");

        self.accept_btn.set_visible(false);
        self.decline_btn.set_visible(false);
        self.cancel_btn.set_visible(false);
        self.cancel_btn.set_sensitive(true);
        self.open_btn.set_visible(false);
        self.show_folder_btn.set_visible(false);
        self.copy_btn.set_visible(false);
        self.retry_btn.set_visible(false);
        self.clear_btn.set_visible(false);
        self.progress_bar.set_visible(false);
        self.pin_label.set_visible(false);
        self.button_stack.set_spacing(6);
        *self.open_target.borrow_mut() = None;
        *self.copy_text.borrow_mut() = None;

        if let Some(source) = &meta.source {
            self.row.set_title(&source.name);
            *self.last_title.borrow_mut() = source.name.clone();
            let icon_name = match &source.device_type {
                DeviceType::Phone => "phone-symbolic",
                DeviceType::Tablet => "tablet-symbolic",
                DeviceType::Laptop => "computer-symbolic",
                DeviceType::Unknown => "computer-symbolic",
            };
            self.icon.set_icon_name(Some(icon_name));
        }

        match state {
            State::WaitingForUserConsent => {
                if let Some(pin) = &meta.pin_code {
                    self.pin_label.set_text(&format!("PIN {pin}"));
                    self.pin_label.set_visible(true);
                }
                self.button_stack.set_spacing(4);
                let desc = build_transfer_description(meta);
                self.row.set_subtitle(&desc);
                *self.last_subtitle.borrow_mut() = desc;
                self.accept_btn.set_visible(true);
                self.decline_btn.set_visible(true);
            }
            State::ReceivingFiles => {
                if self.pending_cancel.get() {
                    self.row.add_css_class("transfer-error");
                    let subtitle = tr!("Cancelling transfer...");
                    self.row.set_subtitle(&subtitle);
                    *self.last_subtitle.borrow_mut() = subtitle;
                    self.cancel_btn.set_visible(true);
                    self.cancel_btn.set_sensitive(false);
                } else {
                    self.row.add_css_class("transfer-active");
                    let subtitle = progress_subtitle(&tr!("Receiving"), meta);
                    self.row.set_subtitle(&subtitle);
                    *self.last_subtitle.borrow_mut() = subtitle;
                    self.progress_bar.set_visible(true);
                    self.cancel_btn.set_visible(true);
                    update_progress(&self.progress_bar, meta);
                }
            }
            State::SendingFiles => {
                if self.pending_cancel.get() {
                    self.row.add_css_class("transfer-error");
                    let subtitle = tr!("Cancelling transfer...");
                    self.row.set_subtitle(&subtitle);
                    *self.last_subtitle.borrow_mut() = subtitle;
                    self.cancel_btn.set_visible(true);
                    self.cancel_btn.set_sensitive(false);
                } else {
                    self.row.add_css_class("transfer-active");
                    let subtitle = progress_subtitle(&tr!("Sending"), meta);
                    self.row.set_subtitle(&subtitle);
                    *self.last_subtitle.borrow_mut() = subtitle;
                    self.progress_bar.set_visible(true);
                    self.cancel_btn.set_visible(true);
                    update_progress(&self.progress_bar, meta);
                }
            }
            State::Finished => {
                self.row.add_css_class("transfer-success");
                let open_path = resolve_open_target(meta);
                let desc = if let Some(dest) = &meta.destination {
                    format!("{} {dest}", tr!("Saved to"))
                } else {
                    tr!("Received")
                };
                self.row.set_subtitle(&desc);
                *self.last_subtitle.borrow_mut() = desc;
                if let Some(path) = open_path {
                    *self.open_target.borrow_mut() = Some(path);
                    self.open_btn.set_visible(true);
                    self.show_folder_btn.set_visible(true);
                }
                if meta.text_payload.is_some() {
                    *self.copy_text.borrow_mut() = meta.text_payload.clone();
                    self.copy_btn.set_visible(true);
                }
                self.clear_btn.set_visible(true);
            }
            State::Rejected => {
                self.row.add_css_class("transfer-error");
                let subtitle = tr!("Transfer rejected");
                self.row.set_subtitle(&subtitle);
                *self.last_subtitle.borrow_mut() = subtitle;
                self.clear_btn.set_visible(true);
                self.retry_btn.set_visible(meta.files.is_some());
            }
            State::Cancelled => {
                self.row.add_css_class("transfer-error");
                let subtitle = tr!("Transfer cancelled");
                self.row.set_subtitle(&subtitle);
                *self.last_subtitle.borrow_mut() = subtitle;
                self.clear_btn.set_visible(true);
                self.retry_btn.set_visible(meta.files.is_some());
            }
            State::Disconnected => {
                self.row.add_css_class("transfer-error");
                let subtitle = tr!("Connection lost during transfer");
                self.row.set_subtitle(&subtitle);
                *self.last_subtitle.borrow_mut() = subtitle;
                self.clear_btn.set_visible(true);
                self.retry_btn.set_visible(meta.files.is_some());
            }
            _ => {
                self.row.set_subtitle("");
                self.last_subtitle.borrow_mut().clear();
            }
        }
    }
}

fn update_progress(bar: &gtk4::ProgressBar, meta: &TransferMetadata) {
    if meta.total_bytes > 0 {
        bar.set_fraction(meta.ack_bytes as f64 / meta.total_bytes as f64);
    }
}

fn resolve_open_target(meta: &TransferMetadata) -> Option<String> {
    let dest = meta.destination.as_ref()?;
    let dest_path = PathBuf::from(dest);

    if dest_path.is_dir() {
        if let Some(files) = &meta.files {
            if files.len() == 1 {
                return Some(dest_path.join(&files[0]).to_string_lossy().into_owned());
            }
        }
        return Some(dest_path.to_string_lossy().into_owned());
    }

    Some(dest_path.to_string_lossy().into_owned())
}

fn build_transfer_description(meta: &TransferMetadata) -> String {
    if let Some(files) = &meta.files {
        let count = files.len();
        let label = if count == 1 { tr!("file") } else { tr!("files") };
        format!("{} {count} {label}", tr!("Wants to share"))
    } else if meta.text_payload.is_some() {
        format!("{} {}", tr!("Wants to share"), tr!("text"))
    } else {
        tr!("Wants to share")
    }
}

fn progress_subtitle(prefix: &str, meta: &TransferMetadata) -> String {
    if meta.total_bytes == 0 {
        return format!("{prefix}...");
    }

    format!(
        "{} · {} / {}",
        prefix,
        format_size(meta.ack_bytes),
        format_size(meta.total_bytes)
    )
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;

    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}
