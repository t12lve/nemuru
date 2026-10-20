/* Hallmark · pre-emit critique: P5 H5 E4 S5 R5 V4
 * genre: editorial · macrostructure: bedside clock
 * tone: austere · anchor: brass on warm black
 * audience: someone about to sleep · use: start the fade · tone: soft, quiet
 */

//! Fenêtre. Le temps restant est le seul élément grand.
//! Le laiton n'apparaît que sur l'action principale.

use crate::config::AppConfig;
use crate::fade::FadeCurve;
use crate::google_tv::{self, CastDevice};
use crate::mark;
use crate::power::SleepAction;
use crate::session::{Phase, Session};
use chrono::{Local, Timelike};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

const PAPER: egui::Color32 = egui::Color32::from_rgb(22, 20, 18);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(54, 48, 42);
const SURFACE_HOVER: egui::Color32 = egui::Color32::from_rgb(72, 64, 56);
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

struct SleepApp {
    session: Session,
    display: egui::FontFamily,
    minutes: String,
    late: String,
    curve: FadeCurve,
    action: SleepAction,
    tv_enabled: bool,
    ip: String,
    port: u16,
    startup: bool,
    error: String,
    devices: Vec<CastDevice>,
    searching: bool,
    discover_rx: Option<Receiver<Result<Vec<CastDevice>, String>>>,
    tray: Option<Tray>,
    window_shown: bool,
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
            port: config.google_tv_port,
            startup: config.run_at_startup,
            error: String::new(),
            devices: Vec::new(),
            searching: false,
            discover_rx: None,
            tray,
            window_shown: true,
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
                self.devices = list;
                if self.devices.is_empty() {
                    self.error = "Aucune Google TV trouvée sur ce réseau.".into();
                }
            }
            Some(Ok(Err(err))) => {
                self.discover_rx = None;
                self.searching = false;
                self.error = format!("Recherche impossible ({err}).");
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.discover_rx = None;
                self.searching = false;
            }
            _ => {}
        }
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
        ui.label(egui::RichText::new("Nemuru").size(16.0).color(INK));
        ui.add_space(20.0);

        let clock_text = if active || view.phase == Phase::Finishing {
            format_clock(view.remaining)
        } else if let Some(minutes) = self.parsed_minutes() {
            format!("{minutes}:00")
        } else {
            "--:--".into()
        };
        clock(ui, &clock_text, self.display.clone());
        if active {
            ui.add_space(12.0);
            let progress = if view.total.is_zero() {
                1.0
            } else {
                let gone = view.total.saturating_sub(view.remaining).as_secs_f32();
                (gone / view.total.as_secs_f32()).clamp(0.0, 1.0)
            };
            sunset_bar(ui, progress);
            ui.add_space(10.0);
        } else {
            ui.add_space(6.0);
        }

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

        if !active {
            self.settings(ui);
        } else {
            ui.add_space(28.0);
        }

        ui.add_space(16.0);
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

        if !active {
            ui.add_space(12.0);
            if text_button(ui, "Quitter").clicked() {
                self.persist();
                self.session.shutdown();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ui.add_space(8.0);
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
        });
        if let Some(minutes) = picked {
            self.minutes = minutes.to_string();
            self.error.clear();
            self.persist();
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.minutes)
                    .desired_width(72.0)
                    .hint_text("45")
                    .char_limit(3),
            );
            if response.changed() {
                self.error.clear();
            }
            ui.label(egui::RichText::new("minutes").size(16.0).color(INK));
        });

        ui.add_space(14.0);
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
        if self.curve == FadeCurve::LateFade {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Pendant").size(16.0).color(INK));
                ui.add(
                    egui::TextEdit::singleline(&mut self.late)
                        .desired_width(48.0)
                        .char_limit(3),
                );
                ui.label(egui::RichText::new("minutes").size(16.0).color(INK));
            });
        }

        ui.add_space(14.0);
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

        ui.add_space(14.0);
        if toggle(ui, self.tv_enabled, "Éteindre la Google TV").clicked() {
            self.tv_enabled = !self.tv_enabled;
            self.error.clear();
            self.persist();
        }
        if self.tv_enabled {
            ui.add_space(8.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.ip)
                    .desired_width(f32::INFINITY)
                    .hint_text("192.168.1.42"),
            );
            if response.changed() {
                self.error.clear();
            }
            ui.add_space(8.0);
            let search_label = if self.searching { "Recherche" } else { "Chercher" };
            if chip(ui, search_label, false).clicked() && !self.searching {
                self.begin_search();
            }
            for device in self.devices.clone() {
                ui.add_space(4.0);
                let label = format!("{}   {}", device.name, device.ip);
                if chip(ui, &label, self.ip == device.ip).clicked() {
                    self.ip = device.ip;
                    self.port = device.port;
                    self.persist();
                }
            }
        }

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
}

impl eframe::App for SleepApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_discover();
        self.poll_tray(ctx);
        self.sync_tray();

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) && self.session.is_active() {
            self.session.cancel();
        }

        let active = self.session.is_active();
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
        let delay = if shown {
            Duration::from_millis(66)
        } else if active {
            Duration::from_millis(200)
        } else {
            Duration::from_secs(20)
        };
        ctx.request_repaint_after(delay);

        let frame = egui::Frame::NONE.fill(PAPER);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let time = ui.input(|input| input.time) as f32 * 0.35;
            crate::waves::paint(ui.painter(), ui.max_rect(), time);
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(24))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.body(ui));
                });
        });
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
    let fill = if selected {
        INK
    } else if response.hovered() || response.is_pointer_button_down_on() {
        SURFACE_HOVER
    } else {
        SURFACE
    };
    let color = if selected { PAPER } else { INK };
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
    let lift = if response.is_pointer_button_down_on() {
        -0.06
    } else if response.hovered() {
        0.045
    } else {
        0.0
    };
    if sunset {
        paint_sunset(ui.painter(), rect, rect.left(), rect.width(), lift);
    } else {
        let fill = if !brass {
            if response.hovered() || response.is_pointer_button_down_on() {
                SURFACE_HOVER
            } else {
                SURFACE
            }
        } else if response.is_pointer_button_down_on() {
            BRASS_PRESS
        } else if response.hovered() {
            BRASS_HOVER
        } else {
            BRASS
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

fn text_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let font = egui::FontId::new(16.0, egui::FontFamily::Proportional);
    let galley = ui
        .ctx()
        .fonts(|fonts| fonts.layout_no_wrap(label.to_owned(), font.clone(), INK));
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + egui::vec2(4.0, 8.0), egui::Sense::click());
    let color = if response.hovered() { INK_DIM } else { INK };
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    focus_ring(ui, &response, rect);
    response
}

fn toggle(ui: &mut egui::Ui, on: bool, label: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let box_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(0.0, 5.0),
        egui::vec2(18.0, 18.0),
    );
    let fill = if on {
        INK
    } else if response.hovered() {
        SURFACE_HOVER
    } else {
        SURFACE
    };
    ui.painter().rect_filled(box_rect, 0.0, fill);
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

fn sunset_bar(ui: &mut egui::Ui, progress: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 5.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, SURFACE);
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.0 {
        return;
    }
    let fill = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.left() + rect.width() * progress, rect.bottom()),
    );
    paint_sunset(ui.painter(), fill, rect.left(), rect.width(), 0.0);
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
