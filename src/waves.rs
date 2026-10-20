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
    let h = height(x, y, time);
    let step = 8.0;
    let hx = (height(x + step, y, time) - height(x - step, y, time)) / (2.0 * step);
    let hy = (height(x, y + step, time) - height(x, y - step, time)) / (2.0 * step);
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
    let spec = (nx * hx_l / hlen + ny * hy_l / hlen + nz * hz_l / hlen)
        .clamp(0.0, 1.0)
        .powf(160.0)
        * 0.92;
    let hollow = 0.7 + 0.3 * (h * 0.5 + 0.5).clamp(0.0, 1.0);
    let mut tone = (diffuse * hollow * 0.78 + spec).clamp(0.0, 1.0);
    tone = ((tone - 0.42) * 1.2 + 0.42).clamp(0.0, 1.0);
    let level = tone.powf(2.2);

    let dx = (x - center.x) / reach;
    let dy = (y - center.y) / reach;
    let fade = 1.0 - smoothstep(0.5, 1.18, (dx * dx + dy * dy).sqrt());
    let alpha = fade * (0.22 + 0.78 * level) * 0.5;
    let radius = 0.5 * SPACING * MARK * level.sqrt().max(0.14);

    Sample { radius, alpha }
}

fn height(x: f32, y: f32, time: f32) -> f32 {
    let px = (x) / UNIT;
    let py = y / UNIT;
    let heading = (0.9396926_f32, 0.34202015_f32);
    let u = px * heading.0 + py * heading.1;
    let v = px * -heading.1 + py * heading.0;
    let bend = (v * 1.6 + time * 0.2).sin() * 0.4 + (v * 0.85 + time * 0.15).sin() * 0.85;
    let phase = u * FOLDS + bend - time * 0.45;
    let swell = 0.6 + 0.4 * (u * 0.55 + v * 0.45 + time * 0.12).sin();
    let fold = phase.sin() + 0.32 * (phase * 2.0 + 1.3).sin();
    let ripple = 0.16 * (u * FOLDS * 2.5 + bend * 1.9 - time * 0.9 + 2.1).sin();
    (fold + ripple) * swell
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
    use super::height;

    #[test]
    fn silk_moves_and_stays_finite() {
        let a = height(40.0, 80.0, 0.0);
        let b = height(40.0, 80.0, 1.4);
        assert!(a.is_finite() && b.is_finite());
        assert!((a - b).abs() > 0.02);
    }
}
