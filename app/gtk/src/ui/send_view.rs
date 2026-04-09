use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;

use gnomeqs_core::channel::ChannelMessage;
use gnomeqs_core::{
    DeviceType, EndpointInfo, EndpointTransport, OutboundPayload, SendInfo, State,
};

use crate::bridge::{FromUi, WifiDirectSendRequest, WifiDirectSessionReady};
use crate::settings;
use crate::tr;
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
    transfer_list: gtk4::ListBox,
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
        let endpoint_tx: Rc<RefCell<Option<tokio::sync::broadcast::Sender<EndpointInfo>>>> =
            Rc::new(RefCell::new(None));
        let discovery_active = Rc::new(RefCell::new(false));
        let pending_start: Rc<RefCell<Option<glib::SourceId>>> =
            Rc::new(RefCell::new(None));
        let pending_wifi_direct_send: Rc<RefCell<Option<PendingWifiDirectSend>>> =
            Rc::new(RefCell::new(None));
        let known_mdns_endpoints: Rc<RefCell<HashMap<String, KnownMdnsEndpoint>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // ── File selection area ───────────────────────────────────────────────
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

        // ── Outbound transfer list ──────────────────────────────────────────
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

        // ── File picker button ────────────────────────────────────────────────
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

                // Get the root window
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

        // ── Clear files button ────────────────────────────────────────────────
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

        // ── Drop target ───────────────────────────────────────────────────────
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

        // ── Nearby devices area ───────────────────────────────────────────────
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

        // ── Refresh button ────────────────────────────────────────────────────
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
            transfer_list,
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

    /// Update the device list when an endpoint appears or disappears.
    pub fn update_endpoint(&self, info: EndpointInfo) {
        self.try_auto_send_pending_wifi_direct(&info);

        let present = info.present.unwrap_or(true);
        let mut devices = self.devices.borrow_mut();

        if !present {
            // Remove the tile
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
            return; // Already present, no update needed
        }

        let files = Rc::clone(&self.selected_files);
        let tx = self.from_ui_tx.clone();
        let pending_wifi_direct = Rc::clone(&self.pending_wifi_direct_send);
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

    /// Kick off mDNS discovery when the Send tab is shown.
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

    /// Stop mDNS discovery when the Send tab is hidden.
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
                row.connect_clear(move || {
                    let mut map = transfers.borrow_mut();
                    if let Some(row) = map.remove(&id) {
                        list.remove(&row.row);
                    }
                    list.set_visible(!map.is_empty());
                });
            }
            self.transfer_list.append(&row.row);
            self.transfer_list.set_visible(true);
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
        let send_info = SendInfo {
            id: transfer_id,
            name: known.name,
            device_type: known.device_type,
            addr: format!("{}:{}", ip, known.port),
            ob: OutboundPayload::Files(pending.files),
        };

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
