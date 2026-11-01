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

/// Design tokens. Both themes share every layout and component; only these differ.
#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Color32,
    pub well: Color32,
    /// Light-side shadow of extruded surfaces.
    pub hi: Color32,
    /// Dark-side shadow of extruded surfaces.
    pub lo: Color32,
    /// Hairline that crisps the edge of raised surfaces (mostly visible in dark mode).
    pub edge: Color32,
    /// Backdrop behind playing media (a dark "cinema" stage in both themes).
    pub stage: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    /// VLC orange and its lighter amber partner for gradients.
    pub accent: Color32,
    pub accent2: Color32,
    pub danger: Color32,
    pub ok: Color32,
}

pub fn pal() -> Pal {
    if is_dark() {
        Pal {
            bg: Color32::from_rgb(37, 38, 42),
            well: Color32::from_rgb(29, 30, 33),
            hi: Color32::from_rgba_unmultiplied(255, 255, 255, 13),
            lo: Color32::from_rgba_unmultiplied(0, 0, 0, 170),
            edge: Color32::from_rgba_unmultiplied(255, 255, 255, 14),
            stage: Color32::from_rgb(17, 17, 19),
            text: Color32::from_rgb(190, 193, 200),
            strong: Color32::from_rgb(242, 243, 245),
            muted: Color32::from_rgb(124, 128, 137),
            accent: Color32::from_rgb(255, 140, 10),
            accent2: Color32::from_rgb(255, 186, 66),
            danger: Color32::from_rgb(255, 99, 90),
            ok: Color32::from_rgb(70, 205, 110),
        }
    } else {
        Pal {
            bg: Color32::from_rgb(234, 236, 240),
            well: Color32::from_rgb(226, 229, 234),
            hi: Color32::from_rgba_unmultiplied(255, 255, 255, 240),
            lo: Color32::from_rgba_unmultiplied(160, 167, 180, 140),
            edge: Color32::from_rgba_unmultiplied(255, 255, 255, 90),
            stage: Color32::from_rgb(22, 23, 26),
            text: Color32::from_rgb(88, 94, 106),
            strong: Color32::from_rgb(30, 33, 40),
            muted: Color32::from_rgb(140, 146, 158),
            accent: Color32::from_rgb(255, 128, 0),
            accent2: Color32::from_rgb(255, 172, 40),
            danger: Color32::from_rgb(226, 62, 52),
            ok: Color32::from_rgb(36, 168, 90),
        }
    }
}

/// Eased 0→1 animation for a boolean state (cubic ease-out feels "premium" and snappy).
pub fn ease(ctx: &egui::Context, id: egui::Id, on: bool, seconds: f32) -> f32 {
    ctx.animate_bool_with_time_and_easing(id, on, seconds, egui::emath::easing::cubic_out)
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
    let raised = if dark { Color32::from_rgb(46, 47, 52) } else { Color32::from_rgb(242, 244, 247) };
    let hover = if dark { Color32::from_rgb(54, 55, 61) } else { Color32::from_rgb(248, 249, 251) };
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
        Shape::Rect(RectShape::new(rect, r, fill, Stroke::new(1.0_f32, p.edge))),
    ]
}

/// Soft coloured glow under accent elements (orange halo).
pub fn glow(painter: &egui::Painter, rect: Rect, r: f32, strength: f32) {
    if strength <= 0.01 {
        return;
    }
    let p = pal();
    let shadow = Shadow { offset: vec2(0.0, 4.0), blur: 22.0, spread: 0.0, color: p.accent.gamma_multiply(0.45 * strength) };
    painter.add(shadow.as_shape(rect, r));
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
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
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
        glow(painter, rect, r, 0.35 + 0.65 * hover);
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
        glow(painter, rect.shrink(6.0), r, 0.5 + 0.5 * hover);
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

/// The app logo as the main-menu button: lifts and glows on hover, and its convert
/// ring spins once whenever the pointer arrives or the menu opens/closes.
pub fn logo_button(ui: &mut Ui, size: f32, open: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let ctx = ui.ctx();
        let hover = ease(ctx, resp.id.with("h"), resp.hovered() || open, 0.25);
        let press = ease(ctx, resp.id.with("p"), resp.is_pointer_button_down_on(), 0.08);
        // One full ring turn per state change, eased.
        let spin = ease(ctx, resp.id.with("spin"), resp.hovered() ^ open, 0.7);
        let lift = 1.0 + 0.06 * hover - 0.05 * press;
        let tile = Rect::from_center_size(rect.center(), rect.size() * 0.86 * lift);
        let painter = ui.painter();
        glow(painter, tile, size * 0.24, 0.25 + 0.75 * hover);
        painter.extend(raised_shapes(tile, size * 0.24, 4.0, pal().bg));
        icons::paint_logo_ex(painter, tile.expand(size * 0.04), spin * 360.0);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Full-width row in the main menu drawer.
pub fn menu_item(ui: &mut Ui, icon: Icon, label: &str, detail: Option<&str>, active: bool) -> Response {
    let p = pal();
    let h = if detail.is_some() { 50.0 } else { 42.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.15);
        let painter = ui.painter();
        if active {
            paint_inset(painter, rect, 12.0, 3.0);
            let bar = Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 10.0), vec2(3.0, h - 20.0));
            painter.rect_filled(bar, 1.5, p.accent);
        } else if hover > 0.01 {
            painter.extend(raised_shapes(rect, 12.0, 3.0 * hover, p.bg));
        }
        let color = if active { p.accent } else { lerp_color(p.text, p.strong, hover) };
        let slide = 3.0 * hover;
        let icon_rect = Rect::from_center_size(pos2(rect.left() + 26.0 + slide, rect.center().y), Vec2::splat(17.0));
        icons::paint(painter, icon_rect, icon, if active { p.accent } else { lerp_color(p.text, p.accent, hover) });
        let x = rect.left() + 48.0 + slide;
        match detail {
            Some(d) => {
                painter.text(pos2(x, rect.center().y - 9.0), Align2::LEFT_CENTER, label, FontId::proportional(14.5), color);
                let d = truncate(d, ((rect.right() - x - 10.0) / 6.6) as usize);
                painter.text(pos2(x, rect.center().y + 10.0), Align2::LEFT_CENTER, d, FontId::proportional(11.5), p.muted);
            }
            None => {
                painter.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.5), color);
            }
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Shortens text to `max` characters with an ellipsis.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max.max(4) {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max.max(4) - 1).collect();
        s.push('…');
        s
    }
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
    let avail = ui.available_width();
    let galleys = |size: f32| -> Vec<_> {
        items.iter().map(|(_, t)| ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(size), Color32::PLACEHOLDER)).collect()
    };
    // Natural size first; tighten (smaller text, then equal widths) when space is short.
    let (mut texts, mut pad, mut icons_on) = (galleys(14.5), 54.0, true);
    let natural = |g: &Vec<std::sync::Arc<egui::Galley>>, pad: f32| g.iter().map(|g| g.size().x + pad).sum::<f32>() + 12.0;
    if natural(&texts, pad) > avail {
        texts = galleys(13.0);
        pad = 42.0;
    }
    let mut seg_w: Vec<f32> = texts.iter().map(|g| g.size().x + pad).collect();
    if natural(&texts, pad) > avail {
        let each = (avail - 12.0) / items.len() as f32;
        icons_on = texts.iter().all(|g| g.size().x + 30.0 <= each);
        seg_w = vec![each; items.len()];
    }
    let total = vec2(seg_w.iter().sum::<f32>() + 12.0, 46.0);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    paint_inset(ui.painter(), rect, 23.0, 4.0);

    // One raised indicator glides to the selected segment.
    let lefts: Vec<f32> = seg_w.iter().scan(rect.left() + 6.0, |x, w| {
        let l = *x;
        *x += w;
        Some(l)
    }).collect();
    let sel = (*selected).min(items.len().saturating_sub(1));
    let id = ui.id().with(("seg-ind", items.len()));
    let x = ui.ctx().animate_value_with_time(id.with("x"), lefts[sel], 0.22);
    let w = ui.ctx().animate_value_with_time(id.with("w"), seg_w[sel], 0.22);
    let ind = Rect::from_min_size(pos2(x, rect.top() + 6.0), vec2(w, rect.height() - 12.0));
    ui.painter().extend(raised_shapes(ind, ind.height() / 2.0, 4.0, p.bg));

    let mut changed = false;
    for (i, ((icon, _), galley)) in items.iter().zip(texts).enumerate() {
        let seg = Rect::from_min_size(pos2(lefts[i], rect.top() + 6.0), vec2(seg_w[i], rect.height() - 12.0));
        let resp = ui.interact(seg, ui.id().with(("seg", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() && *selected != i {
            *selected = i;
            changed = true;
        }
        let on = ease(ui.ctx(), resp.id.with("on"), sel == i, 0.2);
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.15);
        let color = lerp_color(lerp_color(p.text, p.strong, hover), p.accent, on);
        let icon_w = if icons_on { 22.0 } else { 0.0 };
        let cx = seg.center().x - (icon_w + galley.size().x) / 2.0;
        if icons_on {
            icons::paint(ui.painter(), Rect::from_center_size(pos2(cx + 8.0, seg.center().y), Vec2::splat(16.0)), *icon, color);
        }
        ui.painter().galley(pos2(cx + icon_w, seg.center().y - galley.size().y / 2.0), galley, color);
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
pub fn rounded_image(painter: &egui::Painter, rect: Rect, r: f32, texture: egui::TextureId, alpha: f32) {
    painter.add(RectShape {
        rect,
        rounding: Rounding::same(r),
        fill: Color32::WHITE.gamma_multiply(alpha.clamp(0.0, 1.0)),
        stroke: Stroke::NONE,
        blur_width: 0.0,
        fill_texture_id: texture,
        uv: Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
    });
}

