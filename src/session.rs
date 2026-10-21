//! Compte à rebours hors du fil de la fenêtre.
//! Le volume de départ est toujours remis, sauf si l'écran seul s'éteint
//! ou si l'ordinateur s'arrête.

use crate::audio::{AudioController, SavedAudio};
use crate::config::AppConfig;
use crate::fade;
use crate::google_tv::GoogleTvController;
use crate::power::{self, PowerController, SleepAction};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Finishing,
    Muted,
}

#[derive(Debug, Clone)]
pub struct View {
    pub phase: Phase,
    pub remaining: Duration,
    pub total: Duration,
    pub volume_percent: Option<u8>,
    pub notice: String,
    pub cancellable: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            remaining: Duration::ZERO,
            total: Duration::ZERO,
            volume_percent: None,
            notice: String::new(),
            cancellable: false,
        }
    }
}

struct Shared {
    view: View,
    saved: Option<SavedAudio>,
}

pub struct Session {
    shared: Arc<Mutex<Shared>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                view: View::default(),
                saved: None,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    pub fn view(&self) -> View {
        lock(&self.shared).view.clone()
    }

    pub fn is_active(&self) -> bool {
        matches!(self.view().phase, Phase::Running | Phase::Finishing)
    }

    pub fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn start(&mut self, config: AppConfig) {
        if self.is_active() {
            return;
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.cancel.store(false, Ordering::SeqCst);
        let total = Duration::from_secs(config.duration_minutes.clamp(1, 480) * 60);
        {
            let mut shared = lock(&self.shared);
            shared.view = View {
                phase: Phase::Running,
                remaining: total,
                total,
                volume_percent: None,
                notice: String::new(),
                cancellable: true,
            };
        }
        let saved = lock(&self.shared).saved;
        let shared = Arc::clone(&self.shared);
        let cancel = Arc::clone(&self.cancel);
        self.thread = std::thread::Builder::new()
            .name("nemuru".into())
            .spawn(move || worker(config, saved, cancel, shared))
            .ok();
    }

    pub fn cancel(&self) {
        if self.is_active() {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn restore_sound(&mut self) {
        if self.is_active() {
            return;
        }
        if self.restore_saved() {
            let mut shared = lock(&self.shared);
            shared.view.phase = Phase::Idle;
            shared.view.notice = "Le volume est rétabli.".into();
            shared.view.volume_percent = None;
            shared.view.cancellable = false;
        }
    }

    pub fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.restore_saved();
    }

    fn restore_saved(&mut self) -> bool {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let saved = lock(&self.shared).saved.take();
        if let Some(saved) = saved {
            if let Ok(audio) = AudioController::new() {
                let _ = audio.restore(saved);
            }
            return true;
        }
        false
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|err| err.into_inner())
}

fn worker(
    config: AppConfig,
    restore_first: Option<SavedAudio>,
    cancel: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
) {
    let audio = AudioController::new().ok();
    if let (Some(audio), Some(previous)) = (audio.as_ref(), restore_first) {
        let _ = audio.restore(previous);
        lock(&shared).saved = None;
    }

    let saved = audio.as_ref().and_then(|audio| audio.capture().ok());
    let total = Duration::from_secs(config.duration_minutes.clamp(1, 480) * 60);
    let late = fade::late_fraction(total.as_secs_f32(), config.late_fade_minutes);

    if audio.is_none() {
        set_notice(
            &shared,
            "Le volume Windows est inaccessible. La veille aura quand même lieu.",
        );
    }

    power::keep_awake(true);
    let (outcome, tv_note) = countdown(
        &config,
        audio.as_ref(),
        saved,
        total,
        late,
        &cancel,
        &shared,
    );
    power::keep_awake(false);

    match outcome {
        Outcome::Cancelled => {
            if let (Some(audio), Some(saved)) = (audio.as_ref(), saved) {
                let _ = audio.restore(saved);
            }
            let mut text = String::from(if saved.is_some() {
                "Annulé. Le volume est rétabli."
            } else {
                "Annulé."
            });
            if let Some(note) = tv_note {
                text.push_str(&note);
            }
            finish_idle(&shared, &text);
        }
        Outcome::Finished => {
            if let Some(audio) = audio.as_ref() {
                let _ = audio.set_volume(0.0);
            }
            let tv_note = tv_note.unwrap_or_default();
            if cancel.load(Ordering::SeqCst) {
                if let (Some(audio), Some(saved)) = (audio.as_ref(), saved) {
                    let _ = audio.restore(saved);
                }
                let mut text = String::from(if saved.is_some() {
                    "Annulé. Le volume est rétabli."
                } else {
                    "Annulé."
                });
                text.push_str(&tv_note);
                finish_idle(&shared, &text);
                return;
            }

            let closing = match config.sleep_action {
                SleepAction::ScreenOff => "Extinction de l'écran.",
                SleepAction::Shutdown => "Arrêt de l'ordinateur.",
                SleepAction::Hibernate => "Hibernation.",
                SleepAction::Sleep => "Mise en veille.",
            };
            set_phase(&shared, Phase::Finishing, closing);
            {
                let mut shared = lock(&shared);
                shared.view.cancellable = false;
            }

            let action_result = PowerController::trigger_action(config.sleep_action);
            match config.sleep_action {
                SleepAction::ScreenOff => {
                    let mut notice = match action_result {
                        Ok(()) => {
                            if saved.is_some() {
                                String::from("L'écran est éteint. Le son reste coupé.")
                            } else {
                                String::from("L'écran est éteint.")
                            }
                        }
                        Err(err) => format!("L'écran n'a pas pu s'éteindre ({err})."),
                    };
                    notice.push_str(&tv_note);
                    let mut shared = lock(&shared);
                    shared.saved = saved;
                    shared.view.phase = Phase::Muted;
                    shared.view.cancellable = false;
                    shared.view.volume_percent = if saved.is_some() { Some(0) } else { None };
                    shared.view.notice = notice;
                }
                SleepAction::Shutdown => {
                    if let Err(err) = action_result {
                        if let (Some(audio), Some(saved)) = (audio.as_ref(), saved) {
                            let _ = audio.restore(saved);
                        }
                        finish_idle(
                            &shared,
                            &format!("L'arrêt a échoué ({err}). Le volume est rétabli.{tv_note}"),
                        );
                    }
                }
                SleepAction::Sleep | SleepAction::Hibernate => {
                    if let (Some(audio), Some(saved)) = (audio.as_ref(), saved) {
                        let _ = audio.restore(saved);
                    }
                    let mut text = match action_result {
                        Ok(()) => {
                            if saved.is_some() {
                                String::from("Le volume est rétabli.")
                            } else {
                                String::from("La veille est terminée.")
                            }
                        }
                        Err(err) => {
                            if saved.is_some() {
                                format!("La veille a échoué ({err}). Le volume est rétabli.")
                            } else {
                                format!("La veille a échoué ({err}).")
                            }
                        }
                    };
                    text.push_str(&tv_note);
                    finish_idle(&shared, &text);
                }
            }
        }
    }
}

enum Outcome {
    Cancelled,
    Finished,
}

fn countdown(
    config: &AppConfig,
    audio: Option<&AudioController>,
    saved: Option<SavedAudio>,
    total: Duration,
    late: f32,
    cancel: &AtomicBool,
    shared: &Arc<Mutex<Shared>>,
) -> (Outcome, Option<String>) {
    let started = Instant::now();
    let initial = saved.map(|s| s.volume).unwrap_or(1.0);
    let mut tv: Option<JoinHandle<Result<(), String>>> = None;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return (Outcome::Cancelled, join_tv(tv));
        }
        let elapsed = started.elapsed();
        let remaining = total.saturating_sub(elapsed);
        if config.google_tv_enabled && tv.is_none() && remaining <= crate::google_tv::LEAD {
            set_notice(shared, "Extinction de la télé.");
            let config = config.clone();
            if let Ok(handle) = std::thread::Builder::new()
                .name("google-tv".into())
                .spawn(move || run_tv(&config).map_err(|err| err.to_string()))
            {
                tv = Some(handle);
            }
        }
        if elapsed >= total {
            publish(shared, Duration::ZERO, total, Some(0));
            return (Outcome::Finished, join_tv(tv));
        }
        let progress = elapsed.as_secs_f32() / total.as_secs_f32();
        let target = initial * fade::volume_factor(progress, config.fade_curve, late);
        if let Some(audio) = audio {
            let _ = audio.set_volume(target);
        }
        let percent = if audio.is_some() {
            Some((target * 100.0).round().clamp(0.0, 100.0) as u8)
        } else {
            None
        };
        publish(shared, total.saturating_sub(elapsed), total, percent);
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn join_tv(handle: Option<JoinHandle<Result<(), String>>>) -> Option<String> {
    handle.map(|handle| match handle.join() {
        Ok(Ok(())) => " La télé a accepté l'extinction.".into(),
        _ => " La télé n'a pas répondu.".into(),
    })
}

fn run_tv(config: &AppConfig) -> anyhow::Result<()> {
    if crate::atv::is_paired() {
        return crate::atv::power_off(&config.google_tv_ip);
    }
    let tv = GoogleTvController::new(config.google_tv_ip.clone(), config.google_tv_port);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(tv.turn_off())
}

fn publish(shared: &Mutex<Shared>, remaining: Duration, total: Duration, volume_percent: Option<u8>) {
    let mut shared = lock(shared);
    if shared.view.phase == Phase::Running {
        shared.view.remaining = remaining;
        shared.view.total = total;
        shared.view.volume_percent = volume_percent;
    }
}

fn set_notice(shared: &Mutex<Shared>, notice: &str) {
    lock(shared).view.notice = notice.to_string();
}

fn set_phase(shared: &Mutex<Shared>, phase: Phase, notice: &str) {
    let mut shared = lock(shared);
    shared.view.phase = phase;
    shared.view.notice = notice.to_string();
}

fn finish_idle(shared: &Mutex<Shared>, notice: &str) {
    let mut shared = lock(shared);
    shared.saved = None;
    shared.view.phase = Phase::Idle;
    shared.view.cancellable = false;
    shared.view.volume_percent = None;
    shared.view.remaining = Duration::ZERO;
    shared.view.notice = notice.to_string();
}
