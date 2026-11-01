//! Design system: a calm, flat look inspired by the Claude app (warm neutral surfaces,
//! hairline borders, generous spacing, serif headlines) with VLC orange as the accent.
//! Light and dark mode share every component; only the tokens in [`Pal`] differ.

use std::sync::atomic::{AtomicBool, Ordering};

use egui::epaint::{RectShape, Shadow};
use egui::{
    pos2, vec2, Align2, Color32, CursorIcon, FontFamily, FontId, Rect, Response, Rounding, Sense, Shape, Stroke, Ui,
    Vec2,
};

use crate::icons::{self, Icon};

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Design tokens.
#[derive(Clone, Copy)]
pub struct Pal {
    /// Main canvas.
    pub bg: Color32,
    /// Sidebar (slightly recessed from the canvas).
    pub side: Color32,
    /// Cards, panels, inputs.
    pub surface: Color32,
    /// Hover / pressed tint and quiet fills.
    pub tint: Color32,
    pub border: Color32,
    /// Backdrop behind playing media.
    pub stage: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub muted: Color32,
    /// VLC orange.
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Orange wash for selected states.
    pub accent_soft: Color32,
    pub danger: Color32,
    pub ok: Color32,
    pub shadow: Color32,
}

pub fn pal() -> Pal {
    if is_dark() {
        Pal {
            bg: Color32::from_rgb(38, 38, 36),
            side: Color32::from_rgb(31, 30, 29),
            surface: Color32::from_rgb(48, 48, 46),
            tint: Color32::from_rgb(58, 58, 55),
            border: Color32::from_rgb(64, 63, 59),
            stage: Color32::from_rgb(20, 20, 19),
            text: Color32::from_rgb(210, 207, 198),
            strong: Color32::from_rgb(250, 249, 245),
            muted: Color32::from_rgb(156, 154, 146),
            accent: Color32::from_rgb(255, 145, 36),
            accent_hover: Color32::from_rgb(255, 165, 77),
            accent_soft: Color32::from_rgba_unmultiplied(255, 145, 36, 38),
            danger: Color32::from_rgb(242, 106, 92),
            ok: Color32::from_rgb(99, 194, 120),
            shadow: Color32::from_black_alpha(110),
        }
    } else {
        Pal {
            bg: Color32::from_rgb(250, 249, 245),
            side: Color32::from_rgb(243, 241, 234),
            surface: Color32::WHITE,
            tint: Color32::from_rgb(240, 238, 230),
            border: Color32::from_rgb(229, 226, 216),
            stage: Color32::from_rgb(20, 20, 19),
            text: Color32::from_rgb(61, 60, 56),
            strong: Color32::from_rgb(31, 30, 29),
            muted: Color32::from_rgb(138, 136, 128),
            accent: Color32::from_rgb(255, 128, 0),
            accent_hover: Color32::from_rgb(235, 112, 0),
            accent_soft: Color32::from_rgba_unmultiplied(255, 128, 0, 26),
            danger: Color32::from_rgb(214, 69, 54),
            ok: Color32::from_rgb(46, 150, 82),
            shadow: Color32::from_black_alpha(22),
        }
    }
}

/// Eased 0→1 animation for a boolean state (cubic ease-out).
pub fn ease(ctx: &egui::Context, id: egui::Id, on: bool, seconds: f32) -> f32 {
    ctx.animate_bool_with_time_and_easing(id, on, seconds, egui::emath::easing::cubic_out)
}

/// Serif face for headlines (Source Serif 4), like Claude's greeting.
pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}

/// Installs Inter (UI) and Source Serif 4 (headlines); both SIL Open Font License.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("inter".into(), egui::FontData::from_static(include_bytes!("../assets/fonts/Inter.ttf")));
    fonts
        .font_data
        .insert("serif".into(), egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSerif4.ttf")));
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "inter".into());
        // Arrows and symbols fall back to the bundled monospace font.
        family.push("Hack".into());
    }
    fonts.families.insert(FontFamily::Name("serif".into()), vec!["serif".into(), "inter".into(), "Hack".into()]);
    ctx.set_fonts(fonts);
}

/// Installs the palette into egui's own widgets (text fields, combo boxes, scroll bars, windows).
pub fn apply(ctx: &egui::Context, dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
    let p = pal();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.faint_bg_color = p.tint;
    v.extreme_bg_color = p.surface;
    v.code_bg_color = p.tint;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.3);
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(10.0);
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_shadow = Shadow { offset: vec2(0.0, 10.0), blur: 36.0, spread: 0.0, color: p.shadow };
    v.popup_shadow = Shadow { offset: vec2(0.0, 6.0), blur: 20.0, spread: 0.0, color: p.shadow };
    let widget = |fill: Color32, stroke: Color32, text: Color32| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0_f32, stroke),
        rounding: Rounding::same(8.0),
        fg_stroke: Stroke::new(1.5_f32, text),
        expansion: 0.0,
    };
    v.widgets.noninteractive = widget(p.bg, p.border, p.text);
    v.widgets.inactive = widget(p.surface, p.border, p.text);
    v.widgets.hovered = widget(p.tint, p.border, p.strong);
    v.widgets.active = widget(p.tint, p.accent, p.strong);
    v.widgets.open = widget(p.tint, p.border, p.strong);
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);
    ctx.set_visuals_of(egui::Theme::Light, v.clone());
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.combo_width = 150.0;
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        use egui::{FontFamily::Proportional, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::new(14.0, Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(14.0, Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(12.0, Proportional));
        s.text_styles.insert(TextStyle::Heading, serif(24.0));
    });
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Flat surface: fill plus a hairline border.
pub fn surface_shape(rect: Rect, r: f32, fill: Color32, border: Color32) -> Shape {
    Shape::Rect(RectShape::new(rect, r, fill, Stroke::new(1.0_f32, border)))
}

/// Very soft drop shadow for floating things (menus, dialogs, the composer).
pub fn soft_shadow(painter: &egui::Painter, rect: Rect, r: f32, strength: f32) {
    let p = pal();
    let s = Shadow { offset: vec2(0.0, 6.0), blur: 24.0, spread: 0.0, color: p.shadow.gamma_multiply(strength) };
    painter.add(s.as_shape(rect, r));
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

// ---------------------------------------------------------------------------
// Containers and text
// ---------------------------------------------------------------------------


/// Quiet tinted block (list rows, job entries).
pub fn well<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none().fill(pal().tint).rounding(10.0).inner_margin(10.0).show(ui, add).inner
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.5).color(pal().muted));
}

/// Small uppercase group label.
pub fn section(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.0).strong().color(pal().muted));
}

/// Serif headline.
pub fn headline(ui: &mut Ui, text: &str, size: f32) {
    ui.label(egui::RichText::new(text).font(serif(size)).color(pal().strong));
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

pub fn button(ui: &mut Ui, text: &str, primary: bool) -> Response {
    button_ex(ui, None, text, primary, true)
}

pub fn button_icon(ui: &mut Ui, icon: Icon, text: &str, primary: bool) -> Response {
    button_ex(ui, Some(icon), text, primary, true)
}

/// Primary = solid orange; secondary = outlined surface button.
pub fn button_ex(ui: &mut Ui, icon: Option<Icon>, text: &str, primary: bool, enabled: bool) -> Response {
    let p = pal();
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(14.0), Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 28.0, 36.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered() && enabled, 0.12);
        let painter = ui.painter();
        let (fill, border, color) = if !enabled {
            (p.tint, p.tint, p.muted)
        } else if primary {
            let fill = lerp_color(p.accent, p.accent_hover, hover);
            (fill, fill, Color32::WHITE)
        } else {
            (lerp_color(p.surface, p.tint, hover), p.border, p.strong)
        };
        let r = rect.shrink(if resp.is_pointer_button_down_on() { 0.5 } else { 0.0 });
        painter.add(surface_shape(r, 9.0, fill, border));
        let mut x = rect.center().x - (galley.size().x + icon_w) / 2.0;
        if let Some(icon) = icon {
            icons::paint(painter, Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(15.0)), icon, color);
            x += icon_w;
        }
        painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    resp.on_hover_cursor(if enabled { CursorIcon::PointingHand } else { CursorIcon::NotAllowed })
}

/// Square ghost icon button; `active` shows an orange wash.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, active: bool, tip: &str) -> Response {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.12);
        let painter = ui.painter();
        if active {
            painter.rect_filled(rect, 9.0, p.accent_soft);
        } else if hover > 0.01 {
            painter.rect_filled(rect, 9.0, p.tint.gamma_multiply(hover));
        }
        let color = if active { p.accent } else { lerp_color(p.text, p.strong, hover) };
        icons::paint(painter, rect.shrink(size * 0.29), icon, color);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
}

/// Round solid orange play/pause button.
pub fn hero_button(ui: &mut Ui, icon: Icon, diameter: f32, tip: &str) -> Response {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.12);
        let press = if resp.is_pointer_button_down_on() { 1.5 } else { 0.0 };
        let r = rect.shrink(press);
        ui.painter().circle_filled(r.center(), r.width() / 2.0, lerp_color(p.accent, p.accent_hover, hover));
        let mut ir = rect.shrink(diameter * 0.32);
        if icon == Icon::Play {
            ir = ir.translate(vec2(diameter * 0.03, 0.0));
        }
        icons::paint(ui.painter(), ir, icon, Color32::WHITE);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip)
}

/// The app logo as a button: lifts on hover and its convert ring spins once
/// whenever the pointer arrives or the sidebar opens/closes.
pub fn logo_button(ui: &mut Ui, size: f32, open: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let ctx = ui.ctx();
        let hover = ease(ctx, resp.id.with("h"), resp.hovered(), 0.2);
        let spin = ease(ctx, resp.id.with("spin"), resp.hovered() ^ open, 0.7);
        let scale = 0.92 + 0.06 * hover;
        let tile = Rect::from_center_size(rect.center(), rect.size() * scale);
        icons::paint_logo_ex(ui.painter(), tile, spin * 360.0);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Sidebar row (Claude-style): quiet until hovered, tinted when active.
pub fn menu_item(ui: &mut Ui, icon: Icon, label: &str, detail: Option<&str>, active: bool) -> Response {
    let p = pal();
    let h = if detail.is_some() { 46.0 } else { 36.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.12);
        let painter = ui.painter();
        if active {
            painter.rect_filled(rect, 9.0, p.tint);
        } else if hover > 0.01 {
            painter.rect_filled(rect, 9.0, p.tint.gamma_multiply(0.7 * hover));
        }
        let color = if active { p.strong } else { lerp_color(p.text, p.strong, hover) };
        let icon_color = if active { p.accent } else { lerp_color(p.muted, p.text, hover) };
        icons::paint(painter, Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(15.0)), icon, icon_color);
        let x = rect.left() + 38.0;
        let max_chars = ((rect.right() - x - 30.0) / 6.8).max(6.0) as usize;
        match detail {
            Some(d) => {
                painter.text(pos2(x, rect.center().y - 8.0), Align2::LEFT_CENTER, truncate(label, max_chars), FontId::proportional(13.5), color);
                painter.text(pos2(x, rect.center().y + 9.0), Align2::LEFT_CENTER, truncate(d, max_chars + 4), FontId::proportional(11.5), p.muted);
            }
            None => {
                painter.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, truncate(label, max_chars), FontId::proportional(13.5), color);
            }
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Thin slider for values in 0..=1; `value` is updated while dragging/clicking.
pub fn slider(ui: &mut Ui, value: &mut f32, width: f32, _height: f32, _buffered: Option<f32>) -> Response {
    let p = pal();
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(width - 14.0, 4.0));
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
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered() || resp.dragged(), 0.12);
        let painter = ui.painter();
        painter.rect_filled(track, 2.0, p.border);
        let v = value.clamp(0.0, 1.0);
        let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * v, track.bottom()));
        painter.rect_filled(fill, 2.0, p.accent);
        let knob = pos2(fill.right(), track.center().y);
        let r = 6.0 + 1.5 * hover;
        painter.circle_filled(knob + vec2(0.0, 1.0), r + 0.5, p.shadow);
        painter.circle(knob, r, Color32::WHITE, Stroke::new(1.0_f32, p.border));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Segmented control; a surface "thumb" glides to the selected item.
/// Returns true if the selection changed.
pub fn segmented(ui: &mut Ui, selected: &mut usize, items: &[(Icon, &str)]) -> bool {
    let p = pal();
    let avail = ui.available_width();
    let galleys: Vec<_> = items
        .iter()
        .map(|(_, t)| ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(13.0), Color32::PLACEHOLDER))
        .collect();
    let natural: Vec<f32> = galleys.iter().map(|g| g.size().x + 40.0).collect();
    let mut icons_on = true;
    let seg_w: Vec<f32> = if natural.iter().sum::<f32>() + 8.0 > avail {
        let each = (avail - 8.0) / items.len() as f32;
        icons_on = galleys.iter().all(|g| g.size().x + 30.0 <= each);
        vec![each; items.len()]
    } else {
        natural
    };
    let (rect, _) = ui.allocate_exact_size(vec2(seg_w.iter().sum::<f32>() + 8.0, 36.0), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, p.tint);
    let lefts: Vec<f32> = seg_w
        .iter()
        .scan(rect.left() + 4.0, |x, w| {
            let l = *x;
            *x += w;
            Some(l)
        })
        .collect();
    let sel = (*selected).min(items.len().saturating_sub(1));
    let id = ui.id().with(("seg", items.len()));
    let x = ui.ctx().animate_value_with_time(id.with("x"), lefts[sel], 0.2);
    let w = ui.ctx().animate_value_with_time(id.with("w"), seg_w[sel], 0.2);
    let thumb = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(w, rect.height() - 8.0));
    ui.painter().add(Shadow { offset: vec2(0.0, 1.0), blur: 4.0, spread: 0.0, color: p.shadow }.as_shape(thumb, 7.0));
    ui.painter().add(surface_shape(thumb, 7.0, p.surface, p.border));
    let mut changed = false;
    for (i, ((icon, _), galley)) in items.iter().zip(galleys).enumerate() {
        let seg = Rect::from_min_size(pos2(lefts[i], rect.top() + 4.0), vec2(seg_w[i], rect.height() - 8.0));
        let resp = ui.interact(seg, ui.id().with(("seg-item", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() && *selected != i {
            *selected = i;
            changed = true;
        }
        let on = ease(ui.ctx(), resp.id.with("on"), sel == i, 0.2);
        let hover = ease(ui.ctx(), resp.id.with("h"), resp.hovered(), 0.12);
        let color = lerp_color(lerp_color(p.muted, p.text, hover), p.strong, on);
        let icon_w = if icons_on { 20.0 } else { 0.0 };
        let cx = seg.center().x - (icon_w + galley.size().x) / 2.0;
        if icons_on {
            let ic = lerp_color(color, p.accent, on);
            icons::paint(ui.painter(), Rect::from_center_size(pos2(cx + 7.0, seg.center().y), Vec2::splat(14.0)), *icon, ic);
        }
        ui.painter().galley(pos2(cx + icon_w, seg.center().y - galley.size().y / 2.0), galley, color);
    }
    changed
}

/// Switch with label.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = pal();
    let resp = ui
        .horizontal(|ui| {
            let (rect, mut resp) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::click());
            let label_resp = ui.add(egui::Label::new(egui::RichText::new(label).color(p.text)).sense(Sense::click()));
            if resp.clicked() || label_resp.clicked() {
                *on = !*on;
                resp.mark_changed();
            }
            let t = ease(ui.ctx(), resp.id, *on, 0.18);
            ui.painter().rect_filled(rect, 10.0, lerp_color(p.border, p.accent, t));
            let knob = pos2(rect.left() + 10.0 + 14.0 * t, rect.center().y);
            ui.painter().circle_filled(knob, 7.5, Color32::WHITE);
            resp
        })
        .inner;
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Thin progress bar 0..=1.
pub fn progress(ui: &mut Ui, frac: f32, width: f32, color_ok: bool) {
    let p = pal();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, p.border);
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        let fill = Rect::from_min_max(rect.min, pos2(rect.left() + (rect.width() * f).max(6.0), rect.bottom()));
        ui.painter().rect_filled(fill, 3.0, if color_ok { p.ok } else { p.accent });
    }
}

/// Small rounded tag.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(11.5), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(14.0, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, rect.height() / 2.0, color.gamma_multiply(0.12));
    ui.painter().galley(rect.min + vec2(7.0, 3.0), galley, color);
}
