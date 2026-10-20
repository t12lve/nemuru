//! Entrée « Nemuru » dans la clé Run de l'utilisateur.
//! Ouvre seulement la fenêtre. Le compte à rebours ne démarre jamais tout seul.

use anyhow::{anyhow, Result};
use std::os::windows::ffi::OsStrExt;
use windows::core::w;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

const VALUE_NAME: windows::core::PCWSTR = w!("Nemuru");
const LEGACY_VALUE_NAME: windows::core::PCWSTR = w!("SommeilDouceur");

pub fn migrate_legacy() {
    unsafe {
        let Ok(key) = open_key() else {
            return;
        };
        let mut size = 0u32;
        let exists = RegQueryValueExW(key, LEGACY_VALUE_NAME, None, None, None, Some(&mut size))
            .is_ok()
            && size > 2;
        if exists {
            let _ = RegDeleteValueW(key, LEGACY_VALUE_NAME);
        }
        let _ = RegCloseKey(key);
        if exists && !is_enabled() {
            let _ = set_enabled(true);
        }
    }
}

pub fn is_enabled() -> bool {
    unsafe {
        let Ok(key) = open_key() else {
            return false;
        };
        let mut size = 0u32;
        let err = RegQueryValueExW(key, VALUE_NAME, None, None, None, Some(&mut size));
        let _ = RegCloseKey(key);
        err.is_ok() && size > 2
    }
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    unsafe {
        let key = open_key()?;
        let result = if enabled {
            let exe = std::env::current_exe()?;
            let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
            let set = RegSetValueExW(key, VALUE_NAME, 0, REG_SZ, Some(bytes));
            let _ = RegDeleteValueW(key, LEGACY_VALUE_NAME);
            set
        } else {
            let err = delete_missing_ok(key, VALUE_NAME);
            let legacy = delete_missing_ok(key, LEGACY_VALUE_NAME);
            if err.is_err() { err } else { legacy }
        };
        let _ = RegCloseKey(key);
        if result.is_err() {
            return Err(anyhow!("le registre a refusé la modification"));
        }
        Ok(())
    }
}

unsafe fn delete_missing_ok(key: HKEY, name: windows::core::PCWSTR) -> windows::Win32::Foundation::WIN32_ERROR {
    let err = unsafe { RegDeleteValueW(key, name) };
    if err == ERROR_FILE_NOT_FOUND {
        windows::Win32::Foundation::WIN32_ERROR(0)
    } else {
        err
    }
}

unsafe fn open_key() -> Result<HKEY> {
    let mut key = HKEY::default();
    let err = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    };
    if err.is_err() {
        return Err(anyhow!("clé de démarrage inaccessible"));
    }
    Ok(key)
}
