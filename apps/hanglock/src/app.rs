//! The Windows adapter: the model's `Action`s translated into calls on a `Host`, and Win32 events
//! translated back into model calls.
//!
//! There is deliberately no logic in here beyond that translation. "What does a wheel notch do",
//! "when should the frame clock stop", "which rectangle is dirty" and "what does a click on the
//! `Cord` row mean" all live in `model` and `hanglock-platform::panel`, which run and are tested
//! without a desktop. When a bug report says *the clock drifted after a monitor change*, the fix
//! goes in `model` and gains a test; the adapter stays boring, which is what makes ~250 lines of glue
//! around ~1.5k lines of `unsafe` an acceptable trade.
//!
//! ## Three surfaces, one writer
//!
//! The tray menu, the settings window and the card's own context menu all end up in
//! [`hanglock_platform::Command`], and all of them read their tickmarks from a snapshot built from
//! `self.model.settings`. That is the whole of why they cannot disagree: no surface holds a value, and
//! the two places that *do* own something — the window styles and the registry — are written from here
//! and nowhere else.
//!
//! ## Two settings the host owns, not the model
//!
//! `topmost` and `click_through` are window *styles*, so the adapter applies them whenever a present
//! happens and they differ from what is on the HWND. Doing it on present rather than on every toggle
//! means a style write can never be missed after a re-create, and doing it only on change means the
//! window server is not poked every frame — the lesson the reference project learned the hard way,
//! where an unconditional per-frame `ignoresMouseEvents` write kept a settled overlay measurably busy.

#![cfg(windows)]

use crate::model::{Action, CursorKind, Model, State};
use crate::store;
use hanglock_core::settings::Settings;
use hanglock_platform::panel::{self, Group, RowId, Step};
use hanglock_platform::{Command, Dialogs, Input, OverlayHost, SystemEvent};
use hanglock_win::tray::MenuState;
use hanglock_win::window::{AppHook, HitShape, Host, OverlayConfig};
use std::process::ExitCode;

/// The tooltip, in two states. A hidden clock with no visible affordance is the one way this app can
/// strand someone, so the icon says what it will do.
const TIP_SHOWN: &str = "Hanglock — right-click for options";
const TIP_HIDDEN: &str = "Hanglock — clock hidden";

pub struct Adapter {
    model: Model,
    /// What has already been pushed to the HWND, so style writes happen on change only.
    pushed_topmost: Option<bool>,
    /// What the tray icon already says. Cached as the two facts the sentence is built from, not as the
    /// string, because `apply` runs on every frame and a tooltip rebuild would turn a settled clock back
    /// into an allocating one.
    pushed_tip: Option<(bool, Option<&'static str>)>,
    primary: u32,
}

pub fn run(settings: Settings) -> ExitCode {
    let adapter = Adapter {
        model: Model::new(settings),
        pushed_topmost: None,
        pushed_tip: None,
        primary: 0,
    };
    // A placeholder size; `on_ready` immediately replaces it with the real swept box. Starting with
    // something plausible rather than 0x0 avoids a first-frame resize of a visible window.
    let cfg = OverlayConfig {
        frame: hanglock_core::placement::Rect::new(0.0, 0.0, 640.0, 320.0),
        scale: 1.0,
        title: "Hanglock",
        tooltip: TIP_SHOWN,
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
        let mut autostart: Option<bool> = None;
        let mut dialogs = 0u8;
        for a in actions {
            match a {
                Action::PresentFull => present = Some(None),
                Action::PresentRect(r) => present = Some(Some(r)),
                Action::Move(f) => moved = Some(f),
                Action::Relayout => relayout = true,
                Action::Save => dirty = true,
                Action::ApplyAutostart(wanted) => autostart = Some(wanted),
                // A bit, not a bool, because "open the window" and "show the About box" can arrive in
                // one batch and neither cancels the other.
                Action::OpenSettings => dialogs |= 1,
                Action::About => dialogs |= 2,
                Action::Quit(code) => host.quit(code),
                Action::None => {}
            }
        }
        if let Some(wanted) = autostart {
            self.write_autostart(wanted);
            dirty = true;
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
        self.sync_tooltip(host);
        if dialogs & 1 != 0 {
            let groups = self.groups(host);
            host.show_settings(groups);
        }
        if dialogs & 2 != 0 {
            let text = self.about_text();
            host.about(&text);
        }
    }

    /// The rows for the settings window, derived from the document and the live display list. One
    /// expression, used by both the tray's `Settings…` and the window's own refresh, so the two can
    /// only ever show the same form.
    fn groups(&self, host: &Host) -> Vec<Group> {
        panel::form(&self.model.settings, &host.monitors())
    }

    /// What the registry will do at the next sign-in, written back into the document if it disagrees
    /// with what the user asked for. The registry answers rather than the write's return code, because
    /// "the value is there but Task Manager has disabled it" is a state the checkbox has to show.
    fn write_autostart(&mut self, wanted: bool) {
        let exe = match std::env::current_exe() {
            Ok(p) => p.display().to_string(),
            Err(_) => {
                eprintln!("hanglock: cannot find my own executable, so autostart is unchanged");
                return;
            }
        };
        let got = hanglock_win::autostart::apply(&exe, wanted);
        if got != self.model.settings.general.launch_at_login {
            self.model.settings.general.launch_at_login = got;
        }
    }

    fn about_text(&self) -> String {
        let m = &self.model;
        let s = &m.settings;
        let a = m.anchor();
        format!(
            "Hanglock {}\n\
             A clock that hangs on a cord in front of your desktop.\n\
             \n\
             Time: {}-hour{}, {}\n\
             Mouse: {}\n\
             Hang: {} px from the top edge, {}% across\n\
             Always on top: {}\n\
             Settings file: {}\n\
             \n\
             Drag the card to swing it. Hold Alt and drag to move where it hangs from.\n\
             The tray icon holds every setting, including Reset position.",
            env!("CARGO_PKG_VERSION"),
            if s.face.hour12 { "12" } else { "24" },
            if s.face.hour12 && s.face.meridiem {
                " with AM / PM"
            } else {
                ""
            },
            if s.face.seconds {
                "seconds shown"
            } else {
                "no seconds"
            },
            s.overlay.click_through.label(),
            a.drop.round() as i64,
            (a.ratio * 100.0).round() as i64,
            if s.overlay.topmost { "yes" } else { "no" },
            store::display_path(),
        )
    }

    fn sync_styles(&mut self, host: &mut Host) {
        let topmost = self.model.settings.overlay.topmost;
        if self.pushed_topmost != Some(topmost) {
            host.set_topmost(topmost);
            self.pushed_topmost = Some(topmost);
        }
        // The host keeps its own last-written mode, so this costs nothing when nothing changed — which
        // is every frame but the one after a click.
        host.set_input_mode(self.model.settings.overlay.click_through);
        host.show(self.model.state != State::Hidden);
        host.set_rate(if self.model.wants_ticks() {
            self.model.preferred_rate()
        } else {
            0
        });
        host.sync_tick_timer();
    }

    fn sync_tooltip(&mut self, host: &mut Host) {
        let hidden = self.model.state == State::Hidden;
        let notice = self.model.notice;
        if self.pushed_tip == Some((hidden, notice)) {
            return;
        }
        self.pushed_tip = Some((hidden, notice));
        let base = if hidden { TIP_HIDDEN } else { TIP_SHOWN };
        // A refusal is worth reading where the user was just looking. The tray icon is the only
        // surface left when the overlay is click-through, so it carries the sentence.
        match self.model.notice {
            Some(what) => host.set_tooltip(&format!("{base} — {what}")),
            None => host.set_tooltip(base),
        }
    }

    fn menu_state(&self, host: &mut Host) -> MenuState {
        let s = &self.model.settings;
        let monitors = host.monitors();
        let names = monitors
            .iter()
            .map(|m| {
                let b = m.bounds_logical();
                (
                    m.index,
                    format!(
                        "{}: {} x {} at {}%",
                        m.index + 1,
                        b.w(),
                        b.h(),
                        (m.scale * 100.0).round() as i64
                    ),
                )
            })
            .collect();
        MenuState {
            visible: self.model.state != State::Hidden,
            topmost: s.overlay.topmost,
            seconds: s.face.seconds,
            hour12: s.face.hour12,
            meridiem: s.face.meridiem,
            posture: s.face.posture,
            click_through: s.overlay.click_through,
            launch_at_login: s.general.launch_at_login,
            monitors: names,
            monitor: s.overlay.monitor_index,
            notice: self.model.notice,
        }
    }
}

impl AppHook for Adapter {
    fn on_ready(&mut self, host: &mut Host) {
        let monitors = host.monitors();
        self.primary = monitors
            .iter()
            .find_map(|m| m.primary.then_some(m.index))
            .unwrap_or(0);
        let tray = host.tray_installed();
        let at_login = hanglock_win::autostart::is_enabled();
        let actions = self.model.on_ready(tray, at_login);
        self.apply(host, actions);
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
        let actions = self.model.on_input(input);
        // The model drops a refusal as soon as the overlay answers the mouse again, and `apply`
        // pushes that change to the tooltip: the sentence disappears when it stops being true.
        self.apply(host, actions);
    }

    fn on_command(&mut self, host: &mut Host, cmd: Command) {
        let actions = self.model.on_command(cmd);
        self.apply(host, actions);
        if host.settings_open() {
            let groups = self.groups(host);
            host.sync_settings(&groups);
        }
    }

    fn panel_command(&mut self, host: &mut Host, id: RowId, step: Step) {
        // The window's answer is the tray's answer: one row and one step become a `Command`, which the
        // model then decides on. Nothing in this path knows about `Settings` except `panel::change`,
        // which is the same function the model's own tests call.
        let Some(cmd) = panel::change(&self.model.settings, id, step) else {
            return;
        };
        let actions = self.model.on_command(cmd);
        self.apply(host, actions);
        let groups = self.groups(host);
        host.sync_settings(&groups);
    }

    fn on_system(&mut self, host: &mut Host, event: SystemEvent) {
        let actions = self.model.on_system(event);
        self.apply(host, actions);
        if matches!(
            event,
            SystemEvent::DisplaysChanged | SystemEvent::DpiChanged
        ) {
            // The display list is what the settings window's monitor row and the tray's submenu are
            // built from, so a hotplug has to reach both.
            if host.settings_open() {
                let groups = self.groups(host);
                host.sync_settings(&groups);
            }
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
            whole_window: r.whole_window,
            pad: 0.0,
            anchor: r.anchor,
            anchor_radius: r.anchor_radius,
        }
    }

    fn cursor(&self) -> hanglock_win::window::Cursor {
        match self.model.cursor() {
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

    fn menu_state(&self, host: &mut Host) -> MenuState {
        Adapter::menu_state(self, host)
    }
}
