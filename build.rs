//! Icône et manifeste Windows. Le point de veille est le même que dans la fenêtre.

use std::fs;
use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into());
    let ico_path = Path::new(&out_dir).join("app.ico");
    let bytes = write_ico(&[16, 32, 48, 64]);
    let _ = fs::write(&ico_path, &bytes);

    let installer_icon = Path::new("installer").join("app_icon.ico");
    if fs::read(&installer_icon).ok().as_deref() != Some(bytes.as_slice()) {
        let _ = fs::create_dir_all("installer");
        let _ = fs::write(&installer_icon, &bytes);
    }

    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        if ico_path.exists() {
            res.set_icon(&ico_path.to_string_lossy());
        }
        res.set("ProductName", "Nemuru");
        res.set("FileDescription", "Baisse du volume puis veille de l'ordinateur");
        res.set("LegalCopyright", "Copyright (C) 2026");
        let _ = res.compile();
    }
}

fn render(size: u32) -> Vec<u8> {
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

fn write_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes.iter().copied().map(|s| (s, render(s))).collect();
    let count = images.len() as u16;
    let mut header_len = 6 + 16 * images.len();
    let mut body = Vec::new();
    let mut entries = Vec::new();

    for (size, rgba) in &images {
        let dib = dib_image(*size, rgba);
        let offset = header_len as u32;
        header_len += dib.len();
        entries.push(dir_entry(*size, dib.len() as u32, offset));
        body.extend(dib);
    }

    let mut file = Vec::new();
    file.extend_from_slice(&0u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&count.to_le_bytes());
    for entry in entries {
        file.extend(entry);
    }
    file.extend(body);
    file
}

fn dir_entry(size: u32, bytes: u32, offset: u32) -> Vec<u8> {
    let dim = if size >= 256 { 0u8 } else { size as u8 };
    let mut e = Vec::with_capacity(16);
    e.push(dim);
    e.push(dim);
    e.push(0);
    e.push(0);
    e.extend_from_slice(&1u16.to_le_bytes());
    e.extend_from_slice(&32u16.to_le_bytes());
    e.extend_from_slice(&bytes.to_le_bytes());
    e.extend_from_slice(&offset.to_le_bytes());
    e
}

fn dib_image(size: u32, rgba: &[u8]) -> Vec<u8> {
    let stride = ((size + 31) / 32) * 4;
    let mask_size = stride * size;
    let xor_size = size * size * 4;
    let mut dib = Vec::with_capacity(40 + xor_size as usize + mask_size as usize);

    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(size as i32).to_le_bytes());
    dib.extend_from_slice(&((size as i32) * 2).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&(xor_size + mask_size).to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());

    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            dib.push(rgba[i + 2]);
            dib.push(rgba[i + 1]);
            dib.push(rgba[i]);
            dib.push(rgba[i + 3]);
        }
    }
    dib.extend(std::iter::repeat(0u8).take(mask_size as usize));
    dib
}
