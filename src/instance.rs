//! Une seule fenêtre. La seconde tentative prévient et s'arrête.

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

pub fn acquire() -> bool {
    unsafe {
        let handle = match CreateMutexW(None, true, w!("Local\\Nemuru")) {
            Ok(handle) => handle,
            Err(_) => return true,
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            let _ = MessageBoxW(
                None,
                w!("Nemuru est déjà ouvert."),
                w!("Nemuru"),
                MB_OK | MB_ICONINFORMATION,
            );
            return false;
        }
        std::mem::forget(handle);
        true
    }
}
