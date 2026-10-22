//! Fond de la fenêtre : trame de points sur des plis de soie.
//! Le motif s'efface vers les bords, le texte reste au premier plan.

use eframe::egui;

const SPACING: f32 = 9.0;
const UNIT: f32 = 520.0;
const FOLDS: f32 = 5.5;
const MARK: f32 = 0.95;

const PAPER: [u8; 3] = [22, 20, 18];
const LIT: [u8; 3] = [214, 186, 148];

pub fn paint(painter: &egui::Painter, rect: egui::Rect, time: f32) {
    if rect.width() < SPACING || rect.height() < SPACING {
        return;
    }
    let cols = (rect.width() / SPACING).ceil().max(1.0);
    let rows = (rect.height() / SPACING).ceil().max(1.0);
    let origin_x = rect.left() + (rect.width() - cols * SPACING) * 0.5 + SPACING * 0.5;
    let origin_y = rect.top() + (rect.height() - rows * SPACING) * 0.5 + SPACING * 0.5;
    let center = egui::pos2(rect.width() * 0.5, rect.height() * 0.5);
    let reach = (rect.width() * rect.width() + rect.height() * rect.height()).sqrt().max(1.0) * 0.5;

    let mut shapes = Vec::with_capacity((cols * rows) as usize);
    let mut row = 0.0;
    while row < rows {
        let y = origin_y + row * SPACING;
        let mut col = 0.0;
        while col < cols {
            let x = origin_x + col * SPACING;
            let sample = shade(x - rect.left(), y - rect.top(), center, reach, time);
            if sample.alpha > 0.04 {
                shapes.push(egui::Shape::circle_filled(
                    egui::pos2(x, y),
                    sample.radius,
                    mix(PAPER, LIT, sample.alpha),
                ));
            }
            col += 1.0;
        }
        row += 1.0;
    }
    painter.extend(shapes);
}

struct Sample {
    radius: f32,
    alpha: f32,
}

fn shade(x: f32, y: f32, center: egui::Pos2, reach: f32, time: f32) -> Sample {
    // Les coins s'effacent : inutile d'y calculer la soie.
    let dx = (x - center.x) / reach;
    let dy = (y - center.y) / reach;
    let fade = 1.0 - smoothstep(0.5, 1.18, (dx * dx + dy * dy).sqrt());
    if fade * 0.5 <= 0.04 {
        return Sample { radius: 0.0, alpha: 0.0 };
    }

    let (h, hx, hy) = surface(x, y, time);
    let gx = hx * UNIT * 0.95 * 0.4;
    let gy = hy * UNIT * 0.95 * 0.4;
    let len = (gx * gx + gy * gy + 1.0).sqrt();
    let nx = -gx / len;
    let ny = -gy / len;
    let nz = 1.0 / len;

    let light = [-0.733, -0.267, 0.62];
    let diffuse = (nx * light[0] + ny * light[1] + nz * light[2]).clamp(0.0, 1.0);
    let hx_l = light[0];
    let hy_l = light[1];
    let hz_l = light[2] + 1.0;
    let hlen = (hx_l * hx_l + hy_l * hy_l + hz_l * hz_l).sqrt();
    let spec = pow160((nx * hx_l / hlen + ny * hy_l / hlen + nz * hz_l / hlen).clamp(0.0, 1.0)) * 0.92;
    let hollow = 0.7 + 0.3 * (h * 0.5 + 0.5).clamp(0.0, 1.0);
    let mut tone = (diffuse * hollow * 0.78 + spec).clamp(0.0, 1.0);
    tone = ((tone - 0.42) * 1.2 + 0.42).clamp(0.0, 1.0);
    let level = tone.powf(2.2);

    let alpha = fade * (0.22 + 0.78 * level) * 0.5;
    let radius = 0.5 * SPACING * MARK * level.sqrt().max(0.14);

    Sample { radius, alpha }
}

/// x^160 par élévations au carré, bien moins cher que `powf`.
fn pow160(x: f32) -> f32 {
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    let x16 = x8 * x8;
    let x32 = x16 * x16;
    let x64 = x32 * x32;
    let x128 = x64 * x64;
    x128 * x32
}

/// Hauteur de la soie et sa pente exacte (d/dx, d/dy), en une seule évaluation.
/// Avant, la pente venait de quatre hauteurs voisines : cinq fois plus de sinus.
fn surface(x: f32, y: f32, time: f32) -> (f32, f32, f32) {
    let px = x / UNIT;
    let py = y / UNIT;
    let (a, b) = (0.9396926_f32, 0.34202015_f32);
    let u = px * a + py * b;
    let v = px * -b + py * a;

    let (b1s, b1c) = (v * 1.6 + time * 0.2).sin_cos();
    let (b2s, b2c) = (v * 0.85 + time * 0.15).sin_cos();
    let bend = b1s * 0.4 + b2s * 0.85;
    let bend_v = 0.4 * 1.6 * b1c + 0.85 * 0.85 * b2c;

    let phase = u * FOLDS + bend - time * 0.45;
    let (ss, sc) = (u * 0.55 + v * 0.45 + time * 0.12).sin_cos();
    let swell = 0.6 + 0.4 * ss;
    let (swell_u, swell_v) = (0.4 * 0.55 * sc, 0.4 * 0.45 * sc);

    let (p1s, p1c) = phase.sin_cos();
    let (p2s, p2c) = (phase * 2.0 + 1.3).sin_cos();
    let fold = p1s + 0.32 * p2s;
    let fold_phase = p1c + 0.64 * p2c;

    let (rs, rc) = (u * FOLDS * 2.5 + bend * 1.9 - time * 0.9 + 2.1).sin_cos();
    let ripple = 0.16 * rs;
    let (ripple_u, ripple_v) = (0.16 * rc * FOLDS * 2.5, 0.16 * rc * 1.9 * bend_v);

    let wave = fold + ripple;
    let h_u = (fold_phase * FOLDS + ripple_u) * swell + wave * swell_u;
    let h_v = (fold_phase * bend_v + ripple_v) * swell + wave * swell_v;
    let h_x = (h_u * a - h_v * b) / UNIT;
    let h_y = (h_u * b + h_v * a) / UNIT;
    (wave * swell, h_x, h_y)
}

#[cfg(test)]
fn height(x: f32, y: f32, time: f32) -> f32 {
    surface(x, y, time).0
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(from: [u8; 3], to: [u8; 3], t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (from[0] as f32 + (to[0] as f32 - from[0] as f32) * t) as u8,
        (from[1] as f32 + (to[1] as f32 - from[1] as f32) * t) as u8,
        (from[2] as f32 + (to[2] as f32 - from[2] as f32) * t) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{height, pow160, surface};

    #[test]
    fn exact_slope_matches_the_old_neighbour_estimate() {
        let step = 8.0;
        for &(x, y, t) in &[(40.0, 80.0, 0.0), (310.0, 620.0, 3.7), (900.0, 15.0, 12.4), (5.0, 1400.0, 40.0)] {
            let (_, hx, hy) = surface(x, y, t);
            let fx = (height(x + step, y, t) - height(x - step, y, t)) / (2.0 * step);
            let fy = (height(x, y + step, t) - height(x, y - step, t)) / (2.0 * step);
            let scale = fx.abs().max(fy.abs()).max(1e-3);
            assert!((hx - fx).abs() / scale < 0.03, "x={x} y={y} t={t}: {hx} vs {fx}");
            assert!((hy - fy).abs() / scale < 0.03, "x={x} y={y} t={t}: {hy} vs {fy}");
        }
    }

    #[test]
    fn pow160_matches_powf() {
        for &x in &[0.0_f32, 0.5, 0.97, 0.995, 1.0] {
            assert!((pow160(x) - x.powf(160.0)).abs() < 1e-4, "x={x}");
        }
    }

    #[test]
    fn silk_moves_and_stays_finite() {
        let a = height(40.0, 80.0, 0.0);
        let b = height(40.0, 80.0, 1.4);
        assert!(a.is_finite() && b.is_finite());
        assert!((a - b).abs() > 0.02);
    }
}
