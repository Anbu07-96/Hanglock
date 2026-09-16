//! The Windows adapter: the model's `Action`s translated into calls on a `Host`, and Win32 events
//! translated back into model calls.
//!
//! There is deliberately no logic in here beyond that translation. "What does a wheel notch do",
//! "when should the frame clock stop", and "which rectangle is dirty" all live in `model`, which
//! runs and is tested without a desktop. When a bug report says *the clock drifted after a monitor
//! change*, the fix goes in `model` and gains a test; the adapter stays boring, which is what makes
//! ~150 lines of glue around ~1.2k lines of `unsafe` an acceptable trade.
//!
//! ## Two settings the host owns, not the model
//!
//! `topmost` and `click_through = "always"` are window *styles*, so the adapter applies them to the
//! HWND whenever a present happens and they differ from what was last pushed. Doing it on present
//! rather than on every toggle means a style write can never be missed after a re-create, and doing
//! it only on change means the window server is not poked every frame — the lesson the reference
//! project learned the hard way, where an unconditional per-frame `ignoresMouseEvents` write kept a
//! settled overlay measurably busy.

#![cfg(windows)]

use crate::model::{Action, CursorKind, Model, State};
use crate::store;
use hanglock_core::ids::ClickThrough;
use hanglock_core::settings::Settings;
use hanglock_platform::{Command, Input, SystemEvent};
use hanglock_win::window::{AppHook, HitShape, Host, OverlayConfig};
use std::process::ExitCode;

pub struct Adapter {
    model: Model,
    /// What has already been pushed to the HWND, so style writes happen on change only.
    pushed_topmost: Option<bool>,
    pushed_ignore: Option<bool>,
    primary: u32,
}

pub fn run(settings: Settings) -> ExitCode {
    let mut adapter = Adapter {
        model: Model::new(settings),
        pushed_topmost: None,
        pushed_ignore: None,
        primary: 0,
    };
    // A placeholder size; `on_ready` immediately replaces it with the real swept box. Starting with
    // something plausible rather than 0x0 avoids a first-frame resize of a visible window.
    let cfg = OverlayConfig {
        frame: hanglock_core::placement::Rect::new(0.0, 0.0, 640.0, 320.0),
        scale: 1.0,
        title: "Hanglock",
        tooltip: "Hanglock — right-click for options",
        topmost: adapter.model.settings.overlay.topmost,
    };
    let code = hanglock_win::run(adapter, cfg);
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(code as u8)
    }
}

impl Adapter {
    fn apply(&mut self, host: &mut Host, actions: Vec<Action>) {
        let mut relayout = false;
        let mut present: Option<Option<hanglock_core::placement::Rect>> = None;
        let mut moved: Option<hanglock_core::placement::Rect> = None;
        let mut dirty = false;
        for a in actions {
            match a {
                Action::PresentFull => present = Some(None),
                Action::PresentRect(r) => present = Some(Some(r)),
                Action::Move(f) => moved = Some(f),
                Action::Relayout => relayout = true,
                Action::Save => dirty = true,
                Action::Quit(code) => host.quit(code),
                Action::None => {}
            }
        }
        if dirty {
            if let Err(e) = store::save(&self.model.settings) {
                // A settings file that will not write is worth printing but never worth exiting
                // over: the clock on screen is the product, and it is still correct.
                eprintln!("hanglock: could not save settings: {e}");
            }
        }
        if relayout {
            let monitors = host.monitors();
            let frame = self.model.relayout(&monitors, self.primary);
            host.set_frame(frame);
            self.model.paint();
            present = Some(None);
        } else if let Some(f) = moved {
            host.set_frame(f);
        }
        // Style writes after geometry, because a resize can reset an extended style on some driver
        // paths and pushing the style second means the last word is ours.
        self.sync_styles(host);
        if let Some(rect) = present {
            let px = &self.model.canvas.px;
            host.present(px, rect);
            if self.model.wants_ticks() {
                host.set_rate(self.model.preferred_rate());
                host.sync_tick_timer();
            }
        }
    }

    fn sync_styles(&mut self, host: &mut Host) {
        let topmost = self.model.settings.overlay.topmost;
        if self.pushed_topmost != Some(topmost) {
            host.set_topmost(topmost);
            self.pushed_topmost = Some(topmost);
        }
        let ignore = self.model.settings.overlay.click_through == ClickThrough::Always;
        if self.pushed_ignore != Some(ignore) {
            host.set_ignore_input(ignore);
            self.pushed_ignore = Some(ignore);
        }
        host.show(self.model.state != State::Hidden);
        host.set_rate(if self.model.wants_ticks() {
            self.model.preferred_rate()
        } else {
            0
        });
        host.sync_tick_timer();
    }
}

impl AppHook for Adapter {
    fn on_ready(&mut self, host: &mut Host) {
        let monitors = host.monitors();
        self.primary = monitors
            .iter()
            .find(|m| m.primary)
            .map(|m| m.index)
            .unwrap_or(0);
        let frame = self.model.relayout(&monitors, self.primary);
        host.set_frame(frame);
        self.model.paint();
        host.present(&self.model.canvas.px, None);
        self.sync_styles(host);
        let (visible, ..) = self.model.menu_state();
        host.set_tooltip(if visible {
            "Hanglock — right-click for options"
        } else {
            "Hanglock — clock hidden"
        });
    }

    fn on_frame(&mut self, host: &mut Host, dt: f64) {
        let actions = self.model.on_frame(dt);
        self.apply(host, actions);
    }

    fn on_second(&mut self, host: &mut Host) {
        let fields = host.local_fields();
        let actions = self.model.on_second(fields);
        self.apply(host, actions);
    }

    fn on_input(&mut self, host: &mut Host, input: Input) {
        // Alt+press is the re-anchor gesture in the design; the ring is the pointerless version of
        // the same thing and is what v0.1 ships, so no key state is needed here.
        let actions = self.model.on_input(input);
        self.apply(host, actions);
    }

    fn on_command(&mut self, host: &mut Host, cmd: Command) {
        let actions = self.model.on_command(cmd);
        self.apply(host, actions);
    }

    fn on_system(&mut self, host: &mut Host, event: SystemEvent) {
        let actions = self.model.on_system(event);
        self.apply(host, actions);
        if matches!(
            event,
            SystemEvent::DisplaysChanged | SystemEvent::DpiChanged
        ) {
            let (visible, ..) = self.model.menu_state();
            host.set_tooltip(if visible {
                "Hanglock — right-click for options"
            } else {
                "Hanglock — clock hidden"
            });
        }
    }

    fn hit_shape(&self) -> HitShape {
        let r = self.model.hit_regions();
        HitShape {
            centre: r.plate_centre,
            hw: r.plate_hw,
            hh: r.plate_hh,
            theta: r.theta,
            interactive: r.interactive,
            pad: 0.0,
            anchor: r.anchor,
            anchor_radius: r.anchor_radius,
        }
    }

    fn cursor(&self) -> hanglock_win::window::Cursor {
        match self.model.cursor() {
            CursorKind::Arrow => hanglock_win::window::Cursor::Arrow,
            CursorKind::Grab => hanglock_win::window::Cursor::Grab,
            CursorKind::Grabbing => hanglock_win::window::Cursor::Grabbing,
            CursorKind::Move => hanglock_win::window::Cursor::Move,
        }
    }

    fn wants_ticks(&self) -> bool {
        self.model.wants_ticks()
    }

    fn preferred_rate(&self) -> u32 {
        self.model.preferred_rate()
    }

    fn menu_state(&self) -> hanglock_win::tray::MenuState {
        let (visible, topmost, seconds, hour12, posture) = self.model.menu_state();
        hanglock_win::tray::MenuState {
            visible,
            topmost,
            seconds,
            hour12,
            posture,
        }
    }
}
