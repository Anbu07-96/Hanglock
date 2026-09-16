//! Reading and writing `%APPDATA%\Hanglock\settings.toml`.
//!
//! Three rules, each earned the hard way by every utility that stores a small document:
//!
//! *   **Write atomically.** Write `settings.toml.new`, then replace. A crash or a locked file
//!     mid-write must never leave a half-written settings document, and on Windows a bare
//!     `rename` onto an existing file fails, so the replace goes through
//!     `MOVEFILE_REPLACE_EXISTING`-equivalent semantics (`ReplaceFileW` when available, rename
//!     over a removed target otherwise).
//! *   **Never overwrite something you cannot read.** An unparseable file is renamed to
//!     `settings.toml.corrupt-<n>` first. The user's chosen settings are not ours to destroy, and a
//!     recovery path that discards data is not a recovery path.
//! *   **Retry the transient case, give up on the real one politely.** An antivirus scanner holding
//!     the file open is the common failure on Windows and it clears in milliseconds.

use hanglock_core::settings::Settings;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .unwrap_or_else(|| PathBuf::from("."))
        });
    base.join("Hanglock")
}

#[must_use]
pub fn path() -> PathBuf {
    dir().join("settings.toml")
}

/// Load, with the corruption dance. `Err` carries the reason and means "defaults are in use, and
/// the old file is still on disk under a different name" — never "the app cannot start".
pub fn load() -> Result<Settings, String> {
    let p = path();
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // First run. Write the defaults now, so the next launch is not also a first run and so a
            // user has a file to look at.
            let _ = save(&Settings::default());
            return Ok(Settings::default());
        }
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    let (settings, warnings) = Settings::from_toml(&text);
    if warnings.iter().any(|w| w.starts_with("line")) && text.trim().is_empty() {
        // Empty file: treat as first run rather than corrupt.
        return Ok(settings);
    }
    if text
        .lines()
        .any(|l| !l.trim().is_empty() && !l.trim().starts_with('#') && !l.contains('='))
    {
        // Something structurally unreadable. Preserve it, use defaults, and say so once.
        let backup = quarantine(&p);
        eprintln!(
            "hanglock: settings file unreadable; kept at {}",
            backup.display()
        );
        return Ok(Settings::default());
    }
    Ok(settings)
}

fn quarantine(p: &std::path::Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("settings.toml.corrupt-{stamp}");
    let target = p.with_file_name(name);
    let _ = std::fs::rename(p, &target);
    target
}

/// Persist. Three attempts, because the interesting failure is a momentary lock and the second
/// failure is a user who will never see the third.
pub fn save(s: &Settings) -> Result<(), String> {
    let p = path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let tmp = p.with_file_name("settings.toml.new");
    let body = s.to_toml();
    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(25 * attempt));
        }
        match write_and_replace(&tmp, &p, body.as_bytes()) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn write_and_replace(
    tmp: &std::path::Path,
    p: &std::path::Path,
    bytes: &[u8],
) -> Result<(), String> {
    std::fs::write(tmp, bytes).map_err(|e| format!("write: {e}"))?;
    #[cfg(windows)]
    {
        // `ReplaceFileW` is the only primitive that substitutes a file another process may have
        // open-with-DELETE, which is exactly the state a running settings file is in.
        if replace_file_win(p, tmp) {
            return Ok(());
        }
    }
    match std::fs::rename(tmp, p) {
        Ok(()) => Ok(()),
        // On Windows a rename onto an existing path fails; drop it first. Losing the *old* file to
        // a failure in the gap is bounded by the fact that `tmp` was written and verified above.
        Err(_) => {
            let _ = std::fs::remove_file(p);
            std::fs::rename(tmp, p).map_err(|e| format!("replace: {e}"))
        }
    }
}

#[cfg(windows)]
fn replace_file_win(target: &std::path::Path, source: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn ReplaceFileW(
            existing: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            sd: *const core::ffi::c_void,
            reserved: *const core::ffi::c_void,
        ) -> i32;
    }
    const REPLACEFILE_WRITE_CHANGES: u32 = 0x1;
    const REPLACEFILE_SECURITY_SDDL: u32 = 0;
    let _ = REPLACEFILE_SECURITY_SDDL;
    let w = |p: &std::path::Path| -> Vec<u16> {
        let mut v: Vec<u16> = p.as_os_str().encode_wide().collect();
        v.push(0);
        v
    };
    let (t, s) = (w(target), w(source));
    unsafe {
        ReplaceFileW(
            t.as_ptr(),
            s.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_CHANGES,
            std::ptr::null(),
            std::ptr::null(),
        ) != 0
    }
}
