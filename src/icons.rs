//! Vector icons and the VMPC logo.
//!
//! The logo is a macOS "Big Sur" style app icon: a superellipse ("squircle") tile with
//! a diagonal violet→cyan gradient and glossy top light, carrying a play triangle
//! wrapped in a circular "convert" arrow. It is painted as vectors inside the UI and
//! rasterised in software for the window/taskbar icon and exported PNG/ICO files.

use egui::epaint::{Mesh, PathShape};
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Play,
    Pause,
    Prev,
    Next,
    Step,
    Volume,
    Mute,
    Fullscreen,
    Folder,
    Globe,
    Playlist,
    Loop,
    Camera,
    Sun,
    Moon,
    Close,
    Screen,
    Convert,
    Frames,
    Gear,
    Upload,
    Music,
    Image,
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

fn line(p: &Painter, pts: Vec<Pos2>, s: Stroke) {
    p.add(Shape::Path(PathShape::line(pts, s)));
}

fn poly(p: &Painter, pts: Vec<Pos2>, color: Color32) {
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), egui::Vec2::splat(s));
    let w = (s * 0.11).clamp(1.4, 3.2);
    let st = Stroke::new(w, color);
    let at = |x: f32, y: f32| pos2(r.left() + x * s, r.top() + y * s);
    let c = r.center();
    match icon {
        Icon::Play => poly(p, vec![at(0.18, 0.08), at(0.18, 0.92), at(0.9, 0.5)], color),
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(0.18, 0.1), at(0.4, 0.9)), s * 0.06, color);
            p.rect_filled(Rect::from_min_max(at(0.6, 0.1), at(0.82, 0.9)), s * 0.06, color);
        }
        Icon::Prev => {
            p.rect_filled(Rect::from_min_max(at(0.12, 0.15), at(0.25, 0.85)), s * 0.04, color);
            poly(p, vec![at(0.88, 0.15), at(0.88, 0.85), at(0.3, 0.5)], color);
        }
        Icon::Next => {
            p.rect_filled(Rect::from_min_max(at(0.75, 0.15), at(0.88, 0.85)), s * 0.04, color);
            poly(p, vec![at(0.12, 0.15), at(0.12, 0.85), at(0.7, 0.5)], color);
        }
        Icon::Step => {
            poly(p, vec![at(0.1, 0.2), at(0.1, 0.8), at(0.55, 0.5)], color);
            p.rect_filled(Rect::from_min_max(at(0.66, 0.2), at(0.8, 0.8)), s * 0.04, color);
        }
        Icon::Volume | Icon::Mute => {
            poly(p, vec![at(0.08, 0.36), at(0.26, 0.36), at(0.5, 0.14), at(0.5, 0.86), at(0.26, 0.64), at(0.08, 0.64)], color);
            if icon == Icon::Volume {
                line(p, arc(at(0.5, 0.5), s * 0.18, -45.0, 45.0, 10), st);
                line(p, arc(at(0.5, 0.5), s * 0.36, -45.0, 45.0, 14), st);
            } else {
                p.line_segment([at(0.64, 0.34), at(0.94, 0.66)], st);
                p.line_segment([at(0.64, 0.66), at(0.94, 0.34)], st);
            }
        }
        Icon::Fullscreen => {
            for (x, y, dx, dy) in [(0.1, 0.1, 1.0, 1.0), (0.9, 0.1, -1.0, 1.0), (0.1, 0.9, 1.0, -1.0), (0.9, 0.9, -1.0, -1.0)] {
                line(p, vec![at(x, y + 0.28 * dy), at(x, y), at(x + 0.28 * dx, y)], st);
            }
        }
        Icon::Folder => {
            line(
                p,
                vec![at(0.08, 0.84), at(0.08, 0.18), at(0.38, 0.18), at(0.48, 0.3), at(0.92, 0.3), at(0.92, 0.84), at(0.08, 0.84)],
                st,
            );
            p.line_segment([at(0.08, 0.44), at(0.92, 0.44)], st);
        }
        Icon::Globe => {
            p.circle_stroke(c, s * 0.42, st);
            let mut ellipse = arc(c, 1.0, 0.0, 360.0, 32);
            for q in &mut ellipse {
                *q = pos2(c.x + (q.x - c.x) * s * 0.18, c.y + (q.y - c.y) * s * 0.42);
            }
            line(p, ellipse, st);
            p.line_segment([at(0.1, 0.5), at(0.9, 0.5)], st);
        }
        Icon::Playlist => {
            for y in [0.2, 0.42, 0.64] {
                p.line_segment([at(0.08, y), at(0.6, y)], st);
            }
            p.line_segment([at(0.08, 0.86), at(0.42, 0.86)], st);
            poly(p, vec![at(0.62, 0.62), at(0.62, 0.98), at(0.94, 0.8)], color);
        }
        Icon::Loop => {
            let (a0, a1) = (-60.0f32, 230.0f32);
            line(p, arc(c, s * 0.36, a0, a1, 24), st);
            let end = a1.to_radians();
            let tip = c + vec2(end.cos(), end.sin()) * s * 0.36;
            let dir = 1.0;
            let tangent = vec2(-end.sin(), end.cos()) * dir;
            let normal = vec2(end.cos(), end.sin());
            poly(p, vec![tip + tangent * s * 0.2, tip + normal * s * 0.16, tip - normal * s * 0.16], color);
        }
        Icon::Camera => {
            p.rect_stroke(Rect::from_min_max(at(0.06, 0.28), at(0.94, 0.86)), s * 0.1, st);
            line(p, vec![at(0.32, 0.28), at(0.4, 0.14), at(0.6, 0.14), at(0.68, 0.28)], st);
            p.circle_stroke(at(0.5, 0.56), s * 0.16, st);
        }
        Icon::Sun => {
            p.circle_filled(c, s * 0.2, color);
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.32, c + d * s * 0.46], st);
            }
        }
        Icon::Moon => {
            // Crescent = arc of the outer disc outside the "bite" disc + arc of the bite inside the disc.
            let (c2, r1, r2) = (c + vec2(0.22, -0.16) * s, s * 0.4, s * 0.32);
            let ring = |center: Pos2, radius: f32| arc(center, radius, 0.0, 358.0, 179);
            let contiguous = |pts: Vec<Pos2>, keep: &dyn Fn(Pos2) -> bool| -> Vec<Pos2> {
                let n = pts.len();
                let start = (0..n).find(|&i| !keep(pts[(i + n - 1) % n]) && keep(pts[i])).unwrap_or(0);
                (0..n).map(|k| pts[(start + k) % n]).take_while(|q| keep(*q)).collect()
            };
            let mut outline = contiguous(ring(c, r1), &|q| q.distance(c2) >= r2);
            let mut bite = contiguous(ring(c2, r2), &|q| q.distance(c) < r1);
            if let (Some(end), Some(first), Some(last)) = (outline.last(), bite.first(), bite.last()) {
                if end.distance(*last) < end.distance(*first) {
                    bite.reverse();
                }
            }
            outline.extend(bite);
            if let Some(first) = outline.first().copied() {
                outline.push(first);
            }
            line(p, outline, st);
        }
        Icon::Close => {
            p.line_segment([at(0.2, 0.2), at(0.8, 0.8)], st);
            p.line_segment([at(0.8, 0.2), at(0.2, 0.8)], st);
        }
        Icon::Screen => {
            p.rect_stroke(Rect::from_min_max(at(0.06, 0.14), at(0.94, 0.74)), s * 0.08, st);
            p.line_segment([at(0.32, 0.9), at(0.68, 0.9)], st);
            poly(p, vec![at(0.42, 0.3), at(0.42, 0.58), at(0.64, 0.44)], color);
        }
        Icon::Convert => {
            line(p, vec![at(0.1, 0.32), at(0.86, 0.32)], st);
            line(p, vec![at(0.66, 0.14), at(0.88, 0.32), at(0.66, 0.5)], st);
            line(p, vec![at(0.9, 0.68), at(0.14, 0.68)], st);
            line(p, vec![at(0.34, 0.5), at(0.12, 0.68), at(0.34, 0.86)], st);
        }
        Icon::Frames => {
            p.rect_stroke(Rect::from_min_max(at(0.06, 0.12), at(0.94, 0.88)), s * 0.06, st);
            p.line_segment([at(0.36, 0.12), at(0.36, 0.88)], st);
            p.line_segment([at(0.64, 0.12), at(0.64, 0.88)], st);
            for x in [0.21, 0.79] {
                for y in [0.3, 0.5, 0.7] {
                    p.circle_filled(at(x, y), s * 0.045, color);
                }
            }
        }
        Icon::Gear => {
            for i in 0..8 {
                let a = (i as f32 * 45.0 + 22.5).to_radians();
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.28, c + d * s * 0.46], Stroke::new(w * 1.6, color));
            }
            p.circle_stroke(c, s * 0.3, st);
            p.circle_stroke(c, s * 0.12, st);
        }
        Icon::Upload => {
            line(p, vec![at(0.1, 0.62), at(0.1, 0.9), at(0.9, 0.9), at(0.9, 0.62)], st);
            p.line_segment([at(0.5, 0.68), at(0.5, 0.1)], st);
            line(p, vec![at(0.26, 0.32), at(0.5, 0.08), at(0.74, 0.32)], st);
        }
        Icon::Music => {
            p.circle_filled(at(0.28, 0.78), s * 0.15, color);
            p.circle_filled(at(0.76, 0.68), s * 0.15, color);
            line(p, vec![at(0.42, 0.78), at(0.42, 0.14), at(0.9, 0.06), at(0.9, 0.68)], st);
        }
        Icon::Image => {
            p.rect_stroke(Rect::from_min_max(at(0.06, 0.14), at(0.94, 0.86)), s * 0.1, st);
            p.circle_filled(at(0.32, 0.38), s * 0.08, color);
            line(p, vec![at(0.1, 0.8), at(0.4, 0.52), at(0.58, 0.68), at(0.72, 0.56), at(0.92, 0.76)], st);
        }
    }
}

// ---------------------------------------------------------------------------
// Logo
// ---------------------------------------------------------------------------

// VLC orange (top-left) into warm amber (bottom-right).
const LOGO_A: [f32; 3] = [255.0, 110.0, 0.0];
const LOGO_B: [f32; 3] = [255.0, 184.0, 48.0];
const SQUIRCLE_N: f32 = 5.0;
const TILE: f32 = 0.92; // half-size of the tile in normalised (-1..1) logo space
const RING_R: f32 = 0.52;
const RING_W: f32 = 0.075;
const GAP_DEG: f32 = 42.0;

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn base_color(u: f32, v: f32) -> [f32; 3] {
    let t = (((u + 1.0) + (v + 1.0)) / 4.0).clamp(0.0, 1.0);
    let c = mix(LOGO_A, LOGO_B, t);
    let gloss = (0.28 * (1.0 - (v + TILE) / (TILE * 1.2))).clamp(0.0, 0.28);
    mix(c, [255.0; 3], gloss)
}

fn arrow_head() -> [(f32, f32); 3] {
    let end = -GAP_DEG.to_radians();
    let tip_a = (-GAP_DEG + 30.0).to_radians();
    let (r0, r1) = (RING_R - 0.17, RING_R + 0.17);
    [
        (end.cos() * r0, end.sin() * r0),
        (end.cos() * r1, end.sin() * r1),
        (tip_a.cos() * RING_R, tip_a.sin() * RING_R),
    ]
}

const PLAY: [(f32, f32); 3] = [(-0.15, -0.23), (-0.15, 0.23), (0.25, 0.0)];

fn in_triangle(p: (f32, f32), t: [(f32, f32); 3]) -> bool {
    let sign = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| (a.0 - c.0) * (b.1 - c.1) - (b.0 - c.0) * (a.1 - c.1);
    let (d1, d2, d3) = (sign(p, t[0], t[1]), sign(p, t[1], t[2]), sign(p, t[2], t[0]));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Rasterises the logo (RGBA, unpremultiplied) with 4×4 supersampling.
pub fn logo_rgba(size: u32) -> Vec<u8> {
    let mut out = vec![0u8; (size * size * 4) as usize];
    let ss = 4;
    let head = arrow_head();
    for y in 0..size {
        for x in 0..size {
            let (mut acc, mut cover) = ([0.0f32; 3], 0.0f32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let u = ((x as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32) * 2.0 - 1.0;
                    let v = ((y as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32) * 2.0 - 1.0;
                    if (u / TILE).abs().powf(SQUIRCLE_N) + (v / TILE).abs().powf(SQUIRCLE_N) > 1.0 {
                        continue;
                    }
                    let d = (u * u + v * v).sqrt();
                    let ang = v.atan2(u).to_degrees();
                    let on_ring = (d - RING_R).abs() <= RING_W && ang.abs() >= GAP_DEG;
                    let white = on_ring || in_triangle((u, v), head) || in_triangle((u - 0.02, v), PLAY);
                    let c = if white { [255.0; 3] } else { base_color(u, v) };
                    acc = [acc[0] + c[0], acc[1] + c[1], acc[2] + c[2]];
                    cover += 1.0;
                }
            }
            if cover > 0.0 {
                let i = ((y * size + x) * 4) as usize;
                out[i] = (acc[0] / cover) as u8;
                out[i + 1] = (acc[1] / cover) as u8;
                out[i + 2] = (acc[2] / cover) as u8;
                out[i + 3] = (cover / (ss * ss) as f32 * 255.0) as u8;
            }
        }
    }
    out
}


/// Paints the logo with its convert ring (and arrow head) rotated by `ring_deg`.
pub fn paint_logo_ex(p: &Painter, rect: Rect, ring_deg: f32) {
    let s = rect.width().min(rect.height()) / 2.0;
    let c = rect.center();
    let to_screen = |u: f32, v: f32| c + vec2(u, v) * s;
    let color = |u: f32, v: f32| {
        let k = base_color(u, v);
        Color32::from_rgb(k[0] as u8, k[1] as u8, k[2] as u8)
    };
    // Squircle tile with gradient (triangle fan, colour per vertex).
    let n = 96;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, color(0.0, 0.0));
    let mut outline = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let (ct, st) = (t.cos(), t.sin());
        let u = TILE * ct.signum() * ct.abs().powf(2.0 / SQUIRCLE_N);
        let v = TILE * st.signum() * st.abs().powf(2.0 / SQUIRCLE_N);
        mesh.colored_vertex(to_screen(u, v), color(u, v));
        outline.push(to_screen(u, v));
    }
    for i in 0..n as u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n as u32);
    }
    p.add(Shape::mesh(mesh));
    let mut closed = outline.clone();
    closed.push(outline[0]);
    p.add(Shape::Path(PathShape::line(closed, Stroke::new(0.8_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 60)))));

    // Circular convert arrow + play triangle.
    let rot = ring_deg.to_radians();
    let ring: Vec<Pos2> = (0..=48)
        .map(|i| {
            let a = (GAP_DEG + (360.0 - 2.0 * GAP_DEG) * i as f32 / 48.0).to_radians() + rot;
            to_screen(a.cos() * RING_R, a.sin() * RING_R)
        })
        .collect();
    p.add(Shape::Path(PathShape::line(ring, Stroke::new(RING_W * 2.0 * s, Color32::WHITE))));
    let (sr, cr) = rot.sin_cos();
    let head = arrow_head().map(|(u, v)| to_screen(u * cr - v * sr, u * sr + v * cr));
    p.add(Shape::convex_polygon(head.to_vec(), Color32::WHITE, Stroke::NONE));
    let play = PLAY.map(|(u, v)| to_screen(u + 0.02, v));
    p.add(Shape::convex_polygon(play.to_vec(), Color32::WHITE, Stroke::NONE));
}

// ---------------------------------------------------------------------------
// Minimal PNG / ICO writers (uncompressed deflate) for exporting the app icon.
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for row in rgba.chunks_exact((w * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    png
}

/// 24-bit BMP of the logo on a white background (installer wizard artwork).
pub fn encode_bmp(size: u32) -> Vec<u8> {
    let rgba = logo_rgba(size);
    let row = (size * 3).div_ceil(4) * 4;
    let data_len = row * size;
    let mut bmp = Vec::with_capacity((54 + data_len) as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(54 + data_len).to_le_bytes());
    bmp.extend_from_slice(&[0, 0, 0, 0]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(size as i32).to_le_bytes());
    bmp.extend_from_slice(&(size as i32).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&data_len.to_le_bytes());
    bmp.extend_from_slice(&[0; 16]);
    for y in (0..size).rev() {
        let start = bmp.len();
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let a = rgba[i + 3] as u32;
            let over_white = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
            bmp.extend_from_slice(&[over_white(rgba[i + 2]), over_white(rgba[i + 1]), over_white(rgba[i])]);
        }
        bmp.resize(start + row as usize, 0);
    }
    bmp
}

/// ICO containing PNG images (supported since Windows Vista).
pub fn encode_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes.iter().map(|&s| (s, encode_png(s, s, &logo_rgba(s)))).collect();
    let mut ico = vec![0, 0, 1, 0];
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (s, data) in &images {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in images {
        ico.extend_from_slice(&data);
    }
    ico
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_reference() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn logo_is_opaque_in_center_and_clear_in_corner() {
        let s = 64;
        let img = logo_rgba(s);
        assert_eq!(img[((s / 2 * s + s / 2) * 4 + 3) as usize], 255);
        assert_eq!(img[3], 0);
    }
}
