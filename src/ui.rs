/* Hallmark · pre-emit critique: P5 H5 E4 S5 R5 V4
 * genre: editorial · macrostructure: bedside clock
 * tone: austere · anchor: brass on warm black
 * audience: someone about to sleep · use: start the fade · tone: soft, quiet
 */

//! Fenêtre. Le temps restant est le seul élément grand.
//! Le laiton n'apparaît que sur l'action principale.

use crate::atv::{self, PairEvent, PairSession, PowerNotice};
use crate::config::AppConfig;
use crate::fade::FadeCurve;
use crate::google_tv::{self, CastDevice};
use crate::mark;
use crate::pair_keys;
use crate::pair_page::PairPage;
use crate::power::SleepAction;
use crate::session::{Phase, Session};
use crate::update;
use chrono::{Local, Timelike};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

const PAPER: egui::Color32 = egui::Color32::from_rgb(22, 20, 18);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(54, 48, 42);
const SURFACE_HOVER: egui::Color32 = egui::Color32::from_rgb(72, 64, 56);
const WELL: egui::Color32 = egui::Color32::from_rgb(12, 11, 10);
const INK: egui::Color32 = egui::Color32::from_rgb(236, 230, 220);
const INK_DIM: egui::Color32 = egui::Color32::from_rgb(176, 166, 152);
const BRASS: egui::Color32 = egui::Color32::from_rgb(196, 154, 92);
const BRASS_HOVER: egui::Color32 = egui::Color32::from_rgb(210, 172, 112);
const BRASS_PRESS: egui::Color32 = egui::Color32::from_rgb(168, 128, 70);
const ON_BRASS: egui::Color32 = egui::Color32::from_rgb(22, 20, 18);

const PRESETS: [u64; 5] = [15, 30, 45, 60, 90];

pub fn run() -> eframe::Result {
    let rgba = mark::rgba(32);
    let icon = egui::IconData {
        rgba,
        width: 32,
        height: 32,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Nemuru")
            .with_inner_size([500.0, 900.0])
            .with_min_inner_size([440.0, 640.0])
            .with_icon(icon),
        persistence_path: None,
        ..Default::default()
    };
    eframe::run_native(
        "Nemuru",
        options,
        Box::new(|cc| Ok(Box::new(SleepApp::new(cc)))),
    )
}

struct Tray {
    icon: tray_icon::TrayIcon,
    cancel: tray_icon::menu::MenuItem,
    open_id: tray_icon::menu::MenuId,
    cancel_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    tip: String,
}

#[derive(Clone, Copy)]
enum TvJob {
    On,
    Off,
    Learn,
}

struct TvReply {
    mac: String,
    notice: Option<Result<PowerNotice, String>>,
}

struct SleepApp {
    session: Session,
    display: egui::FontFamily,
    minutes: String,
    late: String,
    curve: FadeCurve,
    action: SleepAction,
    tv_enabled: bool,
    ip: String,
    mac: String,
    learn_tried: bool,
    port: u16,
    tv_job: Option<TvJob>,
    tv_rx: Option<Receiver<TvReply>>,
    startup: bool,
    error: String,
    devices: Vec<CastDevice>,
    searching: bool,
    discover_rx: Option<Receiver<Result<Vec<CastDevice>, String>>>,
    discover_open: bool,
    discover_note: String,
    about_open: bool,
    tray: Option<Tray>,
    window_shown: bool,
    tucked: bool,
    update_rx: Option<Receiver<update::Ready>>,
    pending_update: Option<update::Ready>,
    update_note: String,
    tv_paired: bool,
    pairing: Option<PairSession>,
    pair_page: Option<PairPage>,
    pair_code: String,
    pair_listening: bool,
    held_code: Option<String>,
    logo: Option<crate::logo::Logo>,
}

impl SleepApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let display = install_fonts(&cc.egui_ctx);
        apply_look(&cc.egui_ctx);
        let mut config = AppConfig::load_or_default().unwrap_or_default();
        config.run_at_startup = crate::startup::is_enabled();
        let tray = Tray::install().map_err(|err| log::warn!("barre des tâches : {err}")).ok();
        Self {
            session: Session::new(),
            display,
            minutes: config.duration_minutes.to_string(),
            late: config.late_fade_minutes.to_string(),
            curve: config.fade_curve,
            action: config.sleep_action,
            tv_enabled: config.google_tv_enabled,
            ip: config.google_tv_ip,
            mac: config.google_tv_mac,
            learn_tried: false,
            port: config.google_tv_port,
            tv_job: None,
            tv_rx: None,
            startup: config.run_at_startup,
            error: String::new(),
            devices: Vec::new(),
            searching: false,
            discover_rx: None,
            discover_open: false,
            discover_note: String::new(),
            about_open: false,
            tray,
            window_shown: true,
            tucked: false,
            update_rx: if cfg!(debug_assertions) {
                None
            } else {
                Some(update::check_in_background())
            },
            pending_update: None,
            update_note: String::new(),
            tv_paired: atv::is_paired(),
            pairing: None,
            pair_page: None,
            pair_code: String::new(),
            pair_listening: false,
            held_code: None,
            logo: crate::logo::Logo::load(&cc.egui_ctx),
        }
    }

    fn parsed_minutes(&self) -> Option<u64> {
        let minutes: u64 = self.minutes.trim().parse().ok()?;
        (1..=480).contains(&minutes).then_some(minutes)
    }

    fn config_from_form(&self) -> AppConfig {
        let duration_minutes = self.parsed_minutes().unwrap_or(45);
        let late = self
            .late
            .trim()
            .parse::<u64>()
            .unwrap_or(10)
            .clamp(1, duration_minutes);
        AppConfig {
            duration_minutes,
            fade_curve: self.curve,
            sleep_action: self.action,
            google_tv_enabled: self.tv_enabled,
            google_tv_ip: self.ip.trim().to_string(),
            google_tv_port: self.port,
            google_tv_mac: self.mac.clone(),
            late_fade_minutes: late,
            run_at_startup: self.startup,
        }
    }

    fn persist(&mut self) {
        if let Err(err) = self.config_from_form().save() {
            self.error = format!("Réglage non enregistré ({err}).");
        }
    }

    fn begin_search(&mut self) {
        if self.searching {
            return;
        }
        self.searching = true;
        self.devices.clear();
        self.error.clear();
        let (tx, rx) = mpsc::channel();
        self.discover_rx = Some(rx);
        std::thread::Builder::new()
            .name("recherche-tv".into())
            .spawn(move || {
                let result = google_tv::discover(Duration::from_secs(3)).map_err(|err| err.to_string());
                let _ = tx.send(result);
            })
            .ok();
    }

    fn poll_discover(&mut self) {
        let ready = self.discover_rx.as_ref().map(|rx| rx.try_recv());
        match ready {
            Some(Ok(Ok(list))) => {
                self.discover_rx = None;
                self.searching = false;
                self.devices = one_per_name(list);
                self.discover_note = if self.devices.is_empty() {
                    "Aucune Google TV trouvée sur ce réseau.".into()
                } else {
                    String::new()
                };
            }
            Some(Ok(Err(err))) => {
                self.discover_rx = None;
                self.searching = false;
                self.discover_note = format!("Recherche impossible ({err}).");
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.discover_rx = None;
                self.searching = false;
            }
            _ => {}
        }
    }

    fn begin_tv(&mut self, job: TvJob) {
        if self.tv_rx.is_some() {
            return;
        }
        let ip = self.ip.trim().to_string();
        if ip.is_empty() {
            self.error = "Indiquez l'adresse de la télé.".into();
            return;
        }
        let mac = self.mac.clone();
        let (tx, rx) = mpsc::channel();
        self.tv_job = Some(job);
        self.tv_rx = Some(rx);
        self.error.clear();
        std::thread::Builder::new()
            .name("télé".into())
            .spawn(move || {
                let learned = atv::hardware_address(&ip).unwrap_or(mac.clone());
                let notice = match job {
                    TvJob::Learn => None,
                    TvJob::Off => Some(atv::power_off(&ip).map(|()| PowerNotice::Sleeping).map_err(|err| err.to_string())),
                    TvJob::On => Some(atv::power_on(&ip, &learned).map_err(|err| err.to_string())),
                };
                let _ = tx.send(TvReply { mac: learned, notice });
            })
            .ok();
    }

    fn poll_tv(&mut self) {
        if self.tv_rx.is_none()
            && !self.learn_tried
            && self.mac.is_empty()
            && self.ip.parse::<std::net::Ipv4Addr>().is_ok()
        {
            self.learn_tried = true;
            self.begin_tv(TvJob::Learn);
        }
        let ready = self.tv_rx.as_ref().map(|rx| rx.try_recv());
        let Some(ready) = ready else {
            return;
        };
        match ready {
            Ok(reply) => {
                self.tv_rx = None;
                self.tv_job = None;
                if !reply.mac.is_empty() && reply.mac != self.mac {
                    self.mac = reply.mac;
                    self.persist();
                }
                if let Some(notice) = reply.notice {
                    self.error = match notice {
                        Ok(PowerNotice::Waking) => "La télé s'allume.".into(),
                        Ok(PowerNotice::AlreadyOn) => "La télé est déjà allumée.".into(),
                        Ok(PowerNotice::WakePacket) => "Signal de réveil envoyé.".into(),
                        Ok(PowerNotice::Sleeping) => "La télé s'éteint.".into(),
                        Err(err) => err,
                    };
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.tv_rx = None;
                self.tv_job = None;
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn stop_pairing_keys(&mut self) {
        if self.pair_listening {
            pair_keys::disarm();
            self.pair_listening = false;
        }
    }

    fn end_pairing(&mut self) {
        self.stop_pairing_keys();
        self.pairing = None;
        self.pair_page = None;
        self.held_code = None;
        self.pair_code.clear();
    }

    fn take_phone_code(&mut self) {
        let Some(code) = self.pair_page.as_ref().and_then(|page| page.poll_code()) else {
            return;
        };
        self.pair_code = code.clone();
        if let Some(session) = &self.pairing {
            let _ = session.key_sender().send(code);
        } else {
            self.held_code = Some(code);
        }
    }

    fn start_handshake(&mut self) {
        if self.pairing.is_some() {
            return;
        }
        match atv::start_pairing(&self.ip) {
            Ok(session) => {
                if let Some(code) = self.held_code.take() {
                    let _ = session.key_sender().send(code);
                }
                self.pairing = Some(session);
                self.error.clear();
            }
            Err(err) => self.error = err.to_string(),
        }
    }

    fn poll_pairing(&mut self) {
        self.take_phone_code();
        if self.pairing.is_none() {
            self.stop_pairing_keys();
            return;
        }
        let event = self.pairing.as_ref().and_then(|session| session.poll());
        match event {
            Some(PairEvent::WaitingForCode) => {
                if !self.pair_listening {
                    let sender = self.pairing.as_ref().map(|session| session.key_sender());
                    if let Some(sender) = sender {
                        if !pair_keys::arm(sender) {
                            self.error = "La saisie du code sans voir la fenêtre est impossible.".into();
                        }
                    }
                    self.pair_listening = true;
                }
            }
            Some(PairEvent::Paired) => {
                self.end_pairing();
                self.tv_paired = true;
                self.error.clear();
            }
            Some(PairEvent::Failed(err)) => {
                self.end_pairing();
                self.error = err;
            }
            None => {}
        }
        if self.pair_listening {
            let typed = pair_keys::snapshot();
            if !typed.is_empty() {
                self.pair_code = typed;
            }
        }
    }

    fn poll_update(&mut self) {
        if let Some(rx) = &self.update_rx {
            match rx.try_recv() {
                Ok(ready) => {
                    self.pending_update = Some(ready);
                    self.update_rx = None;
                }
                Err(TryRecvError::Disconnected) => self.update_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(ready) = self.pending_update.take() else {
            return;
        };
        if self.session.view().phase != Phase::Idle {
            self.pending_update = Some(ready);
            return;
        }
        if self.update_note.is_empty() {
            self.update_note = format!("Mise à jour {}.", ready.version);
            self.pending_update = Some(ready);
            return;
        }
        self.persist();
        self.session.shutdown();
        if let Err(err) = update::install(&ready) {
            self.update_note.clear();
            self.error = format!("Mise à jour impossible ({err}).");
            return;
        }
        std::process::exit(0);
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else {
            return;
        };
        let open_id = tray.open_id.clone();
        let cancel_id = tray.cancel_id.clone();
        let quit_id = tray.quit_id.clone();
        let mut open = false;
        let mut cancel = false;
        let mut quit = false;
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == open_id {
                open = true;
            } else if event.id == cancel_id {
                cancel = true;
            } else if event.id == quit_id {
                quit = true;
            }
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                open = true;
            }
        }
        if open {
            self.window_shown = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if cancel {
            self.session.cancel();
        }
        if quit {
            self.session.shutdown();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn sync_tray(&mut self) {
        let Some(tray) = &mut self.tray else {
            return;
        };
        let view = self.session.view();
        let tip = match view.phase {
            Phase::Running | Phase::Finishing => format!("Nemuru {}", format_clock(view.remaining)),
            _ => "Nemuru".into(),
        };
        if tray.tip != tip {
            let _ = tray.icon.set_tooltip(Some(&tip));
            tray.tip = tip;
        }
        tray.cancel
            .set_enabled(view.cancellable && !self.session.cancel_requested());
    }

    fn try_start(&mut self) {
        let Some(minutes) = self.parsed_minutes() else {
            self.error = "Indiquez une durée entre 1 et 480 minutes.".into();
            return;
        };
        self.minutes = minutes.to_string();
        if self.tv_enabled {
            let ip = self.ip.trim();
            if ip.is_empty() {
                self.error = "Indiquez l'adresse de la télé, ou laissez l'extinction décochée.".into();
                return;
            }
            if ip.parse::<std::net::Ipv4Addr>().is_err() && ip.parse::<std::net::Ipv6Addr>().is_err() {
                self.error = "Cette adresse est invalide.".into();
                return;
            }
        }
        self.error.clear();
        let config = self.config_from_form();
        if let Err(err) = config.save() {
            self.error = format!("Réglage non enregistré ({err}).");
        }
        self.session.start(config);
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        let view = self.session.view();
        let active = matches!(view.phase, Phase::Running | Phase::Finishing);
        let muted = view.phase == Phase::Muted;

        ui.add_space(4.0);

        let clock_text = if active || view.phase == Phase::Finishing {
            format_clock(view.remaining)
        } else if let Some(minutes) = self.parsed_minutes() {
            // L'horloge défile jusqu'à la nouvelle durée au lieu de sauter.
            let shown = ui
                .ctx()
                .animate_value_with_time(egui::Id::new("clock-minutes"), minutes as f32, 0.28);
            format!("{}:00", shown.round() as u64)
        } else {
            "--:--".into()
        };
        clock(ui, &clock_text, self.display.clone());
        ui.add_space(6.0);

        let minutes = self.parsed_minutes().unwrap_or(0);
        if minutes > 0 {
            let end = if active {
                Local::now()
                    + chrono::Duration::seconds(view.remaining.as_secs().max(1) as i64)
            } else {
                Local::now() + chrono::Duration::minutes(minutes as i64)
            };
            ui.label(
                egui::RichText::new(format!(
                    "{} à {} h {:02}",
                    self.action.when(),
                    end.hour(),
                    end.minute()
                ))
                .size(22.0)
                .color(INK),
            );
        }
        ui.add_space(8.0);

        if active {
            if let Some(percent) = view.volume_percent {
                ui.label(egui::RichText::new(format!("Volume {percent} %")).size(16.0).color(INK));
            }
        } else if !muted {
            ui.label(
                egui::RichText::new("Le son baisse jusqu'à cette heure.")
                    .size(16.0)
                    .color(INK),
            );
        }

        if !view.notice.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&view.notice).size(16.0).color(INK));
        }
        if !self.error.is_empty() && !active {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&self.error).size(16.0).color(INK));
        }
        if !self.update_note.is_empty() && !active {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&self.update_note).size(16.0).color(INK_DIM));
        }

        if !active {
            self.settings(ui);
        }
        ui.add_space(8.0);
    }

    /// Toujours visible en bas de la fenêtre, quelle que soit la hauteur du formulaire.
    fn actions(&mut self, ui: &mut egui::Ui) {
        let view = self.session.view();
        let active = matches!(view.phase, Phase::Running | Phase::Finishing);
        let muted = view.phase == Phase::Muted;
        let button_label = if muted {
            "Rétablir le son"
        } else if active && self.session.cancel_requested() {
            "Annulation"
        } else if active {
            "Annuler"
        } else {
            "Commencer"
        };
        let brass = !active || muted;
        if action_button(ui, button_label, brass, button_label == "Commencer").clicked() {
            if muted {
                self.session.restore_sound();
            } else if active {
                self.session.cancel();
            } else {
                self.try_start();
            }
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.persist();
        self.session.shutdown();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(22.0);
        group_label(ui, "Durée");
        let current = self.parsed_minutes();
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            for minutes in PRESETS {
                let selected = current == Some(minutes);
                if chip(ui, &minutes.to_string(), selected).clicked() {
                    picked = Some(minutes);
                }
            }
            ui.add_space(6.0);
            let response = field(ui, &mut self.minutes, 44.0, "45", true);
            keep_digits(&mut self.minutes);
            if response.changed() {
                self.error.clear();
            }
            if response.lost_focus() && self.parsed_minutes().is_some() {
                self.persist();
            }
            ui.label(egui::RichText::new("min").size(16.0).color(INK_DIM));
        });
        if let Some(minutes) = picked {
            self.minutes = minutes.to_string();
            self.error.clear();
            self.persist();
        }

        ui.add_space(18.0);
        group_label(ui, "Courbe");
        let mut next_curve = None;
        ui.horizontal_wrapped(|ui| {
            for curve in FadeCurve::ALL {
                if chip(ui, curve.label(), curve == self.curve).clicked() {
                    next_curve = Some(curve);
                }
            }
        });
        if let Some(curve) = next_curve {
            self.curve = curve;
            self.persist();
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new(self.curve.detail()).size(16.0).color(INK_DIM));
        reveal(ui, "pendant", self.curve == FadeCurve::LateFade, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Pendant").size(16.0).color(INK));
                let response = field(ui, &mut self.late, 44.0, "10", true);
                keep_digits(&mut self.late);
                if response.lost_focus() {
                    self.persist();
                }
                ui.label(egui::RichText::new("minutes").size(16.0).color(INK));
            });
        });

        ui.add_space(18.0);
        group_label(ui, "À la fin");
        let mut next_action = None;
        ui.horizontal_wrapped(|ui| {
            for action in SleepAction::ALL {
                if chip(ui, action.label(), action == self.action).clicked() {
                    next_action = Some(action);
                }
            }
        });
        if let Some(action) = next_action {
            self.action = action;
            self.persist();
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new(self.action.detail()).size(16.0).color(INK_DIM));

        ui.add_space(18.0);
        if toggle(ui, self.tv_enabled, "Éteindre la Google TV").clicked() {
            self.tv_enabled = !self.tv_enabled;
            self.error.clear();
            self.persist();
        }
        let tv_enabled = self.tv_enabled;
        reveal(ui, "tele", tv_enabled, |ui| self.tv_settings(ui));

        ui.add_space(10.0);
        if toggle(ui, self.startup, "Ouvrir au démarrage de Windows").clicked() {
            let next = !self.startup;
            match crate::startup::set_enabled(next) {
                Ok(()) => {
                    self.startup = next;
                    self.error.clear();
                    self.persist();
                }
                Err(err) => {
                    self.startup = crate::startup::is_enabled();
                    self.error = format!("Démarrage automatique impossible ({err}).");
                }
            }
        }
    }

    fn tv_settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let mut search = false;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 36.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let search_label = if self.searching { "Recherche" } else { "Chercher" };
                search = chip(ui, search_label, false).clicked();
                let width = ui.available_width() - 8.0;
                let response = field(ui, &mut self.ip, width, "adresse IP", false);
                if response.changed() {
                    self.error.clear();
                    self.mac.clear();
                    self.learn_tried = false;
                }
                if response.lost_focus() {
                    self.persist();
                }
            },
        );
        if search {
            self.discover_open = true;
            self.about_open = false;
            self.begin_search();
        }
        if self.tv_paired {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("La télé est appairée.").size(16.0).color(INK_DIM));
        }
        ui.add_space(8.0);
        let mut pair = false;
        let mut job = None;
        ui.horizontal_wrapped(|ui| {
            let pair_label = if self.tv_paired { "Appairer à nouveau" } else { "Appairer" };
            if self.pairing.is_none() && self.pair_page.is_none() && chip(ui, pair_label, false).clicked() {
                pair = true;
            }
            let on_label = if matches!(self.tv_job, Some(TvJob::On)) { "Allumage" } else { "Allumer" };
            if chip(ui, on_label, false).clicked() {
                job = Some(TvJob::On);
            }
            if self.tv_paired {
                let off_label = if matches!(self.tv_job, Some(TvJob::Off)) { "Extinction" } else { "Éteindre" };
                if chip(ui, off_label, false).clicked() {
                    job = Some(TvJob::Off);
                }
            }
        });
        if let Some(job) = job {
            self.begin_tv(job);
        }
        if pair {
            let ip = self.ip.trim().to_string();
            if ip.is_empty() {
                self.error = "Indiquez l'adresse de la télé.".into();
            } else {
                match PairPage::open(&ip) {
                    Ok(page) => {
                        self.pair_page = Some(page);
                        self.about_open = false;
                        self.discover_open = false;
                        self.error.clear();
                    }
                    Err(err) => {
                        self.error = format!("Page du téléphone indisponible ({err}).");
                        self.start_handshake();
                    }
                }
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut quit = false;
        let view = self.session.view();
        // Au repos le logo est plein ; pendant le compte, il se remplit avec le temps écoulé.
        let target = match view.phase {
            Phase::Running | Phase::Finishing if !view.total.is_zero() => {
                1.0 - view.remaining.as_secs_f32() / view.total.as_secs_f32()
            }
            _ => preview_progress().unwrap_or(1.0),
        };
        // Retour en douceur à la pleine lumière quand on annule.
        let progress = ui
            .ctx()
            .animate_value_with_time(egui::Id::new("logo-progress"), target.clamp(0.0, 1.0), 0.6);
        let logo = self.logo.take();
        egui::menu::bar(ui, |ui| {
            let entries = |ui: &mut egui::Ui| {
                if ui.button("À propos").clicked() {
                    self.about_open = true;
                    self.discover_open = false;
                    ui.close_menu();
                }
                if ui.button("GitHub").clicked() {
                    open_github();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quitter").clicked() {
                    quit = true;
                    ui.close_menu();
                }
            };
            // Le logo tient lieu de nom et ouvre le menu, comme le mot qu'il remplace.
            match &logo {
                Some(logo) => {
                    // Au moins la moitié de la largeur : le logo est la tête de la fenêtre.
                    let width = (ui.available_width() * 0.6).clamp(220.0, 400.0);
                    let size = egui::vec2(width, width * logo.aspect());
                    let button = egui::Button::new("").frame(false).min_size(size);
                    let response = egui::menu::menu_custom_button(ui, button, entries).response;
                    let hover = hover_amount(ui, &response);
                    let time = ui.input(|input| input.time) as f32;
                    let rect = egui::Rect::from_min_size(response.rect.min, size);
                    logo.paint(ui.painter(), rect, time, progress, hover);
                    response.on_hover_cursor(egui::CursorIcon::PointingHand);
                }
                None => {
                    ui.menu_button("Nemuru", entries);
                }
            }
        });
        self.logo = logo;
        if quit {
            self.quit(ui.ctx());
        }
    }

    fn overlays(&mut self, ctx: &egui::Context) {
        let pairing = self.pair_page.is_some() || self.pairing.is_some();
        let open = self.about_open || self.discover_open || pairing;
        let shade = ctx.animate_bool_with_time(egui::Id::new("scrim-shade"), open, 0.18);
        if shade <= 0.0 {
            return;
        }
        // Un clic à côté ferme les fenêtres d'information, jamais un appairage en cours.
        if scrim(ctx, shade).clicked() && !pairing {
            self.about_open = false;
            self.discover_open = false;
        }
        self.about_modal(ctx);
        self.discover_modal(ctx);
        if pairing && !self.about_open && !self.discover_open {
            self.pairing_modal(ctx);
        }
    }

    fn about_modal(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut github = false;
        modal(ctx, "propos", self.about_open, |ui| {
            ui.label(egui::RichText::new("Nemuru").size(22.0).color(INK));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .size(16.0)
                    .color(INK_DIM),
            );
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Le volume baisse, puis l'ordinateur se met en veille.")
                    .size(16.0)
                    .color(INK),
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if chip(ui, "GitHub", false).clicked() {
                    github = true;
                }
                if chip(ui, "Fermer", false).clicked() {
                    close = true;
                }
            });
        });
        if github {
            open_github();
        }
        if close {
            self.about_open = false;
        }
    }

    fn discover_modal(&mut self, ctx: &egui::Context) {
        let searching = self.searching;
        let note = self.discover_note.clone();
        let devices = self.devices.clone();
        let current = self.ip.clone();
        let mut picked = None;
        let mut relaunch = false;
        let mut close = false;
        modal(ctx, "reseau", self.discover_open, |ui| {
            ui.label(egui::RichText::new("Sur le réseau").size(22.0).color(INK));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Les télés qui répondent ici.")
                    .size(16.0)
                    .color(INK_DIM),
            );
            ui.add_space(12.0);
            if searching {
                searching_dots(ui);
            } else if !note.is_empty() {
                ui.label(egui::RichText::new(note).size(16.0).color(INK));
            }
            for device in &devices {
                ui.add_space(4.0);
                if device_row(ui, &device.name, &device.ip, current == device.ip).clicked() {
                    picked = Some(device.clone());
                }
            }
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if !searching && chip(ui, "Relancer", false).clicked() {
                    relaunch = true;
                }
                if chip(ui, "Fermer", false).clicked() {
                    close = true;
                }
            });
        });
        if let Some(device) = picked {
            self.mac.clear();
            self.learn_tried = false;
            self.ip = device.ip;
            self.port = device.port;
            self.persist();
            self.discover_open = false;
        }
        if relaunch {
            self.begin_search();
        }
        if close {
            self.discover_open = false;
        }
    }

    fn pairing_modal(&mut self, ctx: &egui::Context) {
        let waiting = self.pairing.is_some();
        let url = self.pair_page.as_ref().map(|page| page.url().to_string());
        let matrix = self
            .pair_page
            .as_ref()
            .map(|page| (page.modules().to_vec(), page.width()));
        let code = self.pair_code.clone();
        let display = self.display.clone();
        let mut show_code = false;
        let mut close = false;
        modal(ctx, "appairage", true, |ui| {
            ui.label(egui::RichText::new("Appairage").size(22.0).color(INK));
            ui.add_space(10.0);
            if !waiting {
                ui.label(
                    egui::RichText::new(
                        "Scannez ce code avec le téléphone, pendant que l'ordinateur est encore à l'écran.",
                    )
                    .size(16.0)
                    .color(INK),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "La page est servie par Nemuru, sur ce réseau. Le code de la télé n'en sort pas.",
                    )
                    .size(16.0)
                    .color(INK_DIM),
                );
                ui.add_space(12.0);
                if let Some((modules, width)) = matrix.as_ref() {
                    paint_qr(ui, modules, *width);
                    if let Some(url) = &url {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(url).size(13.0).color(INK_DIM));
                    }
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Si la page ne s'ouvre pas, autorisez Nemuru sur le réseau privé.")
                            .size(16.0)
                            .color(INK_DIM),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new("La télé quitte l'image de l'ordinateur et montre un code.")
                        .size(16.0)
                        .color(INK),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Saisissez-le sur le téléphone, ou tapez-le au clavier.")
                        .size(16.0)
                        .color(INK_DIM),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(pairing_slots(&code))
                        .size(40.0)
                        .family(display)
                        .color(INK),
                );
            }
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if !waiting && chip(ui, "Afficher le code sur la télé", false).clicked() {
                    show_code = true;
                }
                if chip(ui, "Fermer", false).clicked() {
                    close = true;
                }
            });
        });
        if show_code {
            self.start_handshake();
        }
        if close {
            self.end_pairing();
        }
    }
}

/// Fondu à l'ouverture et à la fermeture : `open` à faux laisse la fenêtre s'effacer.
fn modal(ctx: &egui::Context, id: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    let mut open = open;
    let width = (ctx.screen_rect().width() - 96.0).clamp(260.0, 400.0);
    egui::Window::new(id)
        .id(egui::Id::new(id))
        .open(&mut open)
        .order(egui::Order::Foreground)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(
            egui::Frame::NONE
                .fill(PAPER)
                .inner_margin(egui::Margin::same(24))
                .stroke(egui::Stroke::new(1.0_f32, INK_DIM)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            add(ui);
        });
}

fn scrim(ctx: &egui::Context, shade: f32) -> egui::Response {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("scrim"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .fade_in(false)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
            let alpha = (160.0 * shade).round() as u8;
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(alpha));
            response
        })
        .inner
}

/// Section qui se déplie et se replie en glissant, au lieu d'apparaître d'un coup.
fn reveal(ui: &mut egui::Ui, id: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    let id = ui.make_persistent_id(id);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open);
    state.set_open(open);
    state.show_body_unindented(ui, add);
}

/// Trois points qui respirent, dans l'esprit de la trame du fond.
fn searching_dots(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 22.0), egui::Sense::hover());
        let time = ui.input(|input| input.time) as f32;
        for index in 0..3 {
            let phase = time * 3.2 - index as f32 * 0.7;
            let glow = 0.5 + 0.5 * phase.sin();
            let center = egui::pos2(rect.left() + 5.0 + index as f32 * 12.0, rect.center().y);
            ui.painter()
                .circle_filled(center, 2.5 + glow * 1.5, blend(SURFACE_HOVER, INK, glow));
        }
        ui.label(egui::RichText::new("Recherche").size(16.0).color(INK_DIM));
    });
    ui.ctx().request_repaint();
}

/// Une ligne par appareil : le nom à gauche, l'adresse à droite.
fn device_row(ui: &mut egui::Ui, name: &str, ip: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
    let hover = hover_amount(ui, &response);
    let pick = ui.ctx().animate_bool_with_time(response.id.with("pick"), selected, 0.18);
    let fill = blend(blend(SURFACE, SURFACE_HOVER, hover), INK, pick);
    let ink = blend(INK, PAPER, pick);
    let dim = blend(INK_DIM, SURFACE, pick);
    ui.painter().rect_filled(rect, 0.0, fill);
    let font = egui::FontId::new(16.0, egui::FontFamily::Proportional);
    let name_rect = ui.painter().text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        font.clone(),
        ink,
    );
    // L'adresse prend la place qui reste, et se termine par « … » si elle déborde.
    let room = (rect.right() - 14.0 - (name_rect.right() + 20.0)).max(0.0);
    let mut job = egui::text::LayoutJob::simple_singleline(ip.to_owned(), font, dim);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let at = egui::pos2(rect.right() - 14.0 - galley.size().x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, dim);
    focus_ring(ui, &response, rect);
    response
}

fn paint_qr(ui: &mut egui::Ui, modules: &[bool], width: usize) {
    let quiet = 2usize;
    let cells = width + quiet * 2;
    let scale = (240.0 / cells as f32).floor().max(3.0);
    let side = cells as f32 * scale;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, INK);
    let origin = rect.min + egui::vec2(quiet as f32 * scale, quiet as f32 * scale);
    for y in 0..width {
        for x in 0..width {
            if modules.get(y * width + x).copied().unwrap_or(false) {
                let min = origin + egui::vec2(x as f32 * scale, y as f32 * scale);
                painter.rect_filled(
                    egui::Rect::from_min_size(min, egui::vec2(scale, scale)),
                    0.0,
                    PAPER,
                );
            }
        }
    }
}

fn keep_digits(text: &mut String) {
    let next: String = text.chars().filter_map(duration_digit).take(3).collect();
    if next != *text {
        *text = next;
    }
}

fn duration_digit(ch: char) -> Option<char> {
    match ch {
        '0'..='9' => Some(ch),
        '&' => Some('1'),
        'é' | 'É' => Some('2'),
        '"' => Some('3'),
        '\'' => Some('4'),
        '(' => Some('5'),
        '-' | '−' => Some('6'),
        'è' | 'È' => Some('7'),
        '_' => Some('8'),
        'ç' | 'Ç' => Some('9'),
        'à' | 'À' => Some('0'),
        _ => None,
    }
}

/// Un appareil répond souvent en IPv4 et en IPv6. On garde une ligne par nom,
/// l'adresse IPv4 en priorité, plus facile à lire et à recopier.
fn one_per_name(list: Vec<CastDevice>) -> Vec<CastDevice> {
    let has_v4 = |name: &str| {
        list.iter()
            .any(|device| device.name == name && device.ip.parse::<std::net::Ipv4Addr>().is_ok())
    };
    let mut kept: Vec<CastDevice> = Vec::new();
    for device in &list {
        let v4 = device.ip.parse::<std::net::Ipv4Addr>().is_ok();
        if (v4 || !has_v4(&device.name)) && !kept.iter().any(|seen| seen.name == device.name && seen.ip == device.ip) {
            kept.push(device.clone());
        }
    }
    kept
}

fn open_github() {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        let _ = ShellExecuteW(
            None,
            windows::core::w!("open"),
            windows::core::w!("https://github.com/t12lve/nemuru"),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

fn pairing_slots(code: &str) -> String {
    let mut slots = ['·'; 6];
    for (index, character) in code.chars().take(6).enumerate() {
        slots[index] = character;
    }
    slots.iter().map(|slot| slot.to_string()).collect::<Vec<_>>().join(" ")
}

impl eframe::App for SleepApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_discover();
        self.poll_pairing();
        self.poll_tv();
        self.poll_update();
        self.poll_tray(ctx);
        self.sync_tray();

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.about_open {
                self.about_open = false;
            } else if self.discover_open {
                self.discover_open = false;
            } else if self.pair_page.is_some() || self.pairing.is_some() {
                self.end_pairing();
            } else if self.session.is_active() {
                self.session.cancel();
            }
        }

        let active = self.session.is_active();
        if active && !self.tucked && self.tray.is_some() && self.window_shown {
            self.tucked = true;
            self.window_shown = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else if !active && self.tucked {
            self.tucked = false;
            if !self.window_shown {
                self.window_shown = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if active && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.window_shown = false;
            } else {
                if !active {
                    self.persist();
                }
                self.session.shutdown();
            }
        }

        let shown = self.window_shown && !ctx.input(|input| input.viewport().minimized.unwrap_or(false));
        // La soie bouge lentement : 30 images par seconde suffisent. Survols et
        // fondus demandent eux-mêmes une image de plus tant qu'ils tournent.
        let delay = if shown || self.pairing.is_some() || self.pair_page.is_some() {
            Duration::from_millis(33)
        } else if active {
            Duration::from_millis(200)
        } else {
            Duration::from_secs(20)
        };
        ctx.request_repaint_after(delay);

        let screen = ctx.screen_rect();
        let backdrop = ctx.layer_painter(egui::LayerId::background());
        backdrop.rect_filled(screen, 0.0, PAPER);
        let time = ctx.input(|input| input.time) as f32 * 0.35;
        crate::waves::paint(&backdrop, screen, time);

        egui::TopBottomPanel::bottom("actions")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 24,
                right: 24,
                top: 12,
                bottom: 24,
            }))
            .show_separator_line(false)
            .show(ctx, |ui| self.actions(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 24,
                right: 24,
                top: 24,
                bottom: 0,
            }))
            .show(ctx, |ui| {
                self.menu_bar(ui);
                ui.add_space(12.0);
                let scroll = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.body(ui));
                let view = scroll.inner_rect;
                let hidden_below = scroll.content_size.y - view.height() - scroll.state.offset.y;
                edge_fade(ui.painter(), view, true, (scroll.state.offset.y / 24.0).clamp(0.0, 1.0));
                edge_fade(ui.painter(), view, false, (hidden_below / 24.0).clamp(0.0, 1.0));
            });
        self.overlays(ctx);
    }
}

impl Tray {
    fn install() -> anyhow::Result<Self> {
        use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
        let _ = MenuEvent::receiver();
        let menu = Menu::new();
        let open_id = MenuId::new("open");
        let cancel_id = MenuId::new("cancel");
        let quit_id = MenuId::new("quit");
        let open = MenuItem::with_id(open_id.clone(), "Ouvrir", true, None);
        let cancel = MenuItem::with_id(cancel_id.clone(), "Annuler", false, None);
        let quit = MenuItem::with_id(quit_id.clone(), "Quitter", true, None);
        menu.append(&open)?;
        menu.append(&cancel)?;
        menu.append(&quit)?;
        let rgba = mark::rgba(32);
        let icon = tray_icon::Icon::from_rgba(rgba, 32, 32)?;
        let tray_icon = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Nemuru")
            .with_icon(icon)
            .build()?;
        Ok(Self {
            icon: tray_icon,
            cancel,
            open_id,
            cancel_id,
            quit_id,
            tip: "Nemuru".into(),
        })
    }
}

fn install_fonts(ctx: &egui::Context) -> egui::FontFamily {
    let mut fonts = egui::FontDefinitions::default();
    let mut display = egui::FontFamily::Proportional;
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\georgia.ttf") {
        fonts.font_data.insert(
            "georgia".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Name("display".into()))
            .or_default()
            .insert(0, "georgia".to_owned());
        display = egui::FontFamily::Name("display".into());
    }
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "segoe".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".to_owned());
    }
    ctx.set_fonts(fonts);
    display
}

fn apply_look(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 36.0;
    style.animation_time = 0.18;
    style.visuals = visuals();
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = PAPER;
    visuals.panel_fill = PAPER;
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE;
    visuals.code_bg_color = SURFACE;
    visuals.override_text_color = Some(INK);
    visuals.hyperlink_color = INK;
    visuals.selection.bg_fill = SURFACE_HOVER;
    visuals.selection.stroke = egui::Stroke::new(2.0_f32, BRASS);
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_stroke = egui::Stroke::NONE;
        widget.fg_stroke = egui::Stroke::new(1.0_f32, INK);
        widget.corner_radius = egui::CornerRadius::ZERO;
    }
    visuals.widgets.noninteractive.bg_fill = PAPER;
    visuals.widgets.noninteractive.weak_bg_fill = PAPER;
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0_f32, BRASS);
    visuals.widgets.active.bg_fill = SURFACE;
    visuals.widgets.active.weak_bg_fill = SURFACE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0_f32, BRASS);
    visuals.window_corner_radius = egui::CornerRadius::ZERO;
    visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    visuals
}

fn clock(ui: &mut egui::Ui, text: &str, family: egui::FontFamily) {
    let size = 76.0;
    let font = egui::FontId::new(size, family);
    let digit = ui.ctx().fonts(|fonts| {
        ('0'..='9')
            .map(|ch| fonts.glyph_width(&font, ch))
            .fold(0.0, f32::max)
    });
    let mut width = 0.0;
    for ch in text.chars() {
        width += if ch.is_ascii_digit() {
            digit
        } else {
            ui.ctx().fonts(|fonts| fonts.glyph_width(&font, ch))
        };
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, size), egui::Sense::hover());
    let mut x = rect.left();
    for ch in text.chars() {
        let slot_w = if ch.is_ascii_digit() {
            digit
        } else {
            ui.ctx().fonts(|fonts| fonts.glyph_width(&font, ch))
        };
        let slot = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(slot_w, rect.height()));
        ui.painter().text(
            slot.center(),
            egui::Align2::CENTER_CENTER,
            ch.to_string(),
            font.clone(),
            INK,
        );
        x += slot_w;
    }
}

fn group_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(16.0).color(INK));
    ui.add_space(6.0);
}

fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let font = egui::FontId::new(16.0, egui::FontFamily::Proportional);
    let galley = ui
        .ctx()
        .fonts(|fonts| fonts.layout_no_wrap(label.to_owned(), font.clone(), INK));
    let size = galley.size() + egui::vec2(28.0, 16.0);
    let (rect, response) = ui.allocate_exact_size(size.max(egui::vec2(36.0, 36.0)), egui::Sense::click());
    let hover = hover_amount(ui, &response);
    let pick = ui.ctx().animate_bool_with_time(response.id.with("pick"), selected, 0.18);
    let fill = blend(blend(SURFACE, SURFACE_HOVER, hover), INK, pick);
    let color = blend(INK, PAPER, pick);
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter().text(
        rect.center() + egui::vec2(0.0, if response.is_pointer_button_down_on() { 1.0 } else { 0.0 }),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        color,
    );
    focus_ring(ui, &response, rect);
    response
}

fn action_button(ui: &mut egui::Ui, label: &str, brass: bool, sunset: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 48.0), egui::Sense::click());
    let hover = hover_amount(ui, &response);
    let press = ui
        .ctx()
        .animate_bool_with_time(response.id.with("press"), response.is_pointer_button_down_on(), 0.06);
    let lift = 0.045 * hover * (1.0 - press) - 0.06 * press;
    if sunset {
        paint_sunset(ui.painter(), rect, rect.left(), rect.width(), lift);
    } else {
        let fill = if !brass {
            blend(SURFACE, SURFACE_HOVER, hover.max(press))
        } else {
            blend(blend(BRASS, BRASS_HOVER, hover), BRASS_PRESS, press)
        };
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    let color = if brass { ON_BRASS } else { INK };
    ui.painter().text(
        rect.center() + egui::vec2(0.0, if response.is_pointer_button_down_on() { 1.0 } else { 0.0 }),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
        color,
    );
    focus_ring(ui, &response, rect);
    response
}

/// Champ de saisie : un creux plus sombre que le papier, souligné d'un filet
/// qui s'éclaire au focus. On le distingue d'un coup d'œil des puces.
fn field(ui: &mut egui::Ui, text: &mut String, width: f32, hint: &str, digits: bool) -> egui::Response {
    let mut edit = egui::TextEdit::singleline(text)
        .desired_width(width)
        .min_size(egui::vec2(width.min(ui.available_width()), 36.0))
        .vertical_align(egui::Align::Center)
        .background_color(WELL)
        .hint_text(hint);
    if digits {
        edit = edit.horizontal_align(egui::Align::Center).char_limit(3);
    }
    let response = ui.add(edit);
    let lit = ui
        .ctx()
        .animate_bool_with_time(response.id.with("lit"), response.has_focus(), 0.14);
    let hover = hover_amount(ui, &response);
    let rect = response.rect;
    let line = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - 1.0 - lit),
        rect.right_bottom(),
    );
    ui.painter()
        .rect_filled(line, 0.0, blend(blend(SURFACE_HOVER, INK_DIM, hover), INK, lit));
    response
}

/// Le texte qui défile s'efface au bord au lieu d'être tranché net.
/// `strength` vaut 0 quand rien n'est caché de ce côté.
fn edge_fade(painter: &egui::Painter, view: egui::Rect, top: bool, strength: f32) {
    if strength <= 0.0 {
        return;
    }
    let depth = 28.0;
    let solid = egui::Color32::from_rgba_unmultiplied(PAPER.r(), PAPER.g(), PAPER.b(), (230.0 * strength) as u8);
    let (edge_y, inner_y) = if top {
        (view.top(), view.top() + depth)
    } else {
        (view.bottom(), view.bottom() - depth)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(egui::pos2(view.left() - 24.0, edge_y), solid);
    mesh.colored_vertex(egui::pos2(view.right() + 24.0, edge_y), solid);
    mesh.colored_vertex(egui::pos2(view.left() - 24.0, inner_y), egui::Color32::TRANSPARENT);
    mesh.colored_vertex(egui::pos2(view.right() + 24.0, inner_y), egui::Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(egui::Shape::mesh(mesh));
}

/// Survol fondu : 0 au repos, 1 sous le pointeur ou pendant l'appui.
fn hover_amount(ui: &egui::Ui, response: &egui::Response) -> f32 {
    let hot = response.hovered() || response.is_pointer_button_down_on();
    ui.ctx().animate_bool_with_time(response.id.with("hover"), hot, 0.12)
}

fn blend(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let [r, g, b] = mix_rgb([from.r(), from.g(), from.b()], [to.r(), to.g(), to.b()], t);
    egui::Color32::from_rgb(r.round() as u8, g.round() as u8, b.round() as u8)
}

fn toggle(ui: &mut egui::Ui, on: bool, label: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let box_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(0.0, 5.0),
        egui::vec2(18.0, 18.0),
    );
    let hover = hover_amount(ui, &response);
    let fill = ui.ctx().animate_bool_with_time(response.id.with("on"), on, 0.16);
    ui.painter()
        .rect_filled(box_rect, 0.0, blend(SURFACE, SURFACE_HOVER, hover));
    // Le carré clair grandit depuis le centre quand on coche.
    if fill > 0.0 {
        let inner = egui::Rect::from_center_size(box_rect.center(), box_rect.size() * fill);
        ui.painter().rect_filled(inner, 0.0, INK);
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(28.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
        INK,
    );
    focus_ring(ui, &response, box_rect.expand(3.0));
    response
}

fn focus_ring(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(3.0),
            0.0,
            egui::Stroke::new(2.0_f32, BRASS),
            egui::StrokeKind::Outside,
        );
    }
}

fn format_clock(remaining: Duration) -> String {
    let secs = remaining.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Bande étroite : ambre, laiton, reste de rose. La valeur reste assez haute
/// pour que le texte sombre du bouton passe partout, y compris au survol et à l'appui.
const DUSK: [u8; 3] = [196, 142, 102];
const SUN: [u8; 3] = [214, 168, 110];
const GLOW: [u8; 3] = [208, 150, 128];

/// Build de développement seulement : `NEMURU_APERCU=0.4` montre le logo rempli
/// à 40 % sans lancer de vrai compte (qui finirait en veille).
#[cfg(debug_assertions)]
fn preview_progress() -> Option<f32> {
    std::env::var("NEMURU_APERCU").ok()?.parse().ok()
}

#[cfg(not(debug_assertions))]
fn preview_progress() -> Option<f32> {
    None
}

fn paint_sunset(
    painter: &egui::Painter,
    rect: egui::Rect,
    span_left: f32,
    span_width: f32,
    lift: f32,
) {
    if rect.width() <= 0.5 || rect.height() <= 0.0 || span_width <= 0.0 {
        return;
    }
    const COLS: usize = 16;
    let mut mesh = egui::Mesh::default();
    for col in 0..=COLS {
        let x = rect.left() + rect.width() * (col as f32 / COLS as f32);
        let t = ((x - span_left) / span_width).clamp(0.0, 1.0);
        let top = sunset_color(t, 0.0, lift);
        let bottom = sunset_color(t, 1.0, lift);
        mesh.colored_vertex(egui::pos2(x, rect.top()), top);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), bottom);
        if col > 0 {
            let i = mesh.vertices.len() as u32;
            let left_top = i - 4;
            let left_bottom = i - 3;
            let right_top = i - 2;
            let right_bottom = i - 1;
            mesh.add_triangle(left_top, left_bottom, right_top);
            mesh.add_triangle(left_bottom, right_bottom, right_top);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn sunset_color(t: f32, vertical: f32, lift: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let horizontal = if t < 0.5 {
        mix_rgb(DUSK, SUN, t / 0.5)
    } else {
        mix_rgb(SUN, GLOW, (t - 0.5) / 0.5)
    };
    let shade = 0.95 + vertical.clamp(0.0, 1.0) * 0.08 + lift;
    let channel = |value: f32| (value * shade).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgb(
        channel(horizontal[0]),
        channel(horizontal[1]),
        channel(horizontal[2]),
    )
}

fn mix_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] as f32 + (b[0] as f32 - a[0] as f32) * t,
        a[1] as f32 + (b[1] as f32 - a[1] as f32) * t,
        a[2] as f32 + (b[2] as f32 - a[2] as f32) * t,
    ]
}

#[cfg(test)]
fn contrast_ratio(background: egui::Color32, foreground: egui::Color32) -> f32 {
    let lighter = relative_luminance(background).max(relative_luminance(foreground));
    let darker = relative_luminance(background).min(relative_luminance(foreground));
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
fn relative_luminance(color: egui::Color32) -> f32 {
    let channel = |value: u8| {
        let s = value as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

#[cfg(test)]
mod devices {
    use super::one_per_name;
    use crate::google_tv::CastDevice;

    fn device(name: &str, ip: &str) -> CastDevice {
        CastDevice { name: name.into(), ip: ip.into(), port: 8009 }
    }

    #[test]
    fn keeps_one_row_per_name_and_prefers_ipv4() {
        let kept = one_per_name(vec![
            device("Chambre", "fd00::1"),
            device("Chambre", "10.0.0.19"),
            device("Gaming", "fd00::2"),
            device("Salon", "10.0.0.16"),
            device("Salon", "10.0.0.16"),
        ]);
        let ips: Vec<&str> = kept.iter().map(|d| d.ip.as_str()).collect();
        assert_eq!(ips, ["10.0.0.19", "fd00::2", "10.0.0.16"]);
    }
}

#[cfg(test)]
mod digits {
    use super::{duration_digit, keep_digits};

    #[test]
    fn azerty_number_row_becomes_digits() {
        let mapped: String = "&é\"'(-è_çà".chars().filter_map(duration_digit).collect();
        assert_eq!(mapped, "1234567890");
    }

    #[test]
    fn duration_keeps_three_digits() {
        let mut text = String::from("éèçà");
        keep_digits(&mut text);
        assert_eq!(text, "279");
    }
}

#[cfg(test)]
mod sunset_contrast {
    use super::{contrast_ratio, sunset_color, ON_BRASS};

    #[test]
    fn button_text_stays_readable_across_the_sunset() {
        for step in 0..=8 {
            let t = step as f32 / 8.0;
            for vertical in [0.0, 1.0] {
                for lift in [0.0, 0.045, -0.06] {
                    let color = sunset_color(t, vertical, lift);
                    let ratio = contrast_ratio(color, ON_BRASS);
                    assert!(
                        ratio >= 4.5,
                        "t={t} vertical={vertical} lift={lift} ratio={ratio:.2} color={color:?}"
                    );
                }
            }
        }
    }
}
