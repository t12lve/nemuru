//! Marque de la fenêtre : un point de veille sur fond chaud. Pas un pictogramme.

pub fn rgba(size: u32) -> Vec<u8> {
    render(size)
}

pub fn render(size: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 * 0.18;
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let cover = (radius + 0.5 - distance).clamp(0.0, 1.0);
            buf[i] = mix(22, 196, cover);
            buf[i + 1] = mix(20, 154, cover);
            buf[i + 2] = mix(18, 92, cover);
            buf[i + 3] = 255;
        }
    }
    buf
}

fn mix(from: u8, to: u8, t: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * t).round() as u8
}
