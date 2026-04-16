use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;
use libadwaita::prelude::*;

use gnomeqs_core::channel::ChannelMessage;
use gnomeqs_core::{
    DeviceType, EndpointInfo, EndpointTransport, OutboundPayload, SendInfo, State,
};

use crate::bridge::{FromUi, WifiDirectSendRequest, WifiDirectSessionReady};
use crate::settings;
use crate::tr;
use crate::transfer_history::{self, HistoryDirection, HistoryEntry};
use super::cursor::set_pointer_cursor;
use super::device_tile::DeviceTile;
use super::pulse::build_pulse_placeholder;
use super::transfer_row::TransferRow;

pub struct SendView {
    pub root: gtk4::Box,
    devices_box: gtk4::FlowBox,
    selected_files: Rc<RefCell<Vec<String>>>,
    from_ui_tx: async_channel::Sender<FromUi>,
    devices: Rc<RefCell<HashMap<String, DeviceTile>>>,
    transfers: Rc<RefCell<HashMap<String, TransferRow>>>,
    sent_requests: Rc<RefCell<HashMap<String, RetryRequest>>>,
    transfer_list: gtk4::ListBox,
    recent_list: gtk4::ListBox,
    transfer_header: gtk4::Box,
    transfers_heading: gtk4::Label,
    history_button: gtk4::Button,
    devices_stack: gtk4::Stack,
    devices_placeholder: gtk4::Box,
    devices_scroll: gtk4::ScrolledWindow,
    endpoint_tx: Rc<RefCell<Option<tokio::sync::broadcast::Sender<EndpointInfo>>>>,
    discovery_active: Rc<RefCell<bool>>,
    pending_start: Rc<RefCell<Option<glib::SourceId>>>,
    pending_wifi_direct_send: Rc<RefCell<Option<PendingWifiDirectSend>>>,
    known_mdns_endpoints: Rc<RefCell<HashMap<String, KnownMdnsEndpoint>>>,
}

#[derive(Debug, Clone)]
struct PendingWifiDirectSend {
    peer_id: String,
    peer_name: String,
    files: Vec<String>,
}

#[derive(Debug, Clone)]
struct KnownMdnsEndpoint {
    name: String,
    port: String,
    device_type: DeviceType,
}

#[derive(Debug, Clone)]
struct RetryRequest {
    name: String,
    device_type: DeviceType,
    addr: String,
    files: Vec<String>,
}

impl SendView {
    pub fn new(from_ui_tx: async_channel::Sender<FromUi>) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_vexpand(true);

        let content_scroll = gtk4::ScrolledWindow::new();
        content_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        content_scroll.set_vexpand(true);
        content_scroll.set_hexpand(true);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_margin_bottom(128);
        content_scroll.set_child(Some(&content));
        root.append(&content_scroll);

        let selected_files: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let devices: Rc<RefCell<HashMap<String, DeviceTile>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let transfers: Rc<RefCell<HashMap<String, TransferRow>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let sent_requests: Rc<RefCell<HashMap<String, RetryRequest>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let endpoint_tx: Rc<RefCell<Option<tokio::sync::broadcast::Sender<EndpointInfo>>>> =
            Rc::new(RefCell::new(None));
        let discovery_active = Rc::new(RefCell::new(false));
        let pending_start: Rc<RefCell<Option<glib::SourceId>>> =
            Rc::new(RefCell::new(None));
        let pending_wifi_direct_send: Rc<RefCell<Option<PendingWifiDirectSend>>> =
            Rc::new(RefCell::new(None));
        let known_mdns_endpoints: Rc<RefCell<HashMap<String, KnownMdnsEndpoint>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let files_group = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        files_group.add_css_class("glass-card");
        files_group.add_css_class("send-drop-card");
        files_group.set_margin_top(12);
        files_group.set_margin_bottom(8);
        files_group.set_margin_start(12);
        files_group.set_margin_end(12);
        files_group.set_valign(gtk4::Align::Start);

        let upload_icon = gtk4::Image::from_icon_name("io.github.weversonl.GnomeQuickShare-airdrop-symbolic");
        upload_icon.add_css_class("send-drop-icon");
        upload_icon.set_halign(gtk4::Align::Center);

        let files_title = gtk4::Label::new(Some(&tr!("Drop files to send")));
        files_title.add_css_class("send-drop-title");
        files_title.set_halign(gtk4::Align::Center);

        let files_subtitle = gtk4::Label::new(Some(&tr!("Select")));
        files_subtitle.add_css_class("send-drop-subtitle");
        files_subtitle.set_halign(gtk4::Align::Center);

        let files_meta = gtk4::Label::new(Some(&tr!("Drop files here or use Select")));
        files_meta.add_css_class("send-drop-meta");
        files_meta.set_halign(gtk4::Align::Center);

        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        actions.set_halign(gtk4::Align::Center);

        let select_btn = gtk4::Button::with_label(&tr!("Select"));
        select_btn.add_css_class("send-select-button");
        select_btn.set_valign(gtk4::Align::Center);
        set_pointer_cursor(&select_btn);
        actions.append(&select_btn);

        let clear_files_btn = gtk4::Button::from_icon_name("user-trash-symbolic");
        clear_files_btn.add_css_class("flat");
        clear_files_btn.add_css_class("clear-files-button");
        clear_files_btn.set_valign(gtk4::Align::Center);
        clear_files_btn.set_visible(false);
        clear_files_btn.set_tooltip_text(Some(&tr!("Clear")));
        set_pointer_cursor(&clear_files_btn);
        actions.append(&clear_files_btn);

        let selected_files_flow = gtk4::FlowBox::new();
        selected_files_flow.set_selection_mode(gtk4::SelectionMode::None);
        selected_files_flow.set_halign(gtk4::Align::Start);
        selected_files_flow.set_valign(gtk4::Align::Start);
        selected_files_flow.set_max_children_per_line(8);
        selected_files_flow.set_min_children_per_line(1);
        selected_files_flow.set_column_spacing(8);
        selected_files_flow.set_row_spacing(8);
        selected_files_flow.set_visible(false);

        files_group.append(&upload_icon);
        files_group.append(&files_title);
        files_group.append(&files_subtitle);
        files_group.append(&files_meta);
        files_group.append(&actions);
        files_group.append(&selected_files_flow);
        content.append(&files_group);

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
        content.append(&transfer_header);

        let transfer_list = gtk4::ListBox::new();
        transfer_list.add_css_class("boxed-list");
        transfer_list.add_css_class("glass-card");
        transfer_list.set_selection_mode(gtk4::SelectionMode::None);
        transfer_list.set_visible(false);
        transfer_list.set_margin_top(6);
        transfer_list.set_margin_bottom(6);
        transfer_list.set_margin_start(12);
        transfer_list.set_margin_end(12);
        content.append(&transfer_list);

        let recent_list = gtk4::ListBox::new();
        recent_list.add_css_class("boxed-list");
        recent_list.add_css_class("history-list");
        recent_list.set_selection_mode(gtk4::SelectionMode::None);
        recent_list.set_margin_top(0);
        recent_list.set_margin_bottom(0);
        recent_list.set_margin_start(0);
        recent_list.set_margin_end(0);

        let history_dialog = build_history_dialog(&tr!("Send history"), &recent_list);
        {
            let history_dialog = history_dialog.clone();
            history_button.connect_clicked(move |btn| {
                let Some(window) = btn.root().and_downcast::<gtk4::Window>() else {
                    return;
                };
                history_dialog.present(Some(&window));
            });
        }
        load_send_history(&recent_list, &history_button, &transfer_header);

        {
            let selected_files = Rc::clone(&selected_files);
            let files_subtitle_clone = files_subtitle.clone();
            let files_meta_clone = files_meta.clone();
            let clear_btn_clone = clear_files_btn.clone();
            let selected_files_flow_clone = selected_files_flow.clone();
            let upload_icon_clone = upload_icon.clone();
            select_btn.connect_clicked(move |btn| {
                let files_ref = Rc::clone(&selected_files);
                let subtitle_ref = files_subtitle_clone.clone();
                let meta_ref = files_meta_clone.clone();
                let clear_ref = clear_btn_clone.clone();
                let flow_ref = selected_files_flow_clone.clone();
                let upload_icon_ref = upload_icon_clone.clone();

                let window = btn.root().and_downcast::<gtk4::Window>();
                let dialog = gtk4::FileDialog::new();
                dialog.set_title(&tr!("Select files to send"));
                dialog.set_modal(true);

                dialog.open_multiple(
                    window.as_ref(),
                    gio::Cancellable::NONE,
                    move |result| {
                        if let Ok(files) = result {
                            let mut paths = Vec::new();
                            for i in 0..files.n_items() {
                                if let Some(obj) = files.item(i) {
                                    if let Ok(file) = obj.downcast::<gio::File>() {
                                        if let Some(p) = file.path() {
                                            paths.push(p.to_string_lossy().into_owned());
                                        }
                                    }
                                }
                            }
                            if !paths.is_empty() {
                                *files_ref.borrow_mut() = paths;
                                rebuild_selected_files_ui(
                                    &files_ref,
                                    &flow_ref,
                                    &subtitle_ref,
                                    &meta_ref,
                                    &clear_ref,
                                    &upload_icon_ref,
                                );
                            }
                        }
                    },
                );
            });
        }

        {
            let selected_files = Rc::clone(&selected_files);
            let files_subtitle_clone = files_subtitle.clone();
            let files_meta_clone = files_meta.clone();
            let selected_files_flow_clone = selected_files_flow.clone();
            let upload_icon_clone = upload_icon.clone();
            clear_files_btn.connect_clicked(move |btn| {
                *selected_files.borrow_mut() = Vec::new();
                rebuild_selected_files_ui(
                    &selected_files,
                    &selected_files_flow_clone,
                    &files_subtitle_clone,
                    &files_meta_clone,
                    btn,
                    &upload_icon_clone,
                );
            });
        }

        let drop_target = gtk4::DropTarget::new(
            gio::File::static_type(),
            gtk4::gdk::DragAction::COPY,
        );
        {
            let selected_files = Rc::clone(&selected_files);
            let files_subtitle_clone = files_subtitle.clone();
            let files_meta_clone = files_meta.clone();
            let clear_btn_clone = clear_files_btn.clone();
            let selected_files_flow_clone = selected_files_flow.clone();
            let upload_icon_clone = upload_icon.clone();
            let files_group_for_drop = files_group.clone();
            drop_target.connect_drop(move |_, value, _, _| {
                files_group_for_drop.remove_css_class("send-drop-active");
                if let Ok(file) = value.get::<gio::File>() {
                    if let Some(path) = file.path() {
                        let path_str = path.to_string_lossy().into_owned();
                        selected_files.borrow_mut().push(path_str.clone());
                        rebuild_selected_files_ui(
                            &selected_files,
                            &selected_files_flow_clone,
                            &files_subtitle_clone,
                            &files_meta_clone,
                            &clear_btn_clone,
                            &upload_icon_clone,
                        );
                        return true;
                    }
                }
                false
            });
        }
        root.add_controller(drop_target);

        let drop_motion = gtk4::DropControllerMotion::new();
        {
            let files_group = files_group.clone();
            drop_motion.connect_enter(move |_, _, _| {
                files_group.add_css_class("send-drop-active");
            });
        }
        {
            let files_group = files_group.clone();
            drop_motion.connect_leave(move |_| {
                files_group.remove_css_class("send-drop-active");
            });
        }
        files_group.add_controller(drop_motion);

        let devices_card = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        devices_card.add_css_class("glass-card");
        devices_card.add_css_class("devices-card");
        devices_card.set_margin_top(8);
        devices_card.set_margin_bottom(8);
        devices_card.set_margin_start(12);
        devices_card.set_margin_end(12);

        let devices_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

        let devices_label = gtk4::Label::new(Some(&tr!("Nearby devices")));
        devices_label.add_css_class("caption-heading");
        devices_label.set_hexpand(true);
        devices_label.set_halign(gtk4::Align::Start);

        let refresh_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
        refresh_btn.add_css_class("flat");
        refresh_btn.set_tooltip_text(Some(&tr!("Refresh")));
        set_pointer_cursor(&refresh_btn);

        devices_header.append(&devices_label);
        devices_header.append(&refresh_btn);
        devices_card.append(&devices_header);

        let network_summary = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        network_summary.add_css_class("network-summary-card");
        let network_summary_title = gtk4::Label::new(Some(&tr!("Network status")));
        network_summary_title.add_css_class("network-summary-title");
        network_summary_title.set_halign(gtk4::Align::Start);
        let network_summary_subtitle = gtk4::Label::new(Some(&build_network_summary_text()));
        network_summary_subtitle.add_css_class("network-summary-subtitle");
        network_summary_subtitle.set_wrap(true);
        network_summary_subtitle.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        network_summary_subtitle.set_halign(gtk4::Align::Start);
        network_summary_subtitle.set_xalign(0.0);
        network_summary.append(&network_summary_title);
        network_summary.append(&network_summary_subtitle);
        devices_card.append(&network_summary);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);

        let devices_box = gtk4::FlowBox::new();
        devices_box.set_selection_mode(gtk4::SelectionMode::None);
        devices_box.set_valign(gtk4::Align::Start);
        devices_box.set_halign(gtk4::Align::Start);
        devices_box.set_margin_top(6);
        devices_box.set_margin_bottom(12);
        devices_box.set_margin_start(6);
        devices_box.set_margin_end(6);
        devices_box.set_column_spacing(12);
        devices_box.set_row_spacing(12);
        scroll.set_child(Some(&devices_box));

        let devices_placeholder = build_pulse_placeholder(None, Some(&tr!("Nearby devices")), false);
        devices_placeholder.set_margin_top(12);
        devices_placeholder.set_margin_bottom(12);

        let devices_stack = gtk4::Stack::new();
        devices_stack.set_vexpand(true);
        devices_stack.set_size_request(-1, 240);
        devices_stack.add_child(&devices_placeholder);
        devices_stack.add_child(&scroll);
        devices_stack.set_visible_child(&devices_placeholder);
        devices_card.append(&devices_stack);
        content.append(&devices_card);

        {
            let devices = Rc::clone(&devices);
            let devices_stack = devices_stack.clone();
            let devices_placeholder = devices_placeholder.clone();
            let tx = from_ui_tx.clone();
            let active = Rc::clone(&discovery_active);
            refresh_btn.connect_clicked(move |_| {
                log::info!("send view refresh requested");
                if devices.borrow().is_empty() {
                    devices_stack.set_visible_child(&devices_placeholder);
                }

                if !*active.borrow() {
                    let (sender, _) = tokio::sync::broadcast::channel(20);
                    if let Err(e) = tx.try_send(FromUi::StartDiscovery(sender)) {
                        log::warn!("StartDiscovery: {e}");
                    } else {
                        *active.borrow_mut() = true;
                    }
                }
            });
        }

        Self {
            root,
            devices_box,
            selected_files,
            from_ui_tx,
            devices,
            transfers,
            sent_requests,
            transfer_list,
            recent_list,
            transfer_header,
            transfers_heading,
            history_button,
            devices_stack,
            devices_placeholder,
            devices_scroll: scroll,
            endpoint_tx,
            discovery_active,
            pending_start,
            pending_wifi_direct_send,
            known_mdns_endpoints,
        }
    }

    pub fn update_endpoint(&self, info: EndpointInfo) {
        self.try_auto_send_pending_wifi_direct(&info);

        let present = info.present.unwrap_or(true);
        let mut devices = self.devices.borrow_mut();

        if !present {
            if let Some(tile) = devices.remove(&info.id) {
                self.devices_box.remove(&tile.button);
            }
            if devices.is_empty() {
                self.devices_stack.set_visible_child(&self.devices_placeholder);
            }
            return;
        }

        let is_wifi_direct_peer = matches!(info.transport, Some(EndpointTransport::WifiDirectPeer));
        if !is_wifi_direct_peer && (info.ip.is_none() || info.port.is_none()) {
            log::debug!(
                "ignoring incomplete endpoint update: id={} name={:?} transport={:?}",
                info.id,
                info.name,
                info.transport
            );
            return;
        }

        if matches!(info.transport, Some(EndpointTransport::MdnsTcp)) {
            if let (Some(name), Some(port)) = (info.name.clone(), info.port.clone()) {
                self.known_mdns_endpoints.borrow_mut().insert(
                    normalize_device_name(&name),
                    KnownMdnsEndpoint {
                        name,
                        port,
                        device_type: info.rtype.clone().unwrap_or(DeviceType::Unknown),
                    },
                );
            }
        }

        if devices.contains_key(&info.id) {
            return;
        }

        let files = Rc::clone(&self.selected_files);
        let tx = self.from_ui_tx.clone();
        let pending_wifi_direct = Rc::clone(&self.pending_wifi_direct_send);
        let sent_requests = Rc::clone(&self.sent_requests);
        let tile = DeviceTile::new(
            info.clone(),
            move || files.borrow().clone(),
            move |endpoint, files| match endpoint.transport {
                Some(EndpointTransport::WifiDirectPeer) => {
                    let peer_mac = match endpoint.wifi_direct_peer_mac.clone() {
                        Some(peer_mac) => peer_mac,
                        None => {
                            log::warn!("Wi-Fi Direct peer is missing its MAC address");
                            return;
                        }
                    };
                    let peer_name = endpoint.name.clone().unwrap_or_else(|| endpoint.id.clone());
                    log::info!(
                        "queueing Wi-Fi Direct send: peer_id={} peer_name={} peer_mac={} files={}",
                        endpoint.id,
                        peer_name,
                        peer_mac,
                        files.len()
                    );
                    *pending_wifi_direct.borrow_mut() = Some(PendingWifiDirectSend {
                        peer_id: endpoint.id.clone(),
                        peer_name: peer_name.clone(),
                        files: files.clone(),
                    });
                    if let Err(e) = tx.try_send(FromUi::StartWifiDirectSend(WifiDirectSendRequest {
                        peer_id: endpoint.id.clone(),
                        peer_name,
                        peer_mac,
                        files,
                    })) {
                        log::warn!("StartWifiDirectSend failed: {e}");
                    }
                }
                _ => {
                    let retry_files = files.clone();
                    let transfer_id = format!(
                        "{}-{}",
                        endpoint.id,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_micros())
                            .unwrap_or_default()
                    );
                    let send_info = SendInfo {
                        id: transfer_id,
                        name: endpoint.name.clone().unwrap_or_default(),
                        device_type: endpoint.rtype.clone().unwrap_or(DeviceType::Unknown),
                        addr: format!(
                            "{}:{}",
                            endpoint.ip.as_deref().unwrap_or(""),
                            endpoint.port.as_deref().unwrap_or("0")
                        ),
                        ob: OutboundPayload::Files(files),
                    };
                    sent_requests
                        .borrow_mut()
                        .insert(
                            send_info.id.clone(),
                            RetryRequest {
                                name: send_info.name.clone(),
                                device_type: send_info.device_type.clone(),
                                addr: send_info.addr.clone(),
                                files: retry_files,
                            },
                        );
                    if let Err(e) = tx.try_send(FromUi::SendPayload(send_info)) {
                        log::warn!("SendPayload failed: {e}");
                    }
                }
            },
        );
        self.devices_box.append(&tile.button);
        devices.insert(info.id.clone(), tile);
        self.devices_stack.set_visible_child(&self.devices_scroll);
    }

    pub fn start_discovery(&self) {
        if *self.discovery_active.borrow() {
            log::debug!("send view discovery start ignored: already active");
            return;
        }
        if let Some(id) = self.pending_start.borrow_mut().take() {
            let _ = std::panic::catch_unwind(|| id.remove());
        }
        let (sender, _) = tokio::sync::broadcast::channel(20);
        *self.endpoint_tx.borrow_mut() = Some(sender.clone());
        if let Err(e) = self.from_ui_tx.try_send(FromUi::StartDiscovery(sender)) {
            log::warn!("StartDiscovery: {e}");
        } else {
            *self.discovery_active.borrow_mut() = true;
            log::info!("send view discovery started");
        }
    }

    pub fn stop_discovery(&self) {
        if !*self.discovery_active.borrow() && self.pending_start.borrow().is_none() {
            log::debug!("send view discovery stop ignored: already inactive");
            return;
        }
        if let Some(id) = self.pending_start.borrow_mut().take() {
            let _ = std::panic::catch_unwind(|| id.remove());
        }
        if let Err(e) = self.from_ui_tx.try_send(FromUi::StopDiscovery) {
            log::warn!("StopDiscovery: {e}");
        } else {
            *self.discovery_active.borrow_mut() = false;
            log::info!("send view discovery stopped");
        }
    }

    pub fn handle_channel_message(&self, msg: ChannelMessage) {
        let state = match &msg.state {
            Some(state) => state.clone(),
            None => return,
        };
        let meta = match &msg.meta {
            Some(meta) => meta.clone(),
            None => return,
        };
        let id = msg.id.clone();

        let mut map = self.transfers.borrow_mut();

        if !map.contains_key(&id) {
            let row = TransferRow::new(id.clone(), self.from_ui_tx.clone());
            {
                let id = id.clone();
                let transfers = Rc::clone(&self.transfers);
                let list = self.transfer_list.clone();
                let recent_list = self.recent_list.clone();
                let sent_requests = Rc::clone(&self.sent_requests);
                let transfers_heading = self.transfers_heading.clone();
                let transfer_header = self.transfer_header.clone();
                let history_button = self.history_button.clone();
                let tx_history = self.from_ui_tx.clone();
                row.connect_clear(move || {
                    let mut map = transfers.borrow_mut();
                    if let Some(row) = map.remove(&id) {
                        let (title, subtitle) = row.history_snapshot();
                        let open_target = row.open_target_snapshot();
                        let retry_request = if history_allows_retry(&subtitle) {
                            sent_requests.borrow().get(&id).cloned()
                        } else {
                            None
                        };
                        list.remove(&row.row);
                        prepend_history_row(
                            &recent_list,
                            &title,
                            &subtitle,
                            retry_request,
                            open_target,
                            tx_history.clone(),
                        );
                        transfer_history::append(HistoryEntry {
                            created_at: 0,
                            direction: HistoryDirection::Send,
                            title,
                            subtitle,
                            open_target: None,
                        });
                        history_button.set_visible(true);
                    }
                    list.set_visible(!map.is_empty());
                    transfers_heading.set_visible(!map.is_empty());
                    transfer_header.set_visible(!map.is_empty() || history_button.is_visible());
                });
            }
            if let Some(send_info) = self.sent_requests.borrow().get(&id).cloned() {
                let tx = self.from_ui_tx.clone();
                row.connect_retry(move || {
                    let retry_id = format!(
                        "{}-retry-{}",
                        send_info.addr,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_micros())
                            .unwrap_or_default()
                    );
                    let retry_info = SendInfo {
                        id: retry_id,
                        name: send_info.name.clone(),
                        device_type: send_info.device_type.clone(),
                        addr: send_info.addr.clone(),
                        ob: OutboundPayload::Files(send_info.files.clone()),
                    };
                    if let Err(e) = tx.try_send(FromUi::SendPayload(retry_info)) {
                        log::warn!("Retry SendPayload failed: {e}");
                    }
                });
            }
            self.transfer_list.append(&row.row);
            self.transfer_list.set_visible(true);
            self.transfers_heading.set_visible(true);
            self.transfer_header.set_visible(true);
            row.update_state(&state, &meta);
            map.insert(id, row);
        } else if let Some(row) = map.get(&id) {
            row.update_state(&state, &meta);
            match state {
                State::Disconnected | State::Finished | State::Rejected | State::Cancelled => {}
                _ => {}
            }
        }
    }

    fn try_auto_send_pending_wifi_direct(&self, info: &EndpointInfo) {
        let Some(EndpointTransport::MdnsTcp) = info.transport.clone() else {
            return;
        };
        if info.ip.is_none() || info.port.is_none() {
            return;
        }

        let pending = self.pending_wifi_direct_send.borrow().clone();
        let Some(pending) = pending else {
            return;
        };

        let endpoint_name = info.name.clone().unwrap_or_default();
        if normalize_device_name(&endpoint_name) != normalize_device_name(&pending.peer_name) {
            return;
        }

        let transfer_id = format!(
            "{}-{}",
            info.id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_micros())
                .unwrap_or_default()
        );
        let retry_files = pending.files.clone();
        let send_info = SendInfo {
            id: transfer_id,
            name: endpoint_name,
            device_type: info.rtype.clone().unwrap_or(DeviceType::Unknown),
            addr: format!(
                "{}:{}",
                info.ip.as_deref().unwrap_or(""),
                info.port.as_deref().unwrap_or("0")
            ),
            ob: OutboundPayload::Files(pending.files),
        };
        self.sent_requests
            .borrow_mut()
            .insert(
                send_info.id.clone(),
                RetryRequest {
                    name: send_info.name.clone(),
                    device_type: send_info.device_type.clone(),
                    addr: send_info.addr.clone(),
                    files: retry_files,
                },
            );

        if let Err(e) = self.from_ui_tx.try_send(FromUi::SendPayload(send_info)) {
            log::warn!("auto SendPayload failed after Wi-Fi Direct activation: {e}");
            return;
        }

        log::info!(
            "auto-sent pending Wi-Fi Direct payload for peer_id={} via endpoint={}",
            pending.peer_id,
            info.id
        );
        *self.pending_wifi_direct_send.borrow_mut() = None;
    }

    pub fn handle_wifi_direct_session_ready(&self, ready: WifiDirectSessionReady) {
        let pending = self.pending_wifi_direct_send.borrow().clone();
        let Some(pending) = pending else {
            log::debug!(
                "ignoring Wi-Fi Direct session ready for peer_id={}: no pending send",
                ready.peer_id
            );
            return;
        };

        if pending.peer_id != ready.peer_id
            && normalize_device_name(&pending.peer_name) != normalize_device_name(&ready.peer_name)
        {
            log::debug!(
                "ignoring Wi-Fi Direct session ready for peer_id={}: pending peer is {}",
                ready.peer_id,
                pending.peer_id
            );
            return;
        }

        if ready.session.peer_ipv4_candidates.is_empty() {
            log::info!(
                "Wi-Fi Direct session ready for peer_id={}, but there are no direct peer IP candidates yet",
                ready.peer_id
            );
            return;
        }

        let endpoint_cache = self.known_mdns_endpoints.borrow();
        let cached = endpoint_cache
            .get(&normalize_device_name(&ready.peer_name))
            .cloned();
        drop(endpoint_cache);

        let known = match cached {
            Some(known) => {
                log::info!(
                    "Wi-Fi Direct handoff using cached mDNS port for peer_id={} port={}",
                    ready.peer_id,
                    known.port
                );
                Some(known)
            }
            None => settings::get_port().map(|port| {
                log::info!(
                    "Wi-Fi Direct handoff using configured fixed port for peer_id={} port={}",
                    ready.peer_id,
                    port
                );
                KnownMdnsEndpoint {
                    name: ready.peer_name.clone(),
                    port: port.to_string(),
                    device_type: DeviceType::Unknown,
                }
            }),
        };

        let Some(known) = known else {
            log::info!(
                "Wi-Fi Direct session ready for peer_id={}, but no cached or configured port is known for peer_name={}",
                ready.peer_id,
                ready.peer_name
            );
            return;
        };

        let Some(ip) = ready.session.peer_ipv4_candidates.first().cloned() else {
            return;
        };

        let transfer_id = format!(
            "wifi-direct-{}-{}",
            ready.peer_id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_micros())
                .unwrap_or_default()
        );
        let retry_files = pending.files.clone();
        let send_info = SendInfo {
            id: transfer_id,
            name: known.name,
            device_type: known.device_type,
            addr: format!("{}:{}", ip, known.port),
            ob: OutboundPayload::Files(pending.files),
        };
        self.sent_requests
            .borrow_mut()
            .insert(
                send_info.id.clone(),
                RetryRequest {
                    name: send_info.name.clone(),
                    device_type: send_info.device_type.clone(),
                    addr: send_info.addr.clone(),
                    files: retry_files,
                },
            );

        if let Err(e) = self.from_ui_tx.try_send(FromUi::SendPayload(send_info)) {
            log::warn!("direct Wi-Fi Direct SendPayload failed: {e}");
            return;
        }

        log::info!(
            "attempting direct Wi-Fi Direct transport for peer_id={} via {}:{}",
            ready.peer_id,
            ip,
            known.port
        );
        *self.pending_wifi_direct_send.borrow_mut() = None;
    }
}

fn normalize_device_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn build_network_summary_text() -> String {
    let port_summary = match settings::get_port() {
        Some(port) => tr!("Fixed port enabled: {}. Remember to allow it in your firewall.")
            .replace("{}", &port.to_string()),
        None => tr!("Random port in use. A fixed port makes firewall rules easier."),
    };

    port_summary
}

fn history_allows_retry(subtitle: &str) -> bool {
    matches!(
        subtitle,
        s if s == tr!("Transfer rejected")
            || s == tr!("Transfer cancelled")
            || s == tr!("Connection lost during transfer")
    )
}

fn build_history_dialog(title: &str, list: &gtk4::ListBox) -> libadwaita::PreferencesDialog {
    let dialog = libadwaita::PreferencesDialog::new();
    dialog.set_title(title);
    dialog.set_search_enabled(false);

    let page = libadwaita::PreferencesPage::new();
    let group = libadwaita::PreferencesGroup::new();
    group.set_description(Some(&history_retention_notice()));

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

fn history_retention_notice() -> String {
    tr!("Transfer history is stored locally for up to {} days by default, unless changed in Settings.")
        .replace("{}", &settings::get_history_retention_days().to_string())
}

fn load_send_history(
    list: &gtk4::ListBox,
    history_button: &gtk4::Button,
    transfer_header: &gtk4::Box,
) {
    let entries = transfer_history::load(HistoryDirection::Send);
    for entry in entries.into_iter().rev() {
        prepend_history_row(list, &entry.title, &entry.subtitle, None, entry.open_target, async_channel::unbounded().0);
    }
    let has_history = list.first_child().is_some();
    history_button.set_visible(has_history);
    transfer_header.set_visible(has_history);
}

fn prepend_history_row(
    list: &gtk4::ListBox,
    title: &str,
    subtitle: &str,
    retry_request: Option<RetryRequest>,
    open_target: Option<String>,
    from_ui_tx: async_channel::Sender<FromUi>,
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

    let icon = gtk4::Image::from_icon_name("history-symbolic");
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

    if let Some(send_info) = retry_request {
        let retry_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
        retry_btn.set_tooltip_text(Some(&tr!("Retry")));
        retry_btn.add_css_class("suggested-action");
        retry_btn.add_css_class("history-icon-button");
        set_pointer_cursor(&retry_btn);
        let tx = from_ui_tx.clone();
        retry_btn.connect_clicked(move |_| {
            let retry_id = format!(
                "{}-history-{}",
                send_info.addr,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_micros())
                    .unwrap_or_default()
            );
            let retry_info = SendInfo {
                id: retry_id,
                name: send_info.name.clone(),
                device_type: send_info.device_type.clone(),
                addr: send_info.addr.clone(),
                ob: OutboundPayload::Files(send_info.files.clone()),
            };
            if let Err(e) = tx.try_send(FromUi::SendPayload(retry_info)) {
                log::warn!("History retry failed: {e}");
            }
        });
        body.append(&retry_btn);
    }

    if let Some(path) = open_target {
        let show_btn = gtk4::Button::from_icon_name("folder-open-symbolic");
        show_btn.set_tooltip_text(Some(&tr!("Show folder")));
        show_btn.add_css_class("history-icon-button");
        set_pointer_cursor(&show_btn);
        show_btn.connect_clicked(move |_| {
            let folder = Path::new(&path)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| Path::new(&path).to_path_buf());
            let uri = gio::File::for_path(folder).uri().to_string();
            if let Err(e) =
                gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
            {
                log::warn!("History show folder failed: {e}");
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

fn rebuild_selected_files_ui(
    selected_files: &Rc<RefCell<Vec<String>>>,
    flow: &gtk4::FlowBox,
    subtitle: &gtk4::Label,
    meta: &gtk4::Label,
    clear_btn: &gtk4::Button,
    upload_icon: &gtk4::Image,
) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }

    let files = selected_files.borrow().clone();
    let count = files.len();

    if count == 0 {
        subtitle.set_text(&tr!("Select"));
        meta.set_text(&tr!("Drop files here or use Select"));
        clear_btn.set_visible(false);
        flow.set_visible(false);
        upload_icon.set_visible(true);
        return;
    }

    subtitle.set_text(&format!(
        "{count} {}",
        if count == 1 { tr!("file") } else { tr!("files") }
    ));
    meta.set_text(&format_total_selected_size(&files));
    clear_btn.set_visible(true);
    flow.set_visible(true);
    upload_icon.set_visible(false);

    for (index, path) in files.iter().enumerate() {
        let file_name = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path)
            .to_string();

        let tile = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        tile.add_css_class("selected-file-tile");
        tile.set_size_request(52, 52);
        tile.set_halign(gtk4::Align::Center);
        tile.set_valign(gtk4::Align::Center);
        tile.set_tooltip_text(Some(&file_name));
        tile.set_hexpand(true);
        tile.set_vexpand(true);
        tile.set_homogeneous(true);

        let icon = gtk4::Image::from_icon_name(file_icon_name(path));
        icon.add_css_class("selected-file-tile-icon");
        icon.set_icon_size(gtk4::IconSize::Large);
        icon.set_halign(gtk4::Align::Center);
        icon.set_valign(gtk4::Align::Center);
        icon.set_hexpand(true);
        icon.set_vexpand(true);
        tile.append(&icon);

        let remove_btn = gtk4::Button::from_icon_name("window-close-symbolic");
        remove_btn.add_css_class("selected-file-remove-badge");
        remove_btn.set_tooltip_text(Some(&tr!("Remove")));
        remove_btn.set_halign(gtk4::Align::End);
        remove_btn.set_valign(gtk4::Align::Start);
        remove_btn.set_margin_top(0);
        remove_btn.set_margin_end(0);
        set_pointer_cursor(&remove_btn);

        {
            let selected_files = Rc::clone(selected_files);
            let flow = flow.clone();
            let subtitle = subtitle.clone();
            let meta = meta.clone();
            let clear_btn = clear_btn.clone();
            let upload_icon = upload_icon.clone();
            remove_btn.connect_clicked(move |_| {
                let len = selected_files.borrow().len();
                if index < len {
                    selected_files.borrow_mut().remove(index);
                    rebuild_selected_files_ui(
                        &selected_files,
                        &flow,
                        &subtitle,
                        &meta,
                        &clear_btn,
                        &upload_icon,
                    );
                }
            });
        }

        let overlay = gtk4::Overlay::new();
        overlay.add_css_class("selected-file-overlay");
        overlay.set_size_request(56, 56);
        overlay.set_halign(gtk4::Align::Start);
        overlay.set_valign(gtk4::Align::Start);
        overlay.set_tooltip_text(Some(&file_name));
        overlay.set_child(Some(&tile));
        overlay.add_overlay(&remove_btn);
        overlay.set_measure_overlay(&remove_btn, true);

        flow.insert(&overlay, -1);
    }
}

fn file_icon_name(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "bmp" | "avif") => "image-x-generic-symbolic",
        Some("mp4" | "mkv" | "avi" | "mov" | "webm" | "m4v") => "video-x-generic-symbolic",
        Some("mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac") => "audio-x-generic-symbolic",
        Some("pdf") => "application-pdf-symbolic",
        Some("zip" | "rar" | "7z" | "tar" | "gz" | "xz") => "package-x-generic-symbolic",
        Some("txt" | "md" | "json" | "toml" | "yaml" | "yml" | "rs" | "c" | "h" | "cpp" | "py" | "js" | "ts") => "text-x-generic-symbolic",
        _ => "text-x-generic-symbolic",
    }
}

fn format_total_selected_size(files: &[String]) -> String {
    let total_bytes = files
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum::<u64>();

    if total_bytes == 0 {
        return tr!("Size unavailable");
    }

    format_size(total_bytes)
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
