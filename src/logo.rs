//! Logo en tête de fenêtre, peint en lumière de fin de journée.
//! Pendant le compte, il sert de barre de progression : la lumière dorée gagne
//! les lettres de gauche à droite, le reste attend dans l'ombre.

use eframe::egui;

const LOGO_PNG: &[u8] = include_bytes!("../logonemuru.png");

const AMBER: [f32; 3] = [246.0, 160.0, 70.0];
const GOLD: [f32; 3] = [255.0, 204.0, 112.0];
const ROSE: [f32; 3] = [238.0, 138.0, 122.0];
const PEACH: [f32; 3] = [255.0, 182.0, 138.0];
const PALE: [f32; 3] = [255.0, 236.0, 192.0];
const NIGHT: [f32; 3] = [70.0, 62.0, 58.0];

/// Le dessin du logo en niveaux de gris : la couleur est posée à la peinture.
pub struct Logo {
    texture: egui::TextureHandle,
    /// Hauteur des lettres, de 0 (haut de l'image) à 1 (bas), sans les étoiles.
    band: (f32, f32),
    /// Largeur des lettres, de 0 (gauche) à 1 (droite), sans l'ombre portée.
    span: (f32, f32),
}

impl Logo {
    pub fn load(ctx: &egui::Context) -> Option<Self> {
        let rgba = image::load_from_memory(LOGO_PNG)
            .map_err(|err| log::warn!("logo illisible : {err}"))
            .ok()?
            .into_rgba8();
        let (width, height) = rgba.dimensions();
        let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
        for (x, y, pixel) in rgba.enumerate_pixels() {
            if pixel[3] > 0 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
        if right <= left || bottom <= top {
            return None;
        }
        let cropped = image::imageops::crop_imm(&rgba, left, top, right - left, bottom - top).to_image();
        let (w, h) = cropped.dimensions();

        // Masque : la luminance du dessin, ramenée à 1 sur la partie la plus claire.
        // L'ombre portée, presque noire, reste noire sous n'importe quelle couleur.
        let luma = |p: &image::Rgba<u8>| (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        let brightest = cropped
            .pixels()
            .filter(|p| p[3] > 200)
            .map(luma)
            .fold(0.0_f32, f32::max)
            .max(0.01);
        let mut mask = Vec::with_capacity((w * h * 4) as usize);
        let mut glyph = vec![false; (w * h) as usize];
        let mut glyph_rows = Vec::new();
        for y in 0..h {
            let mut lit = 0;
            for x in 0..w {
                let pixel = cropped.get_pixel(x, y);
                let level = (luma(pixel) / brightest).clamp(0.0, 1.0);
                if pixel[3] > 200 && level > 0.5 {
                    lit += 1;
                    glyph[(y * w + x) as usize] = true;
                }
                let gray = (level * 255.0).round() as u8;
                mask.extend_from_slice(&[gray, gray, gray, pixel[3]]);
            }
            // Les étoiles couvrent au plus 4 % d'une ligne, les jambages du N déjà 10 %.
            if lit as f32 > w as f32 * 0.06 {
                glyph_rows.push(y);
            }
        }
        let (first_row, last_row) = match (glyph_rows.first(), glyph_rows.last()) {
            (Some(&first), Some(&last)) => (first, last),
            _ => (0, h - 1),
        };
        // Colonnes où passent les lettres, pour que la barre parte du N et finisse au dernier u.
        let glyph_cols: Vec<u32> = (0..w)
            .filter(|&x| (first_row..=last_row).any(|y| glyph[(y * w + x) as usize]))
            .collect();
        let span = match (glyph_cols.first(), glyph_cols.last()) {
            (Some(&first), Some(&last)) => (first as f32 / w as f32, (last + 1) as f32 / w as f32),
            _ => (0.0, 1.0),
        };
        let band = (first_row as f32 / h as f32, (last_row + 1) as f32 / h as f32);
        let pixels = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &mask);
        // Mipmaps : le logo est réduit plusieurs fois, sans eux les bords scintillent.
        let options = egui::TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification: egui::TextureFilter::Linear,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
            mipmap_mode: Some(egui::TextureFilter::Linear),
        };
        Some(Self {
            texture: ctx.load_texture("logo", pixels, options),
            band,
            span,
        })
    }

    pub fn aspect(&self) -> f32 {
        let size = self.texture.size_vec2();
        size.y / size.x.max(1.0)
    }

    /// `progress` : part du compte écoulée, de 0 (tout dans l'ombre) à 1 (tout éclairé).
    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect, time: f32, progress: f32, hover: f32) {
        // Colonnes fines : la limite de lumière avance sans à-coups.
        const COLS: usize = 120;
        const ROWS: usize = 24;
        let line = fill_line(progress, self.span);
        let mut mesh = egui::Mesh::with_texture(self.texture.id());
        for row in 0..=ROWS {
            let v = row as f32 / ROWS as f32;
            for col in 0..=COLS {
                let u = col as f32 / COLS as f32;
                let pos = rect.min + egui::vec2(u * rect.width(), v * rect.height());
                let color = color_at(u, v, time, line, progress, self.band, hover);
                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv: egui::pos2(u, v),
                    color,
                });
            }
        }
        for row in 0..ROWS {
            for col in 0..COLS {
                let i = (row * (COLS + 1) + col) as u32;
                let below = i + COLS as u32 + 1;
                mesh.add_triangle(i, i + 1, below);
                mesh.add_triangle(i + 1, below + 1, below);
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

const FEATHER: f32 = 0.012;

/// Abscisse de la limite : éclairé à gauche, dans l'ombre à droite.
fn fill_line(progress: f32, span: (f32, f32)) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    let (start, end) = (span.0 - FEATHER, span.1 + FEATHER);
    start + progress * (end - start)
}

fn color_at(u: f32, v: f32, time: f32, line: f32, progress: f32, band: (f32, f32), hover: f32) -> egui::Color32 {
    // Au-dessus des lettres : les étoiles, qui scintillent doucement.
    if v < band.0 - 0.05 {
        let twinkle = 0.78 + 0.22 * (time * 1.9 + u * 23.0).sin();
        return rgb(scale(mix(PALE, GOLD, 0.25), twinkle + hover * 0.1));
    }
    let lit = 1.0 - smoothstep(line - FEATHER, line + FEATHER, u);
    let day = mix(golden_hour(u, v, time, band), PALE, hover * 0.18);
    let mut color = mix(NIGHT, day, lit);
    // Pendant le compte, un liseré clair marque le front de la lumière.
    if progress > 0.0 && progress < 1.0 {
        let rim = (-((u - line) / 0.014).powi(2)).exp();
        color = mix(color, PALE, rim * 0.6);
    }
    rgb(color)
}

/// Dégradé chaud qui glisse lentement de gauche à droite, avec un reflet qui passe.
fn golden_hour(u: f32, v: f32, time: f32, band: (f32, f32)) -> [f32; 3] {
    const STOPS: [[f32; 3]; 4] = [AMBER, GOLD, PEACH, ROSE];
    let drift = (u * 0.9 - time * 0.05).rem_euclid(1.0) * STOPS.len() as f32;
    let index = drift.floor() as usize % STOPS.len();
    let next = (index + 1) % STOPS.len();
    let t = drift.fract();
    let mut color = mix(STOPS[index], STOPS[next], t * t * (3.0 - 2.0 * t));

    // Le haut des lettres prend la lumière du ciel.
    let height = ((v - band.0) / (band.1 - band.0).max(0.01)).clamp(0.0, 1.0);
    color = mix(color, PALE, (1.0 - height) * 0.35);

    // Un reflet traverse le mot toutes les dix secondes environ.
    let sweep = (time * 0.1).rem_euclid(1.0) * 1.8 - 0.4;
    let distance = u + (v - 0.5) * 0.3 - sweep;
    let sheen = (-(distance / 0.07).powi(2)).exp();
    mix(color, [255.0, 246.0, 222.0], sheen * 0.5)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn rgb(c: [f32; 3]) -> egui::Color32 {
    let channel = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgb(channel(c[0]), channel(c[1]), channel(c[2]))
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::{color_at, fill_line};
    use eframe::egui;

    const BAND: (f32, f32) = (0.35, 0.92);
    const SPAN: (f32, f32) = (0.02, 0.96);

    fn letter_at(u: f32, progress: f32) -> egui::Color32 {
        color_at(u, 0.64, 3.0, fill_line(progress, SPAN), progress, BAND, 0.0)
    }

    #[test]
    fn empty_at_start_and_full_at_the_end() {
        assert!(fill_line(0.0, SPAN) < SPAN.0, "début : la limite est avant le N");
        assert!(fill_line(1.0, SPAN) > SPAN.1, "fin : la limite est après le dernier u");
        for u in [SPAN.0, 0.5, SPAN.1 - 0.01] {
            assert!(letter_at(u, 0.0).r() < 90, "u={u}");
            assert!(letter_at(u, 1.0).r() > 200, "u={u}");
        }
    }

    #[test]
    fn light_fills_from_left_to_right() {
        let line = fill_line(0.5, SPAN);
        let left = letter_at(line - 0.1, 0.5);
        let right = letter_at(line + 0.1, 0.5);
        assert!(left.r() > 200 && right.r() < 90, "{left:?} / {right:?}");
    }

    #[test]
    fn lit_letters_stay_warm_at_every_moment() {
        for step in 0..60 {
            let time = step as f32 * 0.7;
            for u in [0.0, 0.2, 0.45, 0.7, 1.0] {
                let c = color_at(u, 0.7, time, fill_line(1.0, SPAN), 1.0, BAND, 0.0);
                assert!(c.r() >= c.g() && c.g() >= c.b(), "t={time} u={u} {c:?}");
            }
        }
    }
}
