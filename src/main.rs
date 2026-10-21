//! Nemuru — baisse le volume, puis met l'ordinateur en veille.
//! La fenêtre s'ouvre sans rien lancer. Le compte à rebours attend « Commencer ».

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod atv;
mod audio;
mod config;
mod fade;
mod google_tv;
mod instance;
mod mark;
mod power;
mod session;
mod startup;
mod ui;
mod update;
mod waves;

use std::fs::OpenOptions;
use std::io::{self, Write};

fn main() {
    enable_dpi();
    if !instance::acquire() {
        return;
    }
    init_log();
    log::info!("Nemuru {}", env!("CARGO_PKG_VERSION"));
    startup::migrate_legacy();
    if let Err(err) = ui::run() {
        log::error!("fenêtre : {err}");
    }
}

fn enable_dpi() {
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn init_log() {
    let path = config::AppConfig::log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 200_000 {
            let _ = std::fs::rename(&path, path.with_extension("log.old"));
        }
    }
    let target = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map(|file| env_logger::Target::Pipe(Box::new(FileLog(file))))
        .unwrap_or_else(|_| env_logger::Target::Stderr);
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(target)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} {} {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .try_init();
}

struct FileLog(std::fs::File);

impl Write for FileLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
