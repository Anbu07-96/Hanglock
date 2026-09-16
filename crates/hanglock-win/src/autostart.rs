//! Start-with-Sign-in, via the `HKCU` `Run` key.
//!
//! Chosen over a scheduled task or a Startup-folder shortcut because it is what Task Manager's
//! "Startup apps" page reads, it needs no elevation, it is one value, and removing it is a single
//! delete a user can verify themselves.
//!
//! The state is *reconciled*, not trusted: the user can disable us from Task Manager without asking,
//! and a settings toggle that lies about what the system is doing is worse than no toggle. So the
//! query runs at startup and again whenever the setting is written.

use crate::sys;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE: &str = "Hanglock";

#[link(name = "advapi32")]
extern "system" {
    fn RegSetKeyValueW(
        key: *mut core::ffi::c_void,
        subkey: *const u16,
        value: *const u16,
        kind: u32,
        data: *const core::ffi::c_void,
        len: u32,
    ) -> i32;
    fn RegDeleteKeyValueW(
        key: *mut core::ffi::c_void,
        subkey: *const u16,
        value: *const u16,
    ) -> i32;
    fn RegQueryValueExW(
        key: *mut core::ffi::c_void,
        value: *const u16,
        reserved: *const u32,
        kind: *mut u32,
        data: *mut u8,
        len: *mut u32,
    ) -> i32;
    fn RegOpenKeyExW(
        key: *mut core::ffi::c_void,
        subkey: *const u16,
        options: u32,
        access: u32,
        out: *mut *mut core::ffi::c_void,
    ) -> i32;
    fn RegCloseKey(key: *mut core::ffi::c_void) -> i32;
}

const HKEY_CURRENT_USER: *mut core::ffi::c_void = 0x8000_0001isize as *mut core::ffi::c_void;
const REG_SZ: u32 = 1;
const KEY_READ: u32 = 0x20019;
const ERROR_SUCCESS: i32 = 0;

/// Enable or disable autostart. `exe` is `std::env::current_exe()`, quoted because a path under
/// `C:\Program Files\...` is otherwise split at the space by the shell that reads the key.
pub fn set_enabled(exe: &str, enabled: bool) -> Result<(), i32> {
    let sub = crate::sys::wide(RUN_KEY);
    let val = crate::sys::wide(VALUE);
    if !enabled {
        let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sub.as_ptr(), val.as_ptr()) };
        // S_FALSE/FILE_NOT_FOUND on a key we never wrote is the success case for "off".
        return if rc == ERROR_SUCCESS || rc == 2 {
            Ok(())
        } else {
            Err(rc)
        };
    }
    let data = format!("\"{exe}\" --background");
    let wide = crate::sys::wide(&data);
    let bytes = (wide.len() * 2) as u32;
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            val.as_ptr(),
            REG_SZ,
            wide.as_ptr() as *const core::ffi::c_void,
            bytes,
        )
    };
    if rc == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(rc)
    }
}

#[must_use]
pub fn is_enabled() -> bool {
    unsafe {
        let sub = crate::sys::wide(RUN_KEY);
        let val = crate::sys::wide(VALUE);
        let mut key: *mut core::ffi::c_void = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_READ, &mut key) != ERROR_SUCCESS {
            return false;
        }
        let mut kind = 0u32;
        let mut len = 0u32;
        // Two calls: the first with a null buffer reports the size we need.
        let probe = RegQueryValueExW(
            key,
            val.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut len,
        );
        RegCloseKey(key);
        probe == ERROR_SUCCESS && len > 0
    }
}
