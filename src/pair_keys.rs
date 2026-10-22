//! Saisie du code d'appairage, même si la télé a quitté l'image de l'ordinateur.

use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{mpsc::Sender, Mutex};
use std::sync::OnceLock;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

struct Capture {
    sender: Option<Sender<String>>,
    buffer: String,
    sent: bool,
    held: u32,
}

static CAPTURE: OnceLock<Mutex<Capture>> = OnceLock::new();
static HOOK: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn capture() -> &'static Mutex<Capture> {
    CAPTURE.get_or_init(|| {
        Mutex::new(Capture {
            sender: None,
            buffer: String::new(),
            sent: false,
            held: 0,
        })
    })
}

pub fn arm(sender: Sender<String>) -> bool {
    {
        let mut capture = capture().lock().unwrap_or_else(|err| err.into_inner());
        capture.sender = Some(sender);
        capture.buffer.clear();
        capture.sent = false;
        capture.held = 0;
    }
    if !HOOK.load(Ordering::Relaxed).is_null() {
        return true;
    }
    let installed = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0)
    };
    match installed {
        Ok(hook) => {
            HOOK.store(hook.0, Ordering::Relaxed);
            true
        }
        Err(err) => {
            log::warn!("saisie du code impossible : {err}");
            disarm();
            false
        }
    }
}

pub fn disarm() {
    if let Some(mutex) = CAPTURE.get() {
        if let Ok(mut capture) = mutex.lock() {
            capture.sender = None;
            capture.buffer.clear();
            capture.sent = false;
            capture.held = 0;
        }
    }
    let hook = HOOK.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if !hook.is_null() {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(hook));
        }
    }
}

pub fn snapshot() -> String {
    capture()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .buffer
        .clone()
}

fn note_down(vk: u32) {
    let mut capture = capture().lock().unwrap_or_else(|err| err.into_inner());
    if capture.sender.is_none() || capture.held == vk {
        return;
    }
    capture.held = vk;
    if vk == 0x08 {
        capture.buffer.pop();
        return;
    }
    let Some(character) = hex_from_vk(vk) else {
        return;
    };
    if capture.buffer.len() >= 6 || capture.sent {
        return;
    }
    capture.buffer.push(character);
    if capture.buffer.len() == 6 {
        capture.sent = true;
        if let Some(sender) = &capture.sender {
            let _ = sender.send(capture.buffer.clone());
        }
    }
}

fn note_up(vk: u32) {
    let mut capture = capture().lock().unwrap_or_else(|err| err.into_inner());
    if capture.held == vk {
        capture.held = 0;
    }
}

fn hex_from_vk(vk: u32) -> Option<char> {
    match vk {
        0x30..=0x39 => char::from_u32(b'0' as u32 + (vk - 0x30)),
        0x41..=0x46 => char::from_u32(b'A' as u32 + (vk - 0x41)),
        0x60..=0x69 => char::from_u32(b'0' as u32 + (vk - 0x60)),
        _ => None,
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        let message = wparam.0 as u32;
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
            note_down(info.vkCode);
        } else if message == WM_KEYUP || message == WM_SYSKEYUP {
            note_up(info.vkCode);
        }
    }
    unsafe { CallNextHookEx(HHOOK(std::ptr::null_mut()), code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::hex_from_vk;

    #[test]
    fn letters_digits_and_numpad_map_to_hex() {
        assert_eq!(hex_from_vk(0x41), Some('A'));
        assert_eq!(hex_from_vk(0x46), Some('F'));
        assert_eq!(hex_from_vk(0x30), Some('0'));
        assert_eq!(hex_from_vk(0x39), Some('9'));
        assert_eq!(hex_from_vk(0x60), Some('0'));
        assert_eq!(hex_from_vk(0x47), None);
    }
}
