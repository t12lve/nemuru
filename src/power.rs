//! Veille, hibernation, extinction de l'écran, arrêt.
//! Pendant le compte à rebours, l'ordinateur reste éveillé pour que le délai arrive au bout.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use windows::Win32::System::Power::{
    SetSuspendState, SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SleepAction {
    Sleep,
    Hibernate,
    ScreenOff,
    Shutdown,
}

impl SleepAction {
    pub const ALL: [SleepAction; 4] = [
        SleepAction::Sleep,
        SleepAction::Hibernate,
        SleepAction::ScreenOff,
        SleepAction::Shutdown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SleepAction::Sleep => "Veille",
            SleepAction::Hibernate => "Hibernation",
            SleepAction::ScreenOff => "Écran éteint",
            SleepAction::Shutdown => "Arrêt",
        }
    }

    pub fn when(self) -> &'static str {
        match self {
            SleepAction::Sleep => "Veille",
            SleepAction::Hibernate => "Hibernation",
            SleepAction::ScreenOff => "Écran éteint",
            SleepAction::Shutdown => "Arrêt",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            SleepAction::Sleep => "Le volume revient au réveil.",
            SleepAction::Hibernate => "Le volume revient à la sortie de veille.",
            SleepAction::ScreenOff => "Le son reste coupé jusqu'à ce que vous le rétablissiez.",
            SleepAction::Shutdown => "L'ordinateur s'arrête complètement.",
        }
    }
}

pub struct PowerController;

impl PowerController {
    pub fn trigger_action(action: SleepAction) -> Result<()> {
        match action {
            SleepAction::Sleep => Self::sleep(),
            SleepAction::Hibernate => Self::hibernate(),
            SleepAction::ScreenOff => Self::turn_off_screens(),
            SleepAction::Shutdown => Self::shutdown(),
        }
    }

    pub fn sleep() -> Result<()> {
        unsafe {
            let res = SetSuspendState(false, false, false);
            if res.as_bool() {
                Ok(())
            } else {
                Err(anyhow!("La mise en veille a été refusée."))
            }
        }
    }

    pub fn hibernate() -> Result<()> {
        unsafe {
            let res = SetSuspendState(true, false, false);
            if res.as_bool() {
                Ok(())
            } else {
                Err(anyhow!("L'hibernation a été refusée."))
            }
        }
    }

    pub fn turn_off_screens() -> Result<()> {
        unsafe {
            let _ = PostMessageW(
                HWND_BROADCAST,
                WM_SYSCOMMAND,
                windows::Win32::Foundation::WPARAM(SC_MONITORPOWER as usize),
                windows::Win32::Foundation::LPARAM(2),
            );
            Ok(())
        }
    }

    pub fn shutdown() -> Result<()> {
        std::process::Command::new("shutdown")
            .args(["/s", "/t", "0"])
            .spawn()
            .map_err(|e| anyhow!("L'arrêt a échoué : {e}"))?;
        Ok(())
    }
}

pub fn keep_awake(on: bool) {
    unsafe {
        if on {
            let _ = SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        } else {
            let _ = SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}
