//! Start-with-Sign-in, via the `HKCU` `Run` key.
//!
//! Chosen over a scheduled task or a Startup-folder shortcut because it is what Task Manager's
//! "Startup apps" page reads, it needs no elevation, it is one value, and removing it is a single
//! delete a user can verify themselves.
//!
//! The state is *reconciled*, not trusted: the user can disable us from Task Manager without asking,
//! and a settings toggle that lies about what the system is doing is worse than no toggle. So the
//! query runs at startup and again whenever the setting is written.

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
/// What Task Manager writes next to the `Run` entry to keep it while refusing to run it. Clearing it
/// is how its "Enable" button works, and how a user re-enabling us from our own settings has to work.
const APPROVED_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
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
    if enabled {
        // Enabling means enabling, not "enabling, unless something disabled it on the way past us".
        let _ = clear_approval();
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
            wide.as_ptr().cast::<core::ffi::c_void>(),
            bytes,
        )
    };
    if rc == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(rc)
    }
}

/// Whether Task Manager currently lets the entry run.
///
/// `StartupApproved\Run` is not a second setting: it is the flag the Startup page writes when a user
/// clicks "Disable" on an entry that still exists in `Run`. Reading only `Run` would make our checkbox
/// say "on" while Windows declined to start us, which is the exact situation this module's opening
/// paragraph promises not to be in.
#[must_use]
pub fn approved() -> bool {
    unsafe {
        let sub_key = crate::sys::wide(APPROVED_KEY);
        let val = crate::sys::wide(VALUE);
        let mut key: *mut core::ffi::c_void = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, KEY_READ, &mut key)
            != ERROR_SUCCESS
        {
            // No key at all is the common case: nothing has been disabled.
            return true;
        }
        let mut kind = 0u32;
        let mut len = 0u32;
        let probe = RegQueryValueExW(
            key,
            val.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut len,
        );
        let mut answer = true;
        if probe == ERROR_SUCCESS && len > 0 {
            let mut buf = [0u8; 12];
            let mut got = len.min(buf.len() as u32);
            if RegQueryValueExW(
                key,
                val.as_ptr(),
                std::ptr::null(),
                &mut kind,
                buf.as_mut_ptr(),
                &mut got,
            ) == ERROR_SUCCESS
            {
                answer = approved_bytes(&buf[..got as usize]);
            }
        }
        RegCloseKey(key);
        answer
    }
}

/// The documented shape is a 12-byte blob whose first four bytes say `enabled` (2) or `disabled` (3);
/// other builds have carried a FILETIME instead. Only the known disabled marker disables: an
/// unrecognised blob is read as "allowed", because guessing that a user's clock should not start is a
/// worse mistake than letting it start.
#[must_use]
pub fn approved_bytes(bytes: &[u8]) -> bool {
    bytes.len() < 4 || bytes[0] != 3
}

/// What the system will actually do at the next sign-in: the entry exists *and* has not been
/// disabled from Task Manager.
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
        probe == ERROR_SUCCESS && len > 0 && approved()
    }
}

/// Write or remove the entry, and answer with what the registry now says. The caller does not get a
/// bare `Ok`: the only useful answer to "did you start with Windows?" is what a subsequent read of the
/// key reports, so a refused write cannot leave the settings document claiming something false.
pub fn apply(exe: &str, wanted: bool) -> bool {
    if set_enabled(exe, wanted).is_err() {
        // A failed write leaves whatever was there in place, which is what `is_enabled` will report.
        return is_enabled();
    }
    is_enabled() == wanted
}

/// Remove Task Manager's "do not run this" flag. Errors are ignored on purpose: the flag not being
/// there is the state we want, and the write of the `Run` value that follows reports the result.
pub fn clear_approval() -> Result<(), i32> {
    let sub = crate::sys::wide(APPROVED_KEY);
    let val = crate::sys::wide(VALUE);
    let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sub.as_ptr(), val.as_ptr()) };
    if rc == ERROR_SUCCESS || rc == 2 {
        Ok(())
    } else {
        Err(rc)
    }
}

#[cfg(test)]
mod tests {
    use super::approved_bytes;

    #[test]
    fn only_the_known_disabled_marker_disables() {
        // The documented 12-byte blob: 3 = disabled, 2 = enabled.
        assert!(!approved_bytes(&[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(approved_bytes(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        // An absent or short value is "never disabled", which is the state of a machine where nobody
        // has opened Task Manager's Startup page at all.
        assert!(approved_bytes(&[]));
        assert!(approved_bytes(&[3, 0]));
        // An unrecognised shape is read as allowed rather than guessed at: starting a clock the user
        // asked for is recoverable, a clock that silently never starts is not noticed.
        assert!(approved_bytes(&[7, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]));
    }
}
