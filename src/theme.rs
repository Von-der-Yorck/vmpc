//! Neumorphic ("soft UI") design system: palette, extruded/inset surfaces and widgets.
//!
//! egui has no inner shadows or gradients, so surfaces are drawn with vertex-coloured
//! meshes: raised elements get a dark drop shadow bottom-right and a light one
//! top-left, inset elements get a directional gradient ring along their inner edge.

use std::sync::atomic::{AtomicBool, Ordering};

use egui::epaint::{Mesh, RectShape, Shadow};
use egui::{
    pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke, Ui, Vec2,
};

use crate::icons::{self, Icon};

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Color32,
    pub well: Color32,
    pub hi: Color32,
    pub lo: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub danger: Color32,
    pub ok: Color32,
}

pub fn pal() -> Pal {
    if is_dark() {
        Pal {
            bg: Color32::from_rgb(43, 46, 53),
            well: Color32::from_rgb(38, 41, 47),
            hi: Color32::from_rgba_unmultiplied(255, 255, 255, 16),
            lo: Color32::from_rgba_unmultiplied(10, 11, 14, 190),
            text: Color32::from_rgb(200, 206, 217),
            strong: Color32::from_rgb(236, 239, 245),
            muted: Color32::from_rgb(128, 136, 150),
            accent: Color32::from_rgb(124, 108, 255),
            accent2: Color32::from_rgb(64, 196, 255),
            danger: Color32::from_rgb(255, 107, 129),
            ok: Color32::from_rgb(80, 210, 150),
        }
    } else {
        Pal {
            bg: Color32::from_rgb(228, 233, 241),
            well: Color32::from_rgb(220, 226, 235),
            hi: Color32::from_rgba_unmultiplied(255, 255, 255, 235),
            lo: Color32::from_rgba_unmultiplied(155, 168, 191, 150),
            text: Color32::from_rgb(84, 96, 117),
            strong: Color32::from_rgb(45, 55, 72),
            muted: Color32::from_rgb(138, 149, 168),
            accent: Color32::from_rgb(112, 94, 255),
            accent2: Color32::from_rgb(52, 182, 255),
            danger: Color32::from_rgb(234, 84, 110),
            ok: Color32::from_rgb(38, 178, 120),
        }
    }
}

/// Installs the palette into egui's own widgets (text fields, combo boxes, scroll bars...).
pub fn apply(ctx: &egui::Context, dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
    let p = pal();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.faint_bg_color = p.well;
    v.extreme_bg_color = p.well;
    v.code_bg_color = p.well;
    v.override_text_color = None;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.window_rounding = Rounding::same(16.0);
    v.menu_rounding = Rounding::same(12.0);
    v.window_stroke = Stroke::NONE;
    v.window_shadow = Shadow { offset: vec2(6.0, 8.0), blur: 24.0, spread: 0.0, color: p.lo };
    v.popup_shadow = Shadow { offset: vec2(4.0, 6.0), blur: 16.0, spread: 0.0, color: p.lo };
    let widget = |fill: Color32, text: Color32| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::NONE,
        rounding: Rounding::same(10.0),
        fg_stroke: Stroke::new(1.5_f32, text),
        expansion: 0.0,
    };
    let raised = if dark { Color32::from_rgb(50, 54, 62) } else { Color32::from_rgb(236, 240, 246) };
    let hover = if dark { Color32::from_rgb(56, 60, 70) } else { Color32::from_rgb(242, 245, 250) };
    v.widgets.noninteractive = widget(p.bg, p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.lo.gamma_multiply(0.35));
    v.widgets.inactive = widget(raised, p.text);
    v.widgets.hovered = widget(hover, p.strong);
    v.widgets.active = widget(p.well, p.accent);
    v.widgets.open = widget(p.well, p.strong);
    ctx.set_visuals_of(egui::Theme::Light, v.clone());
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.combo_width = 140.0;
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        use egui::{FontFamily::Proportional, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(14.5, Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(14.5, Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(12.0, Proportional));
        s.text_styles.insert(TextStyle::Heading, FontId::new(22.0, Proportional));
    });
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

/// Points and outward normals around a rounded rectangle, clockwise from top-left.
fn rrect_outline(rect: Rect, r: f32) -> Vec<(Pos2, Vec2)> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), 180.0f32),
        (pos2(rect.right() - r, rect.top() + r), 270.0),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
    ];
    let steps = 8;
    let mut out = Vec::with_capacity(4 * (steps + 1));
    for (c, start) in corners {
        for i in 0..=steps {
            let a = (start + 90.0 * i as f32 / steps as f32).to_radians();
            let n = vec2(a.cos(), a.sin());
            out.push((c + n * r, n));
        }
    }
    out
}

/// Fills a rounded rect with a diagonal gradient (top-left `a` → bottom-right `b`).
pub fn gradient_rrect(painter: &egui::Painter, rect: Rect, r: f32, a: Color32, b: Color32) {
    let outline = rrect_outline(rect, r);
    let mut mesh = Mesh::default();
    let color_at = |p: Pos2| {
        let t = (((p.x - rect.left()) / rect.width().max(1.0)) + ((p.y - rect.top()) / rect.height().max(1.0))) / 2.0;
        lerp_color(a, b, t.clamp(0.0, 1.0))
    };
    let center = rect.center();
    mesh.colored_vertex(center, color_at(center));
    for (p, _) in &outline {
        mesh.colored_vertex(*p, color_at(*p));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
    // Thin anti-aliased edge so the mesh does not look jagged.
    painter.add(RectShape::stroke(rect, Rounding::same(r), Stroke::new(0.6_f32, lerp_color(a, b, 0.5).gamma_multiply(0.6))));
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Soft extruded surface.
pub fn raised_shapes(rect: Rect, r: f32, depth: f32, fill: Color32) -> Vec<Shape> {
    let p = pal();
    let dark = Shadow { offset: vec2(depth, depth), blur: depth * 2.6, spread: 0.0, color: p.lo };
    let light = Shadow { offset: vec2(-depth, -depth), blur: depth * 2.6, spread: 0.0, color: p.hi };
    vec![
        Shape::Rect(light.as_shape(rect, r)),
        Shape::Rect(dark.as_shape(rect, r)),
        Shape::Rect(RectShape::filled(rect, r, fill)),
    ]
}

/// Pressed-in surface with directional inner shadow.
pub fn inset_shapes(rect: Rect, r: f32, depth: f32) -> Vec<Shape> {
    let p = pal();
    let mut shapes = vec![Shape::Rect(RectShape::filled(rect, r, p.well))];
    let width = (depth * 1.8).min(r.max(2.0)).max(1.5);
    let outline = rrect_outline(rect, r);
    let mut mesh = Mesh::default();
    let diag = std::f32::consts::FRAC_1_SQRT_2;
    let (lo, hi) = (p.lo, p.hi);
    for (pt, n) in &outline {
        let toward_light = (-n.x - n.y) * diag; // top-left edges
        let c = if toward_light > 0.0 {
            lo.gamma_multiply(toward_light.min(1.0))
        } else {
            hi.gamma_multiply((-toward_light).min(1.0))
        };
        mesh.colored_vertex(*pt, c);
        mesh.colored_vertex(*pt - *n * width, Color32::TRANSPARENT);
    }
    let n = outline.len() as u32;
    for i in 0..n {
        let (a, b) = (2 * i, 2 * ((i + 1) % n));
        mesh.add_triangle(a, b, a + 1);
        mesh.add_triangle(b, b + 1, a + 1);
    }
    shapes.push(Shape::mesh(mesh));
    shapes
}

pub fn paint_raised(painter: &egui::Painter, rect: Rect, r: f32, depth: f32) {
    painter.extend(raised_shapes(rect, r, depth, pal().bg));
}

pub fn paint_inset(painter: &egui::Painter, rect: Rect, r: f32, depth: f32) {
    painter.extend(inset_shapes(rect, r, depth));
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// Raised card around arbitrary content.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    surface(ui, false, 18.0, 16.0, add)
}

/// Inset well around arbitrary content.
pub fn well<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    surface(ui, true, 14.0, 12.0, add)
}

fn surface<R>(ui: &mut Ui, inset: bool, radius: f32, margin: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let bg = ui.painter().add(Shape::Noop);
    let inner = egui::Frame::none().inner_margin(margin).show(ui, add);
    let rect = inner.response.rect;
    let shapes = if inset { inset_shapes(rect, radius, 5.0) } else { raised_shapes(rect, radius, 7.0, pal().bg) };
    ui.painter().set(bg, Shape::Vec(shapes));
    inner.inner
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(16.0).strong().color(pal().strong));
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.5).color(pal().muted));
}

// ---------------------------------------------------------------------------
// Widgets
// ---------------------------------------------------------------------------

fn press_anim(ui: &Ui, resp: &Response) -> (f32, f32) {
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
    let press = ui.ctx().animate_bool_with_time(resp.id.with("p"), resp.is_pointer_button_down_on(), 0.06);
    (hover, press)
}

/// Soft button with text. `primary` uses the accent gradient.
pub fn button(ui: &mut Ui, text: &str, primary: bool) -> Response {
    button_ex(ui, None, text, primary, true)
}

pub fn button_icon(ui: &mut Ui, icon: Icon, text: &str, primary: bool) -> Response {
    button_ex(ui, Some(icon), text, primary, true)
}

pub fn button_ex(ui: &mut Ui, icon: Option<Icon>, text: &str, primary: bool, enabled: bool) -> Response {
    let p = pal();
    let font = FontId::proportional(14.5);
    let text_color = if primary { Color32::WHITE } else { p.strong };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, text_color);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 32.0, 38.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let (hover, press) = press_anim(ui, &resp);
    let painter = ui.painter();
    let r = 12.0;
    if primary && enabled {
        painter.extend(raised_shapes(rect, r, 5.0 * (1.0 - press), p.bg));
        let (a, b) = (p.accent, p.accent2);
        let k = 1.0 + 0.08 * hover;
        gradient_rrect(painter, rect.shrink(press), r, a.gamma_multiply(k), b.gamma_multiply(k));
    } else if press > 0.5 || !enabled {
        paint_inset(painter, rect, r, 3.5);
    } else {
        painter.extend(raised_shapes(rect, r, 5.0 - 1.5 * hover, p.bg));
    }
    let color = if !enabled {
        p.muted
    } else if primary {
        Color32::WHITE
    } else {
        lerp_color(p.strong, p.accent, hover)
    };
    let content_w = galley.size().x + icon_w;
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(icon) = icon {
        let ir = Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0));
        icons::paint(painter, ir, icon, color);
        x += icon_w;
    }
    painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
    resp.on_hover_cursor(if enabled { CursorIcon::PointingHand } else { CursorIcon::NotAllowed })
}

/// Round icon button. `active` lights the icon in the accent colour.
pub fn icon_button(ui: &mut Ui, icon: Icon, diameter: f32, active: bool, tip: &str) -> Response {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    if ui.is_rect_visible(rect) {
        let (hover, press) = press_anim(ui, &resp);
        let painter = ui.painter();
        let r = diameter / 2.0;
        if press > 0.5 || active {
            paint_inset(painter, rect, r, 3.5);
        } else {
            painter.extend(raised_shapes(rect, r, (diameter / 9.0).min(5.0) * (1.0 - 0.3 * hover), p.bg));
        }
        let color = if active { p.accent } else { lerp_color(p.text, p.accent, hover) };
        icons::paint(painter, rect.shrink(diameter * 0.3), icon, color);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
}

/// Big round gradient button (play/pause).
pub fn hero_button(ui: &mut Ui, icon: Icon, diameter: f32, tip: &str) -> Response {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    if ui.is_rect_visible(rect) {
        let (hover, press) = press_anim(ui, &resp);
        let painter = ui.painter();
        let r = diameter / 2.0;
        painter.extend(raised_shapes(rect, r, 7.0 * (1.0 - 0.6 * press), p.bg));
        let inner = rect.shrink(5.0 + press);
        let k = 1.0 + 0.1 * hover;
        gradient_rrect(painter, inner, inner.width() / 2.0, p.accent.gamma_multiply(k), p.accent2.gamma_multiply(k));
        let mut ir = rect.shrink(diameter * 0.32);
        if icon == Icon::Play {
            ir = ir.translate(vec2(diameter * 0.03, 0.0));
        }
        icons::paint(painter, ir, icon, Color32::WHITE);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
}

/// Horizontal soft slider for values in 0..=1. Returns the response; `value` is
/// updated while dragging/clicking.
pub fn slider(ui: &mut Ui, value: &mut f32, width: f32, height: f32, buffered: Option<f32>) -> Response {
    let p = pal();
    let knob_d = height + 10.0;
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, knob_d), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(width - knob_d, height));
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.is_pointer_button_down_on() || resp.clicked() || resp.dragged() {
            let v = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            if (v - *value).abs() > f32::EPSILON {
                *value = v;
                resp.mark_changed();
            }
        }
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hover = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered() || resp.dragged(), 0.12);
        paint_inset(painter, track, height / 2.0, 2.5);
        if let Some(b) = buffered {
            let br = Rect::from_min_max(track.min, pos2(track.left() + track.width() * b.clamp(0.0, 1.0), track.bottom()));
            painter.rect_filled(br, height / 2.0, p.muted.gamma_multiply(0.15));
        }
        let v = value.clamp(0.0, 1.0);
        let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * v, track.bottom()));
        if fill.width() > 1.0 {
            gradient_rrect(painter, fill, height / 2.0, p.accent, p.accent2);
        }
        let knob = Rect::from_center_size(pos2(fill.right(), track.center().y), Vec2::splat(knob_d - 2.0 + 2.0 * hover));
        painter.extend(raised_shapes(knob, knob.width() / 2.0, 3.0, p.bg));
        painter.circle_filled(knob.center(), knob.width() * 0.18, lerp_color(p.accent, p.accent2, v));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Segmented control. Returns true if the selection changed.
pub fn segmented(ui: &mut Ui, selected: &mut usize, items: &[(Icon, &str)]) -> bool {
    let p = pal();
    let font = FontId::proportional(14.5);
    let galleys: Vec<_> = items
        .iter()
        .map(|(_, t)| ui.painter().layout_no_wrap(t.to_string(), font.clone(), p.text))
        .collect();
    let seg_w: Vec<f32> = galleys.iter().map(|g| g.size().x + 54.0).collect();
    let total = vec2(seg_w.iter().sum::<f32>() + 12.0, 46.0);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    paint_inset(ui.painter(), rect, 23.0, 4.0);
    let mut changed = false;
    let mut x = rect.left() + 6.0;
    for (i, ((icon, _), galley)) in items.iter().zip(galleys).enumerate() {
        let seg = Rect::from_min_size(pos2(x, rect.top() + 6.0), vec2(seg_w[i], rect.height() - 12.0));
        x += seg_w[i];
        let resp = ui.interact(seg, ui.id().with(("seg", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() && *selected != i {
            *selected = i;
            changed = true;
        }
        let on = *selected == i;
        let t = ui.ctx().animate_bool_with_time(resp.id.with("on"), on, 0.15);
        if t > 0.01 {
            let shapes = raised_shapes(seg, seg.height() / 2.0, 4.0 * t, p.bg);
            ui.painter().extend(shapes);
        }
        let color = if on { p.accent } else if resp.hovered() { p.strong } else { p.text };
        let content = 22.0 + galley.size().x;
        let cx = seg.center().x - content / 2.0;
        icons::paint(ui.painter(), Rect::from_center_size(pos2(cx + 8.0, seg.center().y), Vec2::splat(16.0)), *icon, color);
        ui.painter().galley(pos2(cx + 22.0, seg.center().y - galley.size().y / 2.0), galley, color);
    }
    changed
}

/// Soft switch with label.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = pal();
    let size = vec2(44.0, 24.0);
    let resp = ui
        .horizontal(|ui| {
            let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
            if resp.clicked() {
                *on = !*on;
                resp.mark_changed();
            }
            let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
            paint_inset(ui.painter(), rect, 12.0, 3.0);
            if t > 0.01 {
                let fill = rect.shrink(3.0);
                gradient_rrect(ui.painter(), fill, 9.0, p.accent.gamma_multiply(t), p.accent2.gamma_multiply(t));
            }
            let x = rect.left() + 12.0 + (rect.width() - 24.0) * t;
            let knob = Rect::from_center_size(pos2(x, rect.center().y), Vec2::splat(18.0));
            ui.painter().extend(raised_shapes(knob, 9.0, 2.0, p.bg));
            let label_resp = ui.add(egui::Label::new(egui::RichText::new(label).color(p.text)).sense(Sense::click()));
            if label_resp.clicked() {
                *on = !*on;
                resp.mark_changed();
            }
            resp
        })
        .inner;
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Progress bar 0..=1.
pub fn progress(ui: &mut Ui, frac: f32, width: f32, color_ok: bool) {
    let p = pal();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 12.0), Sense::hover());
    paint_inset(ui.painter(), rect, 6.0, 2.5);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        let fill = Rect::from_min_max(rect.min, pos2(rect.left() + (rect.width() * f).max(12.0), rect.bottom()));
        if color_ok {
            gradient_rrect(ui.painter(), fill, 6.0, p.ok, p.ok.gamma_multiply(0.8));
        } else {
            gradient_rrect(ui.painter(), fill, 6.0, p.accent, p.accent2);
        }
    }
}

/// Inset drop area for files. Clicking it is reported in the response.
pub fn drop_zone(ui: &mut Ui, size: Vec2, title: &str, subtitle: &str, hovering: bool) -> Response {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered() || hovering, 0.15);
    let painter = ui.painter();
    paint_inset(painter, rect, 24.0, 7.0);
    // Dashed outline.
    let inner = rect.shrink(14.0);
    let outline = rrect_outline(inner, 16.0);
    let dash_color = lerp_color(p.muted.gamma_multiply(0.5), p.accent, hover);
    for (i, w) in outline.windows(2).enumerate() {
        if i % 2 == 0 {
            painter.line_segment([w[0].0, w[1].0], Stroke::new(1.5_f32, dash_color));
        }
    }
    let c = rect.center() - vec2(0.0, 26.0);
    let disc = Rect::from_center_size(c, Vec2::splat(72.0 + 6.0 * hover));
    painter.extend(raised_shapes(disc, disc.width() / 2.0, 6.0, p.bg));
    icons::paint(painter, Rect::from_center_size(c, Vec2::splat(30.0)), Icon::Upload, lerp_color(p.text, p.accent, hover));
    painter.text(c + vec2(0.0, 62.0), Align2::CENTER_CENTER, title, FontId::proportional(18.0), p.strong);
    painter.text(c + vec2(0.0, 86.0), Align2::CENTER_CENTER, subtitle, FontId::proportional(13.0), p.muted);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Small rounded "chip" label.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(16.0, 8.0), Sense::hover());
    paint_inset(ui.painter(), rect, rect.height() / 2.0, 2.0);
    ui.painter().galley(rect.min + vec2(8.0, 4.0), galley, color);
}

/// Draws a texture clipped to a rounded rectangle.
pub fn rounded_image(painter: &egui::Painter, rect: Rect, r: f32, texture: egui::TextureId) {
    painter.add(RectShape {
        rect,
        rounding: Rounding::same(r),
        fill: Color32::WHITE,
        stroke: Stroke::NONE,
        blur_width: 0.0,
        fill_texture_id: texture,
        uv: Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
    });
}

