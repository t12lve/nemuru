//! Volume maître Windows, le même que le curseur de la barre des tâches (eConsole).

use anyhow::{anyhow, Result};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub struct AudioController {
    endpoint_volume: IAudioEndpointVolume,
    release_com: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SavedAudio {
    pub volume: f32,
    pub muted: bool,
}

impl AudioController {
    pub fn new() -> Result<Self> {
        unsafe {
            let release_com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| anyhow!("Périphérique audio inaccessible : {e}"))?;

            let default_device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| anyhow!("Sortie audio par défaut inaccessible : {e}"))?;

            let endpoint_volume: IAudioEndpointVolume = default_device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| anyhow!("Volume maître inaccessible : {e}"))?;

            Ok(Self {
                endpoint_volume,
                release_com,
            })
        }
    }

    pub fn capture(&self) -> Result<SavedAudio> {
        Ok(SavedAudio {
            volume: self.get_volume()?,
            muted: self.get_mute()?,
        })
    }

    pub fn restore(&self, saved: SavedAudio) -> Result<()> {
        self.set_mute(saved.muted)?;
        self.set_volume(saved.volume)?;
        Ok(())
    }

    pub fn get_volume(&self) -> Result<f32> {
        unsafe {
            self.endpoint_volume
                .GetMasterVolumeLevelScalar()
                .map_err(|e| anyhow!("Lecture du volume impossible : {e}"))
        }
    }

    pub fn set_volume(&self, level: f32) -> Result<()> {
        let clamped = level.clamp(0.0, 1.0);
        unsafe {
            self.endpoint_volume
                .SetMasterVolumeLevelScalar(clamped, std::ptr::null())
                .map_err(|e| anyhow!("Réglage du volume impossible : {e}"))?;
            Ok(())
        }
    }

    pub fn get_mute(&self) -> Result<bool> {
        unsafe {
            self.endpoint_volume
                .GetMute()
                .map(|mute| mute.as_bool())
                .map_err(|e| anyhow!("Lecture du mode muet impossible : {e}"))
        }
    }

    pub fn set_mute(&self, mute: bool) -> Result<()> {
        unsafe {
            self.endpoint_volume
                .SetMute(mute, std::ptr::null())
                .map_err(|e| anyhow!("Mode muet inaccessible : {e}"))?;
            Ok(())
        }
    }
}

impl Drop for AudioController {
    fn drop(&mut self) {
        if self.release_com {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
