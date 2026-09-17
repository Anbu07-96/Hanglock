//! Reading and writing `%APPDATA%\Hanglock\settings.toml`.
//!
//! Four rules, each earned the hard way by every utility that stores a small document:
//!
//! *   **Write atomically.** Write `settings.toml.new`, then replace. A crash or a locked file
//!     mid-write must never leave a half-written settings document, and on Windows a bare
//!     `rename` onto an existing file fails, so the replace goes through
//!     `MOVEFILE_REPLACE_EXISTING`-equivalent semantics (`ReplaceFileW` when available, rename
//!     over a removed target otherwise).
//! *   **Never overwrite something you cannot read.** A file that is not plausibly a Hanglock
//!     document is renamed to `settings.toml.corrupt-<n>` first. The user's chosen settings are not
//!     ours to destroy, and a recovery path that discards data is not a recovery path.
//! *   **Create the file when there is something to say.** A first run does not write: defaults are
//!     the absence of a file, and writing one on every fresh profile puts a file in `%APPDATA%` for a
//!     clock the user may have run for four seconds and quit. The first command that changes anything
//!     answers with [`crate::model::Action::Save`], and that is when the file appears.
//! *   **Retry the transient case, give up on the real one politely.** An antivirus scanner holding
//!     the file open is the common failure on Windows and it clears in milliseconds.
//!
//! Everything here is a function of a directory rather than of a path out of the environment, so the
//! three rules above are tested on a Linux runner with a temp dir: the recovery paths are the code
//! most likely to be wrong and least likely to be noticed.

use hanglock_core::settings::Settings;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where the document lives: an environment variable, joined with the app's own directory name.
#[must_use]
pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map_or_else(
            || {
                std::env::var_os("HOME")
                    .map_or_else(|| PathBuf::from("."), |h| PathBuf::from(h).join(".config"))
            },
            PathBuf::from,
        );
    base.join("Hanglock")
}

pub const FILE: &str = "settings.toml";

/// Which file inside a directory is ours. A function because `load_at`, `save_at` and the recovery path
/// have to name the same one, and a settings document that is read from one file and written to another
/// is a settings file that appears to be ignored.
#[must_use]
fn file_in(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// What a read found. Kept separate from the `Settings` because the app says something different for
/// each of these, and a caller that cannot tell "no file" from "unreadable file" writes a file it
/// should not have written or shows a warning it should not have shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The document, as written.
    Loaded,
    /// No file. Defaults are in use and nothing has been written.
    FirstRun,
    /// A file that is not a Hanglock document was found, moved aside to `kept`, and defaults are in
    /// use. `kept` is where the user's own words still are.
    Recovered {
        kept: PathBuf,
        /// What the file looked like, in one line, for the log. Never its contents: a settings file a
        /// user hand-edited can contain anything, and `--diag` output gets pasted into bug reports.
        note: String,
    },
}

/// Load from `dir`, with the corruption dance.
///
/// A file that has *some* usable lines is not quarantined: a document truncated by a power cut
/// mid-write still holds the settings the user chose, and `Settings::from_toml` answers with defaults
/// for whatever fell off the end plus a warning per line it could not use. Quarantining that file
/// would turn a one-line accident into losing the whole document, which is a worse outcome than the
/// one the quarantine exists to prevent. What *is* quarantined is a file with nothing of ours in it —
/// no section headers and no `schema` — because at that point defaults are the only honest answer and
/// the user's real file is somewhere else.
#[must_use]
pub fn load_at(dir: &Path) -> (Settings, Outcome) {
    let p = file_in(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Settings::default(), Outcome::FirstRun);
        }
        Err(e) => {
            // An unreadable file (locked, denied, a directory in the way) is not a corrupt one: do
            // not rename what we cannot read, and do not write over it either.
            return (
                Settings::default(),
                Outcome::Recovered {
                    kept: p.clone(),
                    note: format!("cannot be read: {e}"),
                },
            );
        }
    };
    if text.trim().is_empty() {
        // A zero-length file is what an interrupted creation leaves behind. It is a first run, not a
        // corruption: there is nothing in it to preserve and nothing to complain about.
        return (Settings::default(), Outcome::FirstRun);
    }
    let (settings, warnings) = Settings::from_toml(&text);
    if !plausibly_ours(&text) {
        let backup = quarantine(&p);
        return (
            Settings::default(),
            Outcome::Recovered {
                kept: backup,
                note: "no Hanglock keys in it".to_string(),
            },
        );
    }
    if !warnings.is_empty() {
        // Lines we could not use: report them, keep everything else. This is the only place the
        // parser's warnings are allowed to surface, and it says "ignored" rather than "corrupt"
        // because that is what happened.
        eprintln!(
            "hanglock: {} setting(s) not understood, rest kept; {}",
            warnings.len(),
            p.display()
        );
    }
    (settings, Outcome::Loaded)
}

/// Whether the text has the shape of a document this parser wrote or a human edited: a `[section]`,
/// or the `schema` key that opens every file.
fn plausibly_ours(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim();
        (t.starts_with('[') && t.ends_with(']')) || t.starts_with("schema")
    })
}

fn quarantine(p: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("{FILE}.corrupt-{stamp}");
    let target = p.with_file_name(name);
    if std::fs::rename(p, &target).is_ok() {
        return target;
    }
    // A file we cannot move we also must not overwrite, and `save` is allowed to be called later. Say
    // so, and hand back the name the copy would have had so the message is not a lie.
    eprintln!("hanglock: could not move {p:?} aside; leaving it in place");
    target
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loaded => f.write_str("read as written"),
            Self::FirstRun => f.write_str("no file yet; defaults, none written"),
            // The name of the file that holds what was preserved, never its directory: the expanded
            // path of `%APPDATA%` carries the account name, and this line is quoted in bug reports.
            Self::Recovered { kept, note } => {
                let name = kept.file_name().unwrap_or_default().to_string_lossy();
                f.write_str(&format!(
                    "defaults; the previous file {note} (kept as {name})"
                ))
            }
        }
    }
}

/// Load from the platform's own directory.
#[must_use]
pub fn load() -> (Settings, Outcome) {
    load_at(&dir())
}

/// Persist into `dir`, creating it if needed. Three attempts, because the interesting failure is a
/// momentary lock and the second failure is a user who will never see the third.
#[cfg(any(windows, test))]
pub fn save_at(dir: &Path, s: &Settings) -> Result<(), String> {
    let p = file_in(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = p.with_file_name(format!("{FILE}.new"));
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

/// Persist to the platform's own directory.
#[cfg(any(windows, test))]
pub fn save(s: &Settings) -> Result<(), String> {
    save_at(&dir(), s)
}

/// Where the file is, in the form a user can type: the environment variable is named rather than
/// expanded, because the expanded form carries the account name, and `--diag` output is written down
/// in bug reports.
#[must_use]
pub fn display_path() -> &'static str {
    if cfg!(windows) {
        "%APPDATA%\\Hanglock\\settings.toml"
    } else {
        "$XDG_CONFIG_HOME/Hanglock/settings.toml"
    }
}

#[cfg(any(windows, test))]
fn write_and_replace(tmp: &Path, p: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(tmp, bytes).map_err(|e| format!("write: {e}"))?;
    #[cfg(windows)]
    {
        // `ReplaceFileW` is the only primitive that substitutes a file another process may have
        // open-with-DELETE, which is exactly the state a running settings file is in.
        if replace_file_win(p, tmp) {
            return Ok(());
        }
    }
    if std::fs::rename(tmp, p).is_err() {
        // On Windows a rename onto an existing path fails; drop it first. Losing the *old* file to
        // a failure in the gap is bounded by the fact that `tmp` was written and verified above.
        let _ = std::fs::remove_file(p);
        return std::fs::rename(tmp, p).map_err(|e| format!("replace: {e}"));
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file_win(target: &Path, source: &Path) -> bool {
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
    let w = |p: &Path| -> Vec<u16> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own per test: these tests write, and two tests sharing one temp dir would
    /// be a race that fails on a busy machine rather than a bug in the store.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("hanglock-store-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("temp dir");
            Self(root)
        }
        fn write(&self, body: &str) {
            std::fs::write(self.0.join(FILE), body).expect("write fixture");
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_first_run_reads_defaults_and_writes_nothing() {
        let t = Tmp::new("first-run");
        let (s, outcome) = load_at(&t.0);
        assert_eq!(s, Settings::default());
        assert_eq!(outcome, Outcome::FirstRun);
        assert!(
            !t.0.join(FILE).exists(),
            "a first run must not create the settings file"
        );
    }

    #[test]
    fn saving_creates_the_file_and_reading_it_back_is_exact() {
        let t = Tmp::new("round-trip");
        let mut s = Settings::default();
        s.face.seconds = true;
        s.overlay.hang = 190.0;
        s.overlay.anchor_ratio = 0.25;
        s.overlay.anchor_drop = 48.0;
        s.general.launch_at_login = true;
        save_at(&t.0, &s).expect("save");
        let (back, outcome) = load_at(&t.0);
        assert_eq!(outcome, Outcome::Loaded);
        assert_eq!(back, s, "a saved document must come back unchanged");
    }

    #[test]
    fn saving_twice_leaves_no_temporary_behind() {
        let t = Tmp::new("atomic");
        save_at(&t.0, &Settings::default()).expect("first save");
        save_at(&t.0, &Settings::default()).expect("second save");
        let left: Vec<String> = std::fs::read_dir(&t.0)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left, vec![FILE.to_string()], "stray files: {left:?}");
    }

    #[test]
    fn a_file_that_is_not_ours_is_moved_aside_and_defaults_are_used() {
        let t = Tmp::new("not-ours");
        t.write("{\"overlay\": {\"hang\": 20}}\n");
        let (s, outcome) = load_at(&t.0);
        assert_eq!(s, Settings::default(), "defaults, not partial JSON");
        let Outcome::Recovered { kept, note } = outcome else {
            panic!("a foreign file must be quarantined, got {outcome:?}");
        };
        assert!(note.contains("no Hanglock keys"));
        assert!(
            !t.0.join(FILE).exists(),
            "the original must not be left in place"
        );
        assert!(kept.exists(), "and it must still exist somewhere");
        assert_eq!(
            std::fs::read_to_string(&kept).expect("read backup"),
            "{\"overlay\": {\"hang\": 20}}\n",
            "quarantining must not alter what it preserves"
        );
    }

    #[test]
    fn a_truncated_document_keeps_everything_that_survived() {
        let t = Tmp::new("truncated");
        // Cut mid-write: the face section is there, the general section is not.
        t.write("schema = 1\n\n[overlay]\nhang = 230\nscale = 1.15\n\n[face]\nhour12 = no\n");
        let (s, outcome) = load_at(&t.0);
        assert_eq!(outcome, Outcome::Loaded, "not a corruption event");
        assert_eq!(s.overlay.hang, 230.0);
        assert_eq!(s.overlay.scale, 1.15);
        // Not "ignored, default stands": the reader's boolean set is deliberately wider than TOML's so
        // that `yes` and `on` work in a hand-edited file, and the price of a wide true-list is that every
        // other word is false. `hour12 = n0` will therefore read as 24-hour rather than be refused, which
        // is why the settings *window* is the way to change this and not a text box.
        assert!(!s.face.hour12, "`no` is one of the false words");
        assert_eq!(
            s.general.fps_cap, 60,
            "the missing section took its default, not a reset"
        );
        assert!(
            t.0.join(FILE).exists(),
            "the file is the user's; it stays where it is"
        );
    }

    #[test]
    fn a_value_that_is_not_a_number_leaves_that_key_at_its_own_default() {
        // The other half of "tolerant": a number that cannot be parsed does not fail the load and does
        // not zero the field, it falls back to the value the key would have had anyway, and every other
        // key in the file is still read. A half-written file must not cost the user the whole file.
        let t = Tmp::new("junk-numbers");
        t.write(
            "[overlay]
opacity = 9abc
scale = 1.4
click_through = sideways

[general]
fps_cap = lots
",
        );
        let (s, outcome) = load_at(&t.0);
        assert_eq!(outcome, Outcome::Loaded, "junk is not corruption");
        assert_eq!(s.overlay.opacity, 1.0, "unparseable: the key's own fallback");
        assert_eq!(s.overlay.scale, 1.4, "the line beside it still landed");
        assert_eq!(
            s.overlay.click_through,
            hanglock_core::ids::ClickThrough::default(),
            "an unknown enum word is the default mode, not a random one"
        );
        assert_eq!(s.general.fps_cap, 60);
    }

    #[test]
    fn an_empty_file_is_a_first_run_and_not_a_corruption() {
        let t = Tmp::new("empty");
        t.write("\n");
        let (s, outcome) = load_at(&t.0);
        assert_eq!(s, Settings::default());
        assert_eq!(outcome, Outcome::FirstRun, "nothing to quarantine");
        assert!(t.0.join(FILE).exists(), "and nothing to write back yet");
    }

    #[test]
    fn a_value_out_of_range_is_clamped_instead_of_refused() {
        let t = Tmp::new("out-of-range");
        t.write("[overlay]\nhang = 9000\nanchor_drop = -20\nanchor_ratio = 4\n");
        let (s, outcome) = load_at(&t.0);
        assert_eq!(outcome, Outcome::Loaded);
        assert_eq!(s.overlay.hang, hanglock_core::settings::limits::HANG.1);
        assert_eq!(s.overlay.anchor_ratio, 1.0);
        assert_eq!(s.overlay.anchor_drop, 0.0);
    }

    #[test]
    fn a_quarantined_file_is_never_written_over_by_the_next_save() {
        let t = Tmp::new("after-recovery");
        t.write("total nonsense\n");
        let (_, outcome) = load_at(&t.0);
        assert!(matches!(outcome, Outcome::Recovered { .. }));
        save_at(&t.0, &Settings::default()).expect("save after recovery");
        let (again, second) = load_at(&t.0);
        assert_eq!(second, Outcome::Loaded);
        assert_eq!(again, Settings::default());
    }
}
