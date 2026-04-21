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
    build_pulse_placeholder_sized(title, description, compact, None)
}

pub fn build_pulse_placeholder_sized(
    title: Option<&str>,
    description: Option<&str>,
    compact: bool,
    size_override: Option<i32>,
) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, if compact { 10 } else { 18 });
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.set_halign(gtk4::Align::Center);
    root.set_valign(gtk4::Align::Center);
    root.add_css_class("status-page");

    let default_size = if compact { 120 } else { 180 };
    let pulse = PulseWidget::new(size_override.unwrap_or(default_size));
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
                    // #6E63E8 — roxo azulado
                    (0.431, 0.388, 0.910, 0.08)
                };
                let (ring_r, ring_g, ring_b, ring_alpha_scale) = if is_light {
                    (0.45, 0.37, 0.86, 0.12)
                } else {
                    // #A7A5FF — lilás neon
                    (0.655, 0.647, 1.0, 0.22)
                };
                // #A7A5FF core em dark, roxo em light
                let (core_r, core_g, core_b, core_a) = if is_light {
                    (0.46, 0.39, 0.88, 0.92)
                } else {
                    (0.655, 0.647, 1.0, 0.96)
                };
                // #D9D8FF — lavanda clara para as partículas
                let (orb_r, orb_g, orb_b) = if is_light {
                    (0.50, 0.42, 0.90)
                } else {
                    (0.851, 0.847, 1.000)
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

                // Orbiting signal particles (3 dots, 120° apart)
                let orbit_r = unit * 0.295;
                for i in 0..3usize {
                    let angle = p * 2.0 * PI + (i as f64) * (2.0 * PI / 3.0);
                    let px = cx + angle.cos() * orbit_r;
                    let py = cy + angle.sin() * orbit_r;
                    let dot_r = unit * 0.043;
                    // Depth cue: front-hemisphere particles are brighter
                    let depth = (angle.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                    let dot_a = if is_light { 0.28 + depth * 0.42 } else { 0.40 + depth * 0.44 };
                    cr.set_source_rgba(orb_r, orb_g, orb_b, dot_a);
                    cr.arc(px, py, dot_r, 0.0, 2.0 * PI);
                    let _ = cr.fill();

                    // Short trailing dot ~18° behind
                    let trail_angle = angle - 0.32;
                    let tx = cx + trail_angle.cos() * orbit_r;
                    let ty = cy + trail_angle.sin() * orbit_r;
                    cr.set_source_rgba(orb_r, orb_g, orb_b, dot_a * 0.32);
                    cr.arc(tx, ty, dot_r * 0.55, 0.0, 2.0 * PI);
                    let _ = cr.fill();
                }

                cr.set_source_rgba(core_r, core_g, core_b, core_a);
                cr.arc(cx, cy, unit * 0.13, 0.0, 2.0 * PI);
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
