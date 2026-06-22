//! Run-on-login registration via the per-user `Run` registry key.
//!
//! Writes a `brightvol` value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` pointing at the current
//! executable. No installer or elevated rights required.

/// Result of an autostart operation; the error carries a human-readable message.
pub type Result<T> = std::result::Result<T, String>;

#[cfg(windows)]
mod imp {
    use super::Result;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    const VALUE_NAME: PCWSTR = w!("brightvol");

    /// Enable or disable run-on-login.
    pub fn set(enabled: bool) -> Result<()> {
        unsafe {
            let mut hkey = HKEY::default();
            let rc = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_SET_VALUE, &mut hkey);
            if rc != ERROR_SUCCESS {
                return Err(format!("cannot open Run key (error {})", rc.0));
            }

            let result = if enabled {
                let wide = exe_value()?;
                let bytes =
                    std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                RegSetValueExW(hkey, VALUE_NAME, 0, REG_SZ, Some(bytes))
            } else {
                let rc = RegDeleteValueW(hkey, VALUE_NAME);
                if rc == ERROR_FILE_NOT_FOUND {
                    ERROR_SUCCESS
                } else {
                    rc
                }
            };

            let _ = RegCloseKey(hkey);
            if result == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(format!("cannot write Run value (error {})", result.0))
            }
        }
    }

    /// Whether the `brightvol` Run value is currently present.
    pub fn is_enabled() -> bool {
        unsafe {
            let mut hkey = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_QUERY_VALUE, &mut hkey)
                != ERROR_SUCCESS
            {
                return false;
            }
            let rc = RegQueryValueExW(hkey, VALUE_NAME, None, None, None, None);
            let _ = RegCloseKey(hkey);
            rc == ERROR_SUCCESS
        }
    }

    /// The quoted, null-terminated wide path to the current executable.
    fn exe_value() -> Result<Vec<u16>> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut quoted = std::ffi::OsString::from("\"");
        quoted.push(exe);
        quoted.push("\"");
        Ok(quoted.encode_wide().chain(std::iter::once(0)).collect())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Result;

    pub fn set(_enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn is_enabled() -> bool {
        false
    }
}

pub use imp::{is_enabled, set};
