//! Hanglock: a working clock, hanging from the top of the desktop.
//!
//! The binary is deliberately thin. Everything with a decision in it lives in [`model`], which is
//! platform-free and therefore testable without a desktop; [`store`] owns the file; and on Windows,
//! [`app`] adapts the model to the backend. The one place this crate touches the OS directly is
//! `store`'s use of `ReplaceFileW`, because an atomic replace of a file another process may have open
//! is a thing only the OS can do; every other line here is safe Rust.
//!
//! ## Command line
//!
//! ```text
//! hanglock                     run the overlay
//! hanglock --dump-scene PATH   paint one settled frame to PATH.png and exit   (any OS)
//! hanglock --bench N           paint N frames, print ns/frame and present bytes (any OS)
//! hanglock --diag              print settings, displays, placement, budgets   (any OS)
//! hanglock --background        no-op marker used by the autostart entry
//! ```
//!
//! The non-GUI modes exist so the two claims this project makes about itself — what it looks like
//! and what it costs — can be checked on a machine with no display, including CI. They take no
//! arguments beyond a path or a count, print everything they used, and never touch the settings file.

mod app;
// On non-Windows builds the interaction surface (`on_input`, commands, cursors) has no driver —
// the adapter that calls it is the Windows-only half — but the model must stay whole so the same
// code is what ships and what is tested. Alive by contract, not by local callers.
#[cfg_attr(not(windows), allow(dead_code))]
mod model;
mod store;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (settings, found) = store::load();
    if matches!(found, store::Outcome::Recovered { .. }) {
        // Once, on stderr, in the same words `--diag` prints: a user who has just lost a settings
        // document needs to be able to go and look for it, not to be told it is gone.
        eprintln!("hanglock: {found}");
    }

    if let Some(path) = flag_value(&args, "--dump-scene") {
        return match model::dump_scene(&settings, &path) {
            Ok(bytes) => {
                println!("wrote {path} ({bytes} bytes)");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("hanglock: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(n) = flag_value(&args, "--bench").and_then(|s| s.parse::<usize>().ok()) {
        model::bench(&settings, n);
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--bench") {
        model::bench(&settings, 400);
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--diag") {
        #[cfg(windows)]
        let monitors = hanglock_win::displays::monitors();
        #[cfg(not(windows))]
        let monitors = Vec::new();
        println!("{}", model::diag(&settings, &monitors, &found));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", help_text());
        return ExitCode::SUCCESS;
    }

    #[cfg(windows)]
    {
        app::run(settings)
    }
    #[cfg(not(windows))]
    {
        let _ = settings;
        eprintln!(
            "hanglock: this build has no overlay backend for this OS.\n\
             The model, painter, and the `--dump-scene` / `--bench` modes work here; the windowed\n\
             app is Windows-only in v0.1 (see docs/roadmap.md, Phase 7 for macOS)."
        );
        ExitCode::FAILURE
    }
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    // `--bench` with no value is a valid spelling, so a following flag is not our value.
    match args.get(i + 1) {
        Some(v) if !v.starts_with('-') => Some(v.clone()),
        _ => None,
    }
}

fn help_text() -> &'static str {
    "hanglock - a clock hanging from the top of the desktop

  (no arguments)         show the clock; controls are in the tray and on right-click
  --dump-scene FILE.png  render one frame and exit (works headless, for review)
  --bench [n]            paint n frames (default 400) and print per-frame costs
  --diag                 print settings, placement and measured budgets
  --background           silent start, used by the autostart entry
"
}
