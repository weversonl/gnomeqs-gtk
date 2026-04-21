use gtk4::prelude::*;

use gnomeqs_core::{DeviceType, EndpointInfo, EndpointTransport};
use crate::tr;
use super::cursor::set_pointer_cursor;

pub struct DeviceTile {
    pub button: gtk4::Button,
}

impl DeviceTile {
    pub fn new(
        endpoint: EndpointInfo,
        get_files: impl Fn() -> Vec<String> + 'static,
        handle_send: impl Fn(EndpointInfo, Vec<String>) + 'static,
    ) -> Self {
        let icon_name = match &endpoint.rtype {
            Some(DeviceType::Phone)  => "phone-symbolic",
            Some(DeviceType::Tablet) => "tablet-symbolic",
            _                        => "computer-symbolic",
        };

        let name = endpoint.name.clone().unwrap_or_else(|| endpoint.id.clone());

        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        vbox.set_margin_top(12);
        vbox.set_margin_bottom(12);
        vbox.set_margin_start(10);
        vbox.set_margin_end(10);
        vbox.set_halign(gtk4::Align::Center);
        vbox.set_hexpand(false);
        vbox.set_vexpand(false);
        vbox.set_valign(gtk4::Align::Center);

        let icon = gtk4::Image::from_icon_name(icon_name);
        icon.set_icon_size(gtk4::IconSize::Large);
        icon.set_pixel_size(44);
        icon.set_margin_top(12);
        icon.set_margin_bottom(12);
        icon.set_margin_start(12);
        icon.set_margin_end(12);

        let icon_circle = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        icon_circle.add_css_class("device-icon-circle");
        icon_circle.set_halign(gtk4::Align::Center);
        icon_circle.set_valign(gtk4::Align::Center);
        icon_circle.set_size_request(82, 82);
        icon_circle.append(&icon);

        let label = gtk4::Label::new(Some(&name));
        label.add_css_class("device-tile-title");
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(12);
        label.set_halign(gtk4::Align::Center);
        label.set_justify(gtk4::Justification::Center);
        label.set_wrap(true);
        label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);

        let transport_text = match endpoint.transport {
            Some(EndpointTransport::WifiDirectPeer) => tr!("Wi-Fi Direct"),
            _ => tr!("Wi-Fi"),
        };

        let transport_badge = gtk4::Label::new(Some(&transport_text));
        transport_badge.add_css_class("device-transport-badge");
        match endpoint.transport {
            Some(EndpointTransport::WifiDirectPeer) => {
                transport_badge.add_css_class("transport-wifi-direct");
            }
            _ => {
                transport_badge.add_css_class("transport-wifi");
            }
        }
        transport_badge.set_halign(gtk4::Align::Center);

        vbox.append(&icon_circle);
        vbox.append(&label);
        vbox.append(&transport_badge);

        let button = gtk4::Button::new();
        button.set_child(Some(&vbox));
        button.add_css_class("flat");
        button.add_css_class("device-tile");
        button.set_halign(gtk4::Align::Center);
        button.set_valign(gtk4::Align::Start);
        button.set_hexpand(false);
        button.set_vexpand(false);
        button.set_size_request(96, -1);

        let interactive = match endpoint.transport {
            Some(EndpointTransport::WifiDirectPeer) => true,
            _ => endpoint.ip.is_some() && endpoint.port.is_some(),
        };
        button.set_sensitive(interactive);
        button.set_tooltip_text(Some(&format!("{name}\n{transport_text}")));
        if interactive {
            set_pointer_cursor(&button);
        }

        let endpoint_clone = endpoint.clone();
        button.connect_clicked(move |_| {
            let files = get_files();
            if files.is_empty() {
                return;
            }
            handle_send(endpoint_clone.clone(), files);
        });
        Self { button }
    }
}
