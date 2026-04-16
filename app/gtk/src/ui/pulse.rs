use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

pub fn build_pulse_placeholder(
    title: Option<&str>,
    description: Option<&str>,
    compact: bool,
) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, if compact { 10 } else { 18 });
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.set_halign(gtk4::Align::Center);
    root.set_valign(gtk4::Align::Center);
    root.add_css_class("status-page");

    let pulse = PulseWidget::new(if compact { 120 } else { 180 });
    root.append(&pulse.area);

    if let Some(title) = title {
        let title_label = gtk4::Label::new(Some(title));
        title_label.add_css_class("title-3");
        title_label.set_wrap(true);
        title_label.set_justify(gtk4::Justification::Center);
        title_label.set_halign(gtk4::Align::Center);
        root.append(&title_label);
    }

    if let Some(description) = description {
        let desc_label = gtk4::Label::new(Some(description));
        if title.is_none() {
            desc_label.add_css_class("title-3");
        }
        desc_label.set_wrap(true);
        desc_label.set_justify(gtk4::Justification::Center);
        desc_label.set_halign(gtk4::Align::Center);
        desc_label.set_max_width_chars(if compact { 24 } else { 42 });
        if title.is_some() {
            desc_label.add_css_class("dim-label");
        }
        root.append(&desc_label);
    }

    root
}

struct PulseWidget {
    area: gtk4::DrawingArea,
    _tick: glib::SourceId,
}

impl PulseWidget {
    fn new(size: i32) -> Self {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(size);
        area.set_content_height(size);
        area.set_hexpand(false);
        area.set_vexpand(false);

        let phase = Rc::new(Cell::new(0.0f64));
        {
            let phase = phase.clone();
            area.set_draw_func(move |widget, cr, width, height| {
                let width = width as f64;
                let height = height as f64;
                let cx = width / 2.0;
                let cy = height / 2.0;
                let unit = width.min(height) / 2.0;
                let p = phase.get();
                let is_light = widget
                    .root()
                    .map(|root| root.has_css_class("light-mode"))
                    .unwrap_or(false);

                cr.set_antialias(gtk4::cairo::Antialias::Best);

                let (ambient_r, ambient_g, ambient_b, ambient_a) = if is_light {
                    (0.51, 0.43, 0.92, 0.05)
                } else {
                    (1.0, 1.0, 1.0, 0.06)
                };
                let (ring_r, ring_g, ring_b, ring_alpha_scale) = if is_light {
                    (0.45, 0.37, 0.86, 0.12)
                } else {
                    (1.0, 1.0, 1.0, 0.18)
                };
                let (core_r, core_g, core_b, core_a) = if is_light {
                    (0.46, 0.39, 0.88, 0.92)
                } else {
                    (1.0, 1.0, 1.0, 0.96)
                };

                cr.set_source_rgba(ambient_r, ambient_g, ambient_b, ambient_a);
                cr.arc(cx, cy, unit * 0.58, 0.0, 2.0 * PI);
                let _ = cr.fill();

                for offset in [0.0f64, 0.33, 0.66] {
                    let mut t = p + offset;
                    if t >= 1.0 {
                        t -= 1.0;
                    }
                    let eased = t * t * (3.0 - 2.0 * t);
                    let radius = unit * (0.16 + (0.52 * eased));
                    let alpha = (1.0 - eased).powf(1.9) * ring_alpha_scale;
                    cr.set_source_rgba(ring_r, ring_g, ring_b, alpha);
                    cr.arc(cx, cy, radius, 0.0, 2.0 * PI);
                    let _ = cr.fill();
                }

                cr.set_source_rgba(core_r, core_g, core_b, core_a);
                cr.arc(cx, cy, unit * 0.12, 0.0, 2.0 * PI);
                let _ = cr.fill();
            });
        }

        let area_weak = area.downgrade();
        let tick = glib::timeout_add_local(Duration::from_millis(16), move || {
            let Some(area) = area_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let mut next = phase.get() + 0.0065;
            if next >= 1.0 {
                next -= 1.0;
            }
            phase.set(next);
            area.queue_draw();
            glib::ControlFlow::Continue
        });

        Self { area, _tick: tick }
    }
}
