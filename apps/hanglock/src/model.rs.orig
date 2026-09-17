//! Everything Hanglock does, minus the parts that require a desktop.
//!
//! Deliberately the biggest file in the app, because it is the only place where the interaction
//! design exists *once*: the Windows adapter in `app.rs` translates messages and pushes pixels, and
//! has no opinions. That split is what makes the interesting behaviour — when to wake, when to
//! sleep, what counts as dirty, what a wheel notch does — testable on any OS, so `cargo test` on a
//! Linux runner proves things about how the clock feels.
//!
//! The rule the state machine is built around: **nothing runs unless something changed, and nothing
//! is presented that a person cannot see.** A settled clock steps no physics and copies only the
//! digits' rectangle, once a second, because that is the only thing that changed.

use hanglock_core::anchor::{self, Anchor};
use hanglock_core::clock::format::{Civil, FaceOptions, FaceText};
use hanglock_core::ids::{ClickThrough, PostureKind};
use hanglock_core::placement::{place, Monitor, Rect};
use hanglock_core::rope::config::Posture;
use hanglock_core::rope::Rope;
use hanglock_core::scene::Scene;
use hanglock_core::settings::Settings;
use hanglock_core::vec2::Vec2;
use hanglock_platform::panel::HANG_STEPS;
use hanglock_platform::Command;
use hanglock_platform::Input;
use hanglock_render::paint::text_bounds;
use hanglock_render::{Canvas, Theme};

/// The three states the app can be in. `Settled` still repaints once a second: a clock's steady
/// state is a changing picture, and pretending otherwise would make it show the wrong time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No window. Nothing runs.
    Hidden,
    /// Window up, rope asleep. Only the 1 Hz digit timer is armed.
    Settled,
    /// Rope stepping; the tick timer is on.
    Swinging,
}

/// What the caller (the platform adapter) is asked to do, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Repaint and present the whole surface.
    PresentFull,
    /// Repaint and present only `rect`; the rest of the buffer is unchanged pixel-for-pixel.
    PresentRect(Rect),
    /// Move the window (device px). Presented after, with the new size.
    Move(Rect),
    /// Re-run placement from the monitor list, then present fully.
    Relayout,
    /// Persist settings.
    Save,
    /// Make the OS agree with `launch_at_login`: write or remove the HKCU Run entry. The model owns
    /// the wish, the platform owns the registry, and this is the only way between them.
    ApplyAutostart(bool),
    /// Bring up the settings window with the rows `panel::form` derives from the model.
    OpenSettings,
    /// Show the About box.
    About,
    /// Stop the message loop.
    Quit(i32),
    /// Nothing.
    None,
}

/// The cursor the adapter should show. Named here rather than imported from the backend so the model
/// can be tested without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Grab,
    Grabbing,
    /// Over the ring: dragging this moves the whole clock.
    Move,
}

/// The two shapes the window is interactive over: the plate, and the ring it hangs from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitRegions {
    pub plate_centre: Vec2,
    pub plate_hw: f64,
    pub plate_hh: f64,
    pub theta: f64,
    pub anchor: Vec2,
    pub anchor_radius: f64,
    pub interactive: bool,
    /// Whether the *window's* rectangle answers the mouse, not just the object inside it
    /// (`click_through = "solid"`). The adapter folds this into `WM_NCHITTEST` before it consults the
    /// shapes below, so the two modes differ by exactly one answer and nothing else.
    pub whole_window: bool,
}

impl HitRegions {
    // The three predicates below are the model's own hit maths, exercised by its tests; the shipped
    // answer is the backend's shape, which they were written to pin down. Not dead so much as
    // test-facing: cfg(test) says so without an allow.
    #[cfg(test)]
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        if self.whole_window {
            return true;
        }
        if !self.interactive {
            return false;
        }
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let (dx, dy) = (p.x - self.plate_centre.x, p.y - self.plate_centre.y);
        let lx = dx * cs + dy * sn;
        let ly = -dx * sn + dy * cs;
        if lx.abs() <= self.plate_hw && ly.abs() <= self.plate_hh {
            return true;
        }
        // The ring is a small square rather than a circle: at 16 px it is indistinguishable, and a
        // square is cheaper to reason about when someone asks why the corner of it is grabbable.
        (p.x - self.anchor.x).abs() <= self.anchor_radius
            && (p.y - self.anchor.y).abs() <= self.anchor_radius
    }

    #[cfg(test)]
    #[must_use]
    pub fn on_ring(&self, p: Vec2) -> bool {
        if !self.interactive {
            return false;
        }
        let (dx, dy) = ((p.x - self.anchor.x).abs(), (p.y - self.anchor.y).abs());
        (dx <= self.anchor_radius && dy <= self.anchor_radius) && !self.on_plate(p)
    }

    #[cfg(test)]
    #[must_use]
    pub fn on_plate(&self, p: Vec2) -> bool {
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let (dx, dy) = (p.x - self.plate_centre.x, p.y - self.plate_centre.y);
        ((dx * cs + dy * sn).abs() <= self.plate_hw)
            && ((-dx * sn + dy * cs).abs() <= self.plate_hh)
    }
}

pub struct Model {
    pub settings: Settings,
    pub rope: Rope,
    pub canvas: Canvas,
    pub theme: Theme,
    pub state: State,
    pub text: FaceText,
    pub layout_frame: Rect,
    pub layout_scale: f64,
    pub monitor: Option<Monitor>,
    /// The hang point within the frame, in device px, as the placement maths placed it.
    pub layout_anchor: Vec2,
    /// Whether a tray icon is up. `click_through = "always"` is refused without one, because a clock
    /// that answers no clicks and has no menu is a clock that has to be found in a file.
    pub tray_ok: bool,
    /// The digits' box from the *previous* present. A present must cover both boxes: the text got
    /// narrower (`10:42` to `9:42`), and presenting only the new one would leave a fragment of the
    /// old `1` on screen for the next fifty-nine seconds.
    last_text_bounds: Rect,
    reposition: Option<Reposition>,
    pub hover_ring: bool,
    pub saved: bool,
    /// The last thing the app refused to do, in words, for the tray tooltip and `--diag`. A refusal
    /// that is silent is a refusal a user attributes to a bug.
    pub notice: Option<&'static str>,
    pub counters: Counters,
    pub seconds_since_last_text: f64,
}

/// Shown when the clock is asked to go fully click-through with no tray to come back through. Words,
/// because it is read by a person in two places.
pub const TRAY_REQUIRED: &str =
    "Click-through is kept off because no tray icon could be installed: the menu is the way back.";

/// A drag that moves the hang point instead of the card.
///
/// `anchor0` is where the hang point *was on screen* when the button went down, in device px. The
/// gesture is measured from that rather than accumulated onto the stored ratio, because the stored
/// pair and the visible position can differ by the inset `place` keeps from the top of the display —
/// and a clock that starts moving only after the cursor has travelled that far feels stuck.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Reposition {
    start: Vec2,
    anchor0: Vec2,
}

/// Cheap instrumentation, printed by `--diag`. Every budget in `docs/architecture.md` is a claim
/// about one of these, which is the difference between a number and a wish.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    pub ticks: u64,
    pub physics_steps: u64,
    pub presents: u64,
    pub present_pixels: u64,
    pub repaints: u64,
}

impl Model {
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let theme = Theme::default();
        let card = settings.card();
        let rope = Rope::new(
            hanglock_core::rope::config::RopeConfig::default(),
            card,
            posture_of(&settings),
            Vec2::new(0.0, 0.0),
            1.0,
        );
        let text = face_of(&settings, (2026, 1, 1, 10, 42, 7));
        Self {
            settings,
            rope,
            canvas: Canvas::new(1, 1),
            theme,
            state: State::Settled,
            text,
            layout_frame: Rect::new(0.0, 0.0, 1.0, 1.0),
            layout_scale: 1.0,
            monitor: None,
            layout_anchor: Vec2::new(0.0, 0.0),
            tray_ok: false,
            last_text_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            reposition: None,
            hover_ring: false,
            saved: false,
            notice: None,
            counters: Counters::default(),
            seconds_since_last_text: 0.0,
        }
    }

    /// True while a ring drag is moving the whole clock. Test-facing: the release path decides
    /// from `reposition` directly, and the tests use this to watch that state machine.
    #[cfg(test)]
    #[must_use]
    pub fn is_repositioning(&self) -> bool {
        self.reposition.is_some()
    }

    /// Recompute where the overlay goes from the current monitor list. Returns the frame the
    /// adapter must apply, in device px.
    pub fn relayout(&mut self, monitors: &[Monitor], primary: u32) -> Rect {
        let chosen = pick(monitors, self.settings.overlay.monitor_index, primary);
        self.apply_monitor(chosen);
        self.layout_frame
    }

    /// Place the overlay on one display and make the rope agree with it. The single point where a
    /// settings change becomes geometry, which is why both a monitor switch and a nudge of the hang
    /// point go through it: two places that each decide where the anchor is is how the cord and the
    /// window stop lining up.
    fn apply_monitor(&mut self, m: Monitor) {
        let p = self.placement(&m);
        let scale = m.scale;
        self.layout_scale = scale;
        self.monitor = Some(m);
        self.layout_frame = p.frame;
        self.layout_anchor = p.anchor;
        let (w, h) = (p.frame.w().max(1.0), p.frame.h().max(1.0));
        // The rope lives in device px, and its host rect is the frame: the reach clamp plus this
        // rect are what keep the plate on the display it belongs to.
        self.rope.scale = scale;
        self.rope.host = Some(Rect::new(0.0, 0.0, w, h));
        self.rope.card = self.settings.card();
        self.rope.anchor = p.anchor;
        self.rope.refit(scale, self.settings.overlay.hang);
        self.rope.set_anchor(p.anchor);
        self.canvas.resize(w as u32, h as u32);
    }

    /// Where the overlay and its hang point go on `m`, given the current document.
    #[must_use]
    pub fn placement(&self, m: &Monitor) -> hanglock_core::placement::Placement {
        let o = &self.settings.overlay;
        place(
            m,
            &self.rope.cfg,
            &self.settings.card(),
            o.hang,
            o.scale,
            o.anchor_ratio,
            o.anchor_drop,
            o.margin,
            o.respect_taskbar,
        )
    }

    /// The hang point in display-global device px: what a drag measures from, and what `--diag`
    /// prints, because "where is my clock" is answered by a screen coordinate and not by a ratio.
    #[must_use]
    pub fn anchor_device(&self) -> Vec2 {
        Vec2::new(
            self.layout_frame.x0 + self.layout_anchor.x,
            self.layout_frame.y0 + self.layout_anchor.y,
        )
    }

    /// The settings pair that describes the current hang point.
    #[must_use]
    pub fn anchor(&self) -> Anchor {
        Anchor::from_overlay(&self.settings.overlay)
    }

    // ---------------------------------------------------------------------------------------------
    // per-event handling
    // ---------------------------------------------------------------------------------------------

    /// A tick from the frame timer. Steps the physics and decides what, if anything, to present.
    pub fn on_frame(&mut self, dt: f64) -> Vec<Action> {
        if self.state == State::Hidden {
            return Vec::new();
        }
        self.counters.ticks += 1;
        let moved = self.rope.step(dt);
        if moved {
            self.counters.physics_steps += 4; // 240 Hz at 60 Hz: four fixed steps per frame
        }
        if !moved {
            // Nothing moved and the picture is unchanged: drop out of Swinging and stop asking for
            // frames. This is the transition the whole design exists to make cheap.
            if self.state == State::Swinging {
                self.state = State::Settled;
            }
            return vec![Action::None];
        }
        self.state = State::Swinging;
        self.paint();
        vec![Action::PresentFull]
    }

    /// The 1 Hz timer. The only wake the steady state of a clock ever needs.
    pub fn on_second(&mut self, fields: (u32, u32, u32, u32, u32, u32)) -> Vec<Action> {
        if self.state == State::Hidden {
            return Vec::new();
        }
        let next = face_of(&self.settings, fields);
        if next == self.text {
            // A minute boundary that produced no visible change (no seconds shown): no repaint, no
            // present, no work. The timer fires; nothing else has to happen.
            return vec![Action::None];
        }
        self.text = next;
        if self.state == State::Swinging {
            // A frame is already coming; fold the digit change into it rather than presenting twice.
            return vec![Action::None];
        }
        self.paint();
        match self.text_present_rect() {
            Some(r) => vec![Action::PresentRect(r)],
            None => vec![Action::PresentFull],
        }
    }

    /// The rect that a digit-only update must cover: the union of where they were and where they are
    /// now, padded by one pixel so anti-aliased edges cannot leave a seam.
    fn text_present_rect(&mut self) -> Option<Rect> {
        let scene = self.scene();
        let now = text_bounds(&scene, &self.theme);
        let prev = std::mem::replace(&mut self.last_text_bounds, now);
        if now.x1 <= now.x0 {
            return None;
        }
        let u = Rect::new(
            now.x0.min(prev.x0) - 1.0,
            now.y0.min(prev.y0) - 1.0,
            now.x1.max(prev.x1) + 1.0,
            now.y1.max(prev.y1) + 1.0,
        );
        // The rect lives in canvas px — `Surface::present` indexes the buffer with it — so the
        // clamp is the canvas box, never the display-global layout frame. Clamping to the frame
        // inverted every rect whose monitor origin pushed x0 past the digits' box, and the
        // presenter silently copied nothing: stale digits until the next full present.
        let (cw, ch) = self.canvas.size();
        let r = Rect::new(
            u.x0.max(0.0),
            u.y0.max(0.0),
            u.x1.min(cw as f64),
            u.y1.min(ch as f64),
        );
        if r.x1 > r.x0 && r.y1 > r.y0 {
            Some(r)
        } else {
            None
        }
    }

    pub fn on_input(&mut self, input: Input) -> Vec<Action> {
        if self.state == State::Hidden {
            return Vec::new();
        }
        let mut out = Vec::new();
        match input {
            Input::Press { at, alt } => {
                // Alt *at the press* means "move the clock", wherever on the object the button landed.
                // The hang point is something a user should be able to grab without hunting for a 20 px
                // ring first; without Alt, the plate is grabbed and swings, and only a press on the
                // ring re-anchors, which is the gesture the pointer was already aimed at.
                let reaim = alt && self.settings.overlay.click_through.interactive();
                if reaim {
                    self.reposition = Some(Reposition {
                        start: at,
                        anchor0: self.anchor_device(),
                    });
                    self.state = State::Swinging;
                    out.push(Action::PresentFull);
                } else if self.rope.begin_drag(at, 10.0) {
                    self.state = State::Swinging;
                    out.push(Action::PresentFull);
                } else if self.ring_contains(at) {
                    self.reposition = Some(Reposition {
                        start: at,
                        anchor0: self.anchor_device(),
                    });
                }
            }
            Input::Move { at, vel, dt } => {
                let _ = dt;
                if let (Some(rep), Some(m)) = (self.reposition, self.monitor) {
                    let want = anchor::dragged(
                        &m,
                        &self.settings,
                        &self.rope.cfg,
                        rep.anchor0,
                        at.sub(rep.start),
                    );
                    self.set_anchor(want, false);
                    out.push(Action::Move(self.layout_frame));
                    out.push(Action::PresentFull);
                } else if self.rope.is_dragging() {
                    self.rope.move_drag(at, vel);
                    self.state = State::Swinging;
                    out.push(Action::PresentFull);
                } else {
                    let ring = self.ring_contains(at);
                    if ring != self.hover_ring {
                        self.hover_ring = ring;
                        // A cursor change is a repaint-free state change: present nothing, but the
                        // adapter will re-ask for the cursor on the next WM_SETCURSOR.
                    }
                }
            }
            Input::Release { .. } => {
                // The gesture ends with the document already holding where the clock is: `Relayout`
                // re-places the frame and re-hangs the rope beneath the new anchor, which is what lets
                // the card follow the window on release instead of during it.
                if self.reposition.take().is_some() {
                    out.push(Action::Save);
                    out.push(Action::Relayout);
                } else if self.rope.is_dragging() {
                    // The adapter's `vel` is only valid for moves; on release the last move's
                    // velocity is the truth, so the drag keeps whatever it was tracking —
                    // nothing to assign, nothing to override.
                    self.rope.end_drag();
                    self.state = State::Swinging;
                    out.push(Action::PresentFull);
                }
            }
            Input::Wheel { delta } => {
                // A wheel notch changes the swept box, so the window must be re-placed before the
                // next present: a longer rope needs a taller window or the swing gets clipped.
                self.step_hang(delta);
                out.push(Action::Relayout);
                out.push(Action::Save);
            }
            Input::Context { at: _ } | Input::Hover(_) => {}
        }
        out
    }

    #[must_use]
    pub fn ring_contains(&self, at: Vec2) -> bool {
        let r = (10.0 * self.layout_scale).max(8.0);
        let a = self.rope.anchor;
        (at.x - a.x).abs() <= r && (at.y - a.y).abs() <= r
    }

    /// Write a hang point into the document and move the overlay to it.
    ///
    /// `rehang` decides what happens to the card: a deliberate change (a menu item, a nudge in the
    /// settings window) re-hangs it under the new anchor, while a drag leaves it where it is to be
    /// pulled after the anchor as the gesture goes on. The second half is why Alt-drag feels like
    /// moving a hanging object rather than a rectangle: the rope stretches and slackens under the
    /// hand, and the solver is the thing that catches up.
    fn set_anchor(&mut self, want: Anchor, rehang: bool) {
        let o = &mut self.settings.overlay;
        o.anchor_ratio = want.ratio;
        o.anchor_drop = want.drop;
        let Some(m) = self.monitor else {
            return;
        };
        let p = self.placement(&m);
        let scale = m.scale;
        self.layout_frame = p.frame;
        self.layout_anchor = p.anchor;
        let (w, h) = (p.frame.w().max(1.0), p.frame.h().max(1.0));
        self.rope.host = Some(Rect::new(0.0, 0.0, w, h));
        self.rope.anchor = p.anchor;
        if rehang {
            self.rope.refit(scale, self.settings.overlay.hang);
        }
        self.rope.wake();
        self.canvas.resize(w as u32, h as u32);
    }

    pub fn on_command(&mut self, cmd: Command) -> Vec<Action> {
        use hanglock_core::settings::limits;
        let mut out = Vec::new();
        let o = &mut self.settings.overlay;
        let f = &mut self.settings.face;
        match cmd {
            Command::ToggleVisible => {
                o.enabled = !o.enabled;
                self.state = if o.enabled {
                    State::Settled
                } else {
                    State::Hidden
                };
                out.push(Action::Save);
            }
            Command::ToggleTopmost => {
                o.topmost = !o.topmost;
                out.push(Action::Save);
            }
            Command::ToggleSeconds => {
                f.seconds = !f.seconds;
                out.push(Action::Save);
            }
            Command::Toggle12Hour => {
                f.hour12 = !f.hour12;
                out.push(Action::Save);
            }
            Command::SetPosture(p) => {
                self.notice = None;
                f.posture = p;
                self.rope.att.posture = posture_of(&self.settings);
                self.rope.wake();
                out.push(Action::Save);
                out.push(Action::PresentFull);
            }
            Command::HangUp | Command::HangDown => {
                let dir = if matches!(cmd, Command::HangUp) {
                    1
                } else {
                    -1
                };
                let cur = o.hang;
                let next = HANG_STEPS
                    .iter()
                    .filter(|s| (dir > 0 && **s > cur + 0.01) || (dir < 0 && **s < cur - 0.01))
                    .copied()
                    .reduce(|a, b| if dir > 0 { a.min(b) } else { a.max(b) })
                    .unwrap_or(cur);
                o.hang = next.clamp(limits::HANG.0, limits::HANG.1);
                out.push(Action::Relayout);
                out.push(Action::Save);
            }
            Command::Bigger | Command::Smaller => {
                let dir = if matches!(cmd, Command::Bigger) {
                    1.0
                } else {
                    -1.0
                };
                o.scale = (o.scale + 0.15 * dir).clamp(limits::SCALE.0, limits::SCALE.1);
                self.rope.card = self.settings.card();
                out.push(Action::Relayout);
                out.push(Action::Save);
            }
            Command::ResetPosition => {
                // The recovery path, so it is written to put the clock back where a first run would
                // have put it — and to *show* it, because "I hid it, and now I cannot find it" is the
                // other way this app can strand a user. It does not change which display it is on: a
                // clock moved to the monitor on the left is still on that monitor, and the user meant
                // that.
                o.anchor_ratio = Anchor::DEFAULT.ratio;
                o.anchor_drop = Anchor::DEFAULT.drop;
                o.hang = 150.0;
                o.scale = 1.0;
                if !o.enabled {
                    o.enabled = true;
                    self.state = State::Settled;
                }
                self.notice = None;
                out.push(Action::Relayout);
                out.push(Action::Save);
            }
            Command::SetClickThrough(mode) => {
                if mode == ClickThrough::Always && !self.tray_ok {
                    self.notice = Some(TRAY_REQUIRED);
                } else {
                    self.notice = None;
                    if o.click_through != mode {
                        o.click_through = mode;
                        out.push(Action::Save);
                    }
                }
            }
            Command::SetMonitor(index) => {
                if o.monitor_index != index {
                    o.monitor_index = index;
                    // Moving it to a display the user can see is the point of the row, so a clock that
                    // was hidden comes back: a switch that appeared to do nothing reads as a failure.
                    if !o.enabled {
                        o.enabled = true;
                        self.state = State::Settled;
                    }
                    out.push(Action::Relayout);
                    out.push(Action::Save);
                }
            }
            Command::SetAnchor(want) => {
                let cur = Anchor::from_overlay(o);
                if cur != want {
                    self.set_anchor(want, true);
                    out.push(Action::Save);
                }
            }
            Command::ToggleMeridiem => {
                f.meridiem = !f.meridiem;
                out.push(Action::Save);
            }
            Command::ToggleLaunchAtLogin => {
                let on = !self.settings.general.launch_at_login;
                self.settings.general.launch_at_login = on;
                // The registry write is the platform's, and the answer is what the document keeps: a
                // checkbox that ticks itself without the OS agreeing tells the user something false.
                out.push(Action::ApplyAutostart(on));
                out.push(Action::Save);
            }
            Command::OpenSettings => {
                if self.state == State::Hidden {
                    // Opening settings is a request to be seen; a hidden clock that answers with an
                    // invisible overlay is a wasted click.
                    o.enabled = true;
                    self.state = State::Settled;
                    out.push(Action::Save);
                }
                out.push(Action::OpenSettings);
            }
            Command::About => {
                out.push(Action::About);
            }
            Command::Diagnostics => {}
            Command::Quit => {
                self.paint();
                out.push(Action::Quit(0));
            }
        }
        // Re-derive the text now, so a toggle of seconds shows on the very next present instead of
        // up to a second later: the user's action is the interesting frame, not the minute boundary.
        self.text = face_of(&self.settings, current_fields_from(&self.text));
        out
    }

    /// The host is up: the tray has been tried, the displays can be enumerated, and the registry knows
    /// what it knows. Two of the document's claims are not the document's to keep, and this is where
    /// they are made true — before the first frame, so a user never sees a state the app is about to
    /// contradict.
    pub fn on_ready(&mut self, tray_present: bool, launch_at_login: bool) -> Vec<Action> {
        self.tray_ok = tray_present;
        let mut out = Vec::new();
        if self.settings.general.launch_at_login != launch_at_login {
            // The registry wins. It is the record of what Windows will actually do at the next logon,
            // and a taskbar toggle or a Task Manager "Disable" is a decision taken *after* the file was
            // written; re-adding the key at startup would be the app overruling a person.
            self.settings.general.launch_at_login = launch_at_login;
            out.push(Action::Save);
        }
        if self.settings.overlay.click_through == ClickThrough::Always && !tray_present {
            // A file copied from another machine, or one written before the icon failed to install,
            // must not be able to strand a clock the user cannot click.
            self.settings.overlay.click_through = ClickThrough::default();
            self.notice = Some(TRAY_REQUIRED);
            out.push(Action::Save);
        }
        out.push(Action::Relayout);
        out
    }

    pub fn on_system(&mut self, event: hanglock_platform::SystemEvent) -> Vec<Action> {
        match event {
            hanglock_platform::SystemEvent::DisplaysChanged
            | hanglock_platform::SystemEvent::Relayout
            | hanglock_platform::SystemEvent::DpiChanged => vec![Action::Relayout],
            hanglock_platform::SystemEvent::Resumed => {
                // Drop the accumulated deficit rather than paying it: the rope has not been moving
                // while the machine slept, and "catching up" would look like a teleport. Re-read
                // the time, stay settled, and let the next real tick be an ordinary one.
                self.rope.acc_reset();
                // Nothing else to recompute by hand: the relayout is the same path a monitor change
                // takes, and a machine that has been asleep may well have woken on a different dock.
                vec![Action::Relayout]
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // painting and presentation
    // ---------------------------------------------------------------------------------------------

    #[must_use]
    pub fn scene(&self) -> Scene {
        Scene::new(
            &self.rope,
            &self.rope.cfg,
            self.text,
            self.settings.overlay.opacity,
        )
    }

    pub fn paint(&mut self) {
        let scene = self.scene();
        let theme = self.theme;
        hanglock_render::paint(&scene, &mut self.canvas, &theme);
        self.counters.repaints += 1;
        if let Some(r) = self.canvas.present_rect() {
            self.counters.present_pixels += ((r.x1 - r.x0) * (r.y1 - r.y0)) as u64;
        }
        self.counters.presents += 1;
    }

    #[must_use]
    pub fn hit_regions(&self) -> HitRegions {
        let c = self.rope.card_centre();
        let scale = self.layout_scale;
        HitRegions {
            plate_centre: c,
            plate_hw: self.rope.card.width * 0.5 * scale,
            plate_hh: self.rope.card.height * 0.5 * scale,
            theta: self.rope.att.theta,
            anchor: self.rope.anchor,
            anchor_radius: (10.0 * scale).max(8.0),
            interactive: self.settings.overlay.click_through.interactive(),
            whole_window: self.settings.overlay.click_through.takes_window(),
        }
    }

    #[must_use]
    pub fn cursor(&self) -> CursorKind {
        let rep = self.reposition.is_some();
        if rep || self.hover_ring {
            CursorKind::Move
        } else if self.rope.is_dragging() {
            CursorKind::Grabbing
        } else {
            // Any pixel that reaches us is on the plate or the ring, because everything else answers
            // HTTRANSPARENT, so there is no "over the background" cursor to decide about.
            CursorKind::Grab
        }
    }

    #[must_use]
    pub fn wants_ticks(&self) -> bool {
        self.state == State::Swinging
    }

    #[must_use]
    pub fn preferred_rate(&self) -> u32 {
        self.settings.general.fps_cap
    }

}

impl Model {
    /// `hang` moves in discrete steps so the wheel, the menu and the settings file all agree, and so
    /// what a user wrote by hand stays a value the app will produce itself.
    fn step_hang(&mut self, delta: i32) {
        let dir = if delta > 0 { 1 } else { -1 };
        let cur = self.settings.overlay.hang;
        let next = HANG_STEPS
            .iter()
            .filter(|s| (dir > 0 && **s > cur + 0.01) || (dir < 0 && **s < cur - 0.01))
            .copied()
            .reduce(|a, b| if dir > 0 { a.min(b) } else { a.max(b) })
            .unwrap_or(cur);
        self.settings.overlay.hang = next;
    }
}

fn pick(monitors: &[Monitor], wanted: u32, primary: u32) -> Monitor {
    monitors
        .iter()
        .find(|m| m.index == wanted)
        .or_else(|| monitors.iter().find(|m| m.index == primary))
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or_else(|| monitors.first())
        .copied()
        .unwrap_or(hanglock_core::placement::DEFAULT_MONITOR)
}

fn posture_of(s: &Settings) -> Posture {
    match s.face.posture {
        PostureKind::Natural => Posture::NATURAL,
        PostureKind::Plate => Posture::PLATE,
        PostureKind::Mounted => Posture::MOUNTED,
        PostureKind::Locked => Posture::LOCKED,
    }
}

fn face_of(s: &Settings, fields: (u32, u32, u32, u32, u32, u32)) -> FaceText {
    let (year, month, day, hour, minute, second) = fields;
    let c = Civil {
        year: i64::from(year),
        month,
        day,
        hour,
        minute,
        second,
    };
    hanglock_core::clock::format::format(
        &c,
        &FaceOptions {
            hour12: s.face.hour12,
            seconds: s.face.seconds,
            meridiem: s.face.meridiem,
        },
    )
}

/// `on_command` needs to reformat with the same wall time it last displayed; the fields are not
/// worth threading through every command, and a command landing between two seconds displaying the
/// previous second for one frame is invisible and self-correcting.
fn current_fields_from(t: &FaceText) -> (u32, u32, u32, u32, u32, u32) {
    let s = t.as_str();
    let mut chars = s.chars();
    let dig = |c: &mut std::str::Chars<'_>| -> u32 {
        let a = c.next().and_then(|x| x.to_digit(10)).unwrap_or(0);
        let b = c.next().and_then(|x| x.to_digit(10)).unwrap_or(0);
        a * 10 + b
    };
    let h = dig(&mut chars);
    if chars.next() != Some(':') {
        return (2026, 1, 1, h, 0, 0);
    }
    let mi = dig(&mut chars);
    (2026, 1, 1, h, mi, 0)
}

// ---------------------------------------------------------------------------------------------
// headless modes
// ---------------------------------------------------------------------------------------------

/// Render one settled frame. This is how the visual design was reviewed on a machine with no
/// desktop: the painter and the model run unchanged, and the output is the real thing.
pub fn dump_scene(settings: &Settings, path: &str) -> Result<usize, String> {
    let mut m = Model::new(*settings);
    let m0 = Monitor {
        index: 0,
        bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        work: Rect::new(0.0, 0.0, 1920.0, 1040.0),
        scale: 1.0,
        taskbar_top: false,
        taskbar_auto_hidden: false,
        primary: true,
    };
    let frame = m.relayout(&[m0], 0);
    let _ = frame;
    // Swing it a little so the picture under review shows a hanging object, not a rectangle.
    for _ in 0..40 {
        m.rope.step(1.0 / 60.0);
    }
    m.paint();
    let png = hanglock_render::png::encode(&m.canvas);
    std::fs::write(path, &png).map_err(|e| format!("{path}: {e}"))?;
    Ok(png.len())
}

pub fn bench(settings: &Settings, n: usize) {
    let mut m = Model::new(*settings);
    let m0 = Monitor {
        index: 0,
        bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        work: Rect::new(0.0, 0.0, 1920.0, 1040.0),
        scale: 1.5,
        taskbar_top: false,
        taskbar_auto_hidden: false,
        primary: true,
    };
    m.relayout(&[m0], 0);
    let t0 = std::time::Instant::now();
    for _ in 0..n {
        m.paint();
    }
    let dt = t0.elapsed();
    let (w, h) = m.canvas.size();
    let per = dt.as_nanos() as f64 / n.max(1) as f64;
    println!("hanglock bench: {n} frames at {w}x{h}");
    println!(
        "  paint            {:>8.1} us/frame   (budget 400 us)",
        per / 1000.0
    );
    println!(
        "  buffer           {:>8.1} KiB",
        (w * h * 4) as f64 / 1024.0
    );
    println!(
        "  present copy     {:>8.1} KiB/frame full-window",
        (w * h * 4) as f64 / 1024.0
    );
    println!(
        "  solve+paint/sec  {:>8.1} ms of a 1000 ms budget at 60 Hz",
        per * 60.0 / 1e6
    );
}

/// What the app decided, and what it cost: the text `--diag` prints and the CI smoke step publishes.
///
/// Every line is a claim a user or a reviewer can check against their own machine, so the rule here
/// is that it says what the *document* holds and what the *placement maths* made of it, and nothing
/// that could only be known from inside a running window. No account names, no device names, no
/// serial numbers: the expanded path of the settings file would carry a username, so it is printed as
/// `%APPDATA%\Hanglock\settings.toml`.
///
/// `monitors` may be empty (a machine with no desktop, or a Linux runner): the synthetic 1080p panel
/// is then used, and the output says so, because a printed number that looks measured and was invented
/// is worse than one that is missing.
#[must_use]
pub fn diag(settings: &Settings, monitors: &[Monitor], found: &dyn std::fmt::Display) -> String {
    let mut m = Model::new(*settings);
    let synthetic = monitors.is_empty();
    let m0 = Monitor {
        index: 0,
        bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        work: Rect::new(0.0, 0.0, 1920.0, 1040.0),
        scale: 1.0,
        taskbar_top: false,
        taskbar_auto_hidden: false,
        primary: true,
    };
    let list = if synthetic { vec![m0] } else { monitors.to_vec() };
    let primary = list
        .iter()
        .find(|x| x.primary)
        .or(list.first())
        .map_or(0, |x| x.index);
    let chosen = pick(&list, settings.overlay.monitor_index, primary);
    let frame = m.relayout(&list, primary);
    let p = m.placement(&chosen);
    let o = &settings.overlay;
    let mut out = String::new();
    out.push_str(&format!("hanglock {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("  settings file   {}\n", crate::store::display_path()));
    out.push_str(&format!("  as read         {found}\n"));
    out.push_str(&format!(
        "  displays        {}{}\n",
        list.len(),
        if synthetic {
            " (none enumerated: the numbers below are a stand-in 1920x1080 panel)"
        } else {
            ""
        }
    ));
    for d in &list {
        let b = d.bounds_logical();
        let w = d.work_logical();
        out.push_str(&format!(
            "    #{:<2} {:.0}x{:.0} at {:.0},{:.0}  work {:.0}x{:.0}  {:.0}% ({:.0} dpi){}\n",
            d.index,
            b.w(),
            b.h(),
            b.x0,
            b.y0,
            w.w(),
            w.h(),
            d.scale * 100.0,
            d.scale * hanglock_core::units::LOGICAL_DPI,
            flags(d)
        ));
    }
    out.push_str(&format!(
        "  on monitor      #{} of {}{}\n",
        chosen.index,
        list.len(),
        if chosen.index == o.monitor_index {
            String::new()
        } else {
            format!(" (saved #{}, not connected: falling back)", o.monitor_index)
        }
    ));
    out.push_str(&format!(
        "  frame (device)  {:.0},{:.0} {:.0}x{:.0}  clipped={}\n",
        frame.x0,
        frame.y0,
        frame.w(),
        frame.h(),
        p.clipped
    ));
    let a = m.anchor_device();
    out.push_str(&format!(
        "  anchor          ratio {:.4} -> x {:.1}; drop {:.1} logical; device {:.1},{:.1} in the frame\n",
        o.anchor_ratio,
        chosen.bounds_logical().x0 + chosen.bounds_logical().w() * o.anchor_ratio,
        o.anchor_drop,
        m.layout_anchor.x,
        m.layout_anchor.y
    ));
    out.push_str(&format!(
        "  anchor (screen) {:.1},{:.1} device px\n",
        a.x, a.y
    ));
    out.push_str(&format!("  clock           {}\n", face_summary(settings, &m)));
    out.push_str(&format!(
        "  mouse           {} ({})\n",
        o.click_through.label(),
        o.click_through.as_str()
    ));
    out.push_str(&format!(
        "  on top          {}{}\n",
        if o.topmost { "on" } else { "off" },
        if o.topmost { "" } else { " (the OS may cover it)" }
    ));
    out.push_str(&format!(
        "  visible         {}\n",
        if o.enabled { "yes" } else { "no - the tray is the way back" }
    ));
    out.push_str(&format!(
        "  autostart       {}\n",
        if settings.general.launch_at_login {
            "wanted on; the running app writes the HKCU Run entry and reconciles it at startup"
        } else {
            "off"
        }
    ));
    out.push_str(&format!(
        "  posture         {} ({})\n",
        settings.face.posture.as_str(),
        settings.face.posture.label()
    ));
    out.push_str(&format!(
        "  hang            {:.0} logical px, {:.1} device px, {} links\n",
        o.hang, m.rope.hang, m.rope.cfg.segments
    ));
    out.push_str(&format!(
        "  size            {}%, margin {:.0}\n",
        (o.scale * 100.0).round() as i64,
        o.margin
    ));
    out.push_str(&format!(
        "  state           {:?}, sleeping={}\n",
        m.state, m.rope.sleeping
    ));
    m.paint();
    out.push_str(&format!(
        "  canvas          {}x{}, present_rect {}\n",
        m.canvas.size().0,
        m.canvas.size().1,
        m.canvas.present_rect().is_some()
    ));
    out.push_str(&format!(
        "  fps cap         {} Hz, {} frames per present at 60 Hz\n",
        settings.general.fps_cap,
        (settings.general.fps_cap).min(60)
    ));
    out.push_str(&format!(
        "  budgets         paint 400 us, window {}x{} device, buffer {:.0} KiB\n",
        m.canvas.size().0,
        m.canvas.size().1,
        f64::from(m.canvas.size().0 * m.canvas.size().1 * 4) / 1024.0
    ));
    out
}

/// The one-line description of the panel per display: what a reader needs to know to answer "which
/// one is the clock on, and is the taskbar in the way".
fn flags(d: &Monitor) -> String {
    let mut s = String::new();
    if d.primary {
        s.push_str(", primary");
    }
    if d.taskbar_top {
        s.push_str(if d.taskbar_auto_hidden {
            ", taskbar at top (auto)"
        } else {
            ", taskbar at top"
        });
    }
    s
}

/// The format, in the words the settings window uses, with an example: "12-hour" is a claim about a
/// picture, and the picture is the string next to it.
fn face_summary(s: &Settings, m: &Model) -> String {
    let f = &s.face;
    format!(
        "{}, {} AM/PM, seconds {}; shows \"{}\"",
        if f.hour12 { "12-hour" } else { "24-hour" },
        if f.meridiem && f.hour12 { "with" } else { "no" },
        if f.seconds { "on" } else { "off" },
        m.text.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanglock_core::settings::Settings;

    /// The display every test here hangs from: a plain 1080p panel at 100 %, no taskbar at the top.
    fn monitor() -> Monitor {
        Monitor {
            index: 0,
            bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            work: Rect::new(0.0, 0.0, 1920.0, 1040.0),
            scale: 1.0,
            taskbar_top: false,
            taskbar_auto_hidden: false,
            primary: true,
        }
    }

    /// The same panel, and a second one to the right of it: the shape of a docked laptop.
    fn two() -> Vec<Monitor> {
        vec![
            monitor(),
            Monitor {
                index: 1,
                bounds: Rect::new(1920.0, 0.0, 3840.0, 1080.0),
                work: Rect::new(1920.0, 0.0, 3840.0, 1040.0),
                ..Monitor::default()
            },
        ]
    }

    fn model() -> Model {
        let mut m = Model::new(Settings::default());
        m.relayout(&[monitor()], 0);
        m
    }

    #[test]
    fn the_frame_fits_the_display_and_the_anchor_is_inside_it() {
        let m = model();
        let f = m.layout_frame;
        assert!(
            f.x0 >= -0.01 && f.x1 <= 1920.0 + 0.01,
            "frame {f:?} escaped the monitor"
        );
        assert!(f.y0 >= -0.01);
        assert!(m.rope.anchor.x > 0.0 && m.rope.anchor.x < f.w());
        assert!(m.rope.anchor.y > 0.0 && m.rope.anchor.y < f.h());
        // The whole hang must fit vertically, which is the point of sizing the window to the sweep.
        assert!(f.h() > m.rope.hang);
    }

    #[test]
    fn a_settled_rope_stops_asking_for_frames() {
        let mut m = model();
        assert!(m.state == State::Swinging || m.state == State::Settled);
        // Swing it hard, then let it settle.
        let c = m.rope.card_centre();
        m.rope.begin_drag(c, 10.0);
        for i in 0..12 {
            m.rope.move_drag(
                Vec2::new(m.rope.anchor.x + 120.0, m.rope.anchor.y + 140.0 + i as f64),
                Vec2::new(600.0, 0.0),
            );
            m.on_frame(1.0 / 60.0);
        }
        m.rope.end_drag();
        let mut frames = 0;
        let mut actions_with_motion = 0;
        while frames < 2000 {
            let a = m.on_frame(1.0 / 60.0);
            if a.iter().any(|x| matches!(x, Action::PresentFull)) {
                actions_with_motion += 1;
            }
            frames += 1;
            if !m.wants_ticks() {
                break;
            }
        }
        assert!(!m.wants_ticks(), "the app never stopped asking for frames");
        assert!(
            actions_with_motion < 400,
            "too many presented frames while settling: {actions_with_motion}"
        );
        assert!(m.state == State::Settled);
    }

    #[test]
    fn a_second_that_changed_nothing_presents_nothing() {
        let mut m = model();
        // No seconds displayed: the minute boundary changes nothing visible.
        assert!(!m.settings.face.seconds);
        let a = m.on_second((2026, 9, 15, 10, 42, 0));
        let b = m.on_second((2026, 9, 15, 10, 42, 59));
        assert!(
            a.iter().all(|x| matches!(x, Action::None)),
            "an unchanged face must not present: {a:?}"
        );
        assert!(
            b.iter().all(|x| matches!(x, Action::None))
                || b.iter().any(|x| matches!(x, Action::PresentRect(_)))
        );
    }

    #[test]
    fn a_second_that_changed_the_digits_presents_only_their_box() {
        let mut m = model();
        m.settings.face.seconds = true;
        let fields = (2026, 9, 15, 22, 42, 7);
        let _ = m.on_second(fields);
        m.text = FaceText::default();
        let a = m.on_second(fields);
        let rect = a.iter().find_map(|x| match x {
            Action::PresentRect(r) => Some(*r),
            _ => None,
        });
        let rect = rect.expect("a changed face must present a rect while settled");
        let f = m.layout_frame;
        let digits_area = (rect.x1 - rect.x0) * (rect.y1 - rect.y0);
        let whole = f.w() * f.h();
        assert!(digits_area > 0.0);
        assert!(
            digits_area < whole * 0.35,
            "presenting {:.0}% of the window for a digit change",
            digits_area / whole * 100.0
        );
    }

    #[test]
    fn the_wheel_steps_the_hang_and_asks_for_a_relayout() {
        let mut m = model();
        let before = m.settings.overlay.hang;
        let a = m.on_input(Input::Wheel { delta: 1 });
        assert!(
            a.contains(&Action::Relayout),
            "a longer hang needs a taller window: {a:?}"
        );
        assert!(m.settings.overlay.hang > before);
        let top = m.settings.overlay.hang;
        m.on_input(Input::Wheel { delta: 1 });
        m.on_input(Input::Wheel { delta: 1 });
        assert!(
            m.settings.overlay.hang > top,
            "stepping must keep climbing until the range ends"
        );
        assert_eq!(
            m.settings.overlay.hang,
            HANG_STEPS[HANG_STEPS.len() - 1],
            "three notches from the default must land on the longest hang"
        );
        m.on_input(Input::Wheel { delta: 1 });
        assert_eq!(
            m.settings.overlay.hang,
            HANG_STEPS[HANG_STEPS.len() - 1],
            "the longest step must stick at the end of the range"
        );
        for _ in 0..10 {
            m.on_input(Input::Wheel { delta: -1 });
        }
        assert!(
            m.settings.overlay.hang < before,
            "the wheel must be able to come back"
        );
    }

    #[test]
    fn clicking_through_always_disables_hit_testing() {
        let mut m = model();
        assert!(m.hit_regions().interactive);
        m.on_command(Command::SetPosture(PostureKind::Locked));
        m.settings.overlay.click_through = ClickThrough::Always;
        assert!(!m.hit_regions().interactive);
        let p = m.rope.card_centre();
        assert!(
            !m.hit_regions().contains(p),
            "with click_through=always nothing may be clickable"
        );
    }

    #[test]
    fn the_ring_and_the_plate_are_both_reachable_and_distinguishable() {
        let m = model();
        let r = m.hit_regions();
        let centre = r.plate_centre;
        assert!(
            r.on_plate(centre),
            "the plate's own centre must be on the plate"
        );
        let a = r.anchor;
        assert!(
            r.on_ring(a),
            "the ring must be grabbable: it is how the clock is moved"
        );
        assert!(!r.on_plate(a), "the ring must not count as the plate");
    }

    #[test]
    fn every_command_that_changes_something_asks_to_save() {
        let mut m = model();
        for cmd in [
            Command::ToggleTopmost,
            Command::ToggleSeconds,
            Command::Toggle12Hour,
            Command::SetPosture(PostureKind::Natural),
            Command::HangUp,
            Command::Bigger,
            Command::ResetPosition,
        ] {
            let a = m.on_command(cmd);
            assert!(
                a.contains(&Action::Save),
                "{cmd:?} changed settings without persisting"
            );
        }
    }

    #[test]
    fn hiding_stops_everything_and_showing_resumes_it() {
        let mut m = model();
        m.on_command(Command::ToggleVisible);
        assert_eq!(m.state, State::Hidden);
        let a = m.on_frame(1.0 / 60.0);
        assert!(a.is_empty(), "a hidden clock must do nothing at all: {a:?}");
        let b = m.on_second((2026, 9, 15, 22, 42, 7));
        assert!(b.is_empty());
        m.on_command(Command::ToggleVisible);
        assert_eq!(m.state, State::Settled);
    }

    #[test]
    fn resume_from_sleep_relays_out_without_catching_up() {
        let mut m = model();
        for _ in 0..30 {
            m.on_frame(1.0 / 60.0);
        }
        let before = m.rope.card_centre();
        let a = m.on_system(hanglock_platform::SystemEvent::Resumed);
        assert!(a.contains(&Action::Relayout));
        let after = m.rope.card_centre();
        assert_eq!(
            before, after,
            "a resume must not move the object; the deficit is dropped"
        );
    }

    #[test]
    fn moving_the_ring_moves_the_clock_and_persists_a_ratio() {
        let mut m = model();
        let a = m.rope.anchor;
        m.on_input(Input::Press {
            at: Vec2::new(a.x, a.y),
            alt: false,
        });
        assert!(
            m.is_repositioning(),
            "pressing the ring must start a reposition"
        );
        let moved = m.on_input(Input::Move {
            at: Vec2::new(a.x + 300.0, a.y),
            vel: Vec2::ZERO,
            dt: 1.0 / 60.0,
        });
        assert!(
            moved.iter().any(|x| matches!(x, Action::Move(_))),
            "the window must follow the ring"
        );
        let ratio = m.settings.overlay.anchor_ratio;
        assert!(
            ratio > 0.5,
            "dragging right must increase the ratio, got {ratio}"
        );
        m.on_input(Input::Release {
            at: Vec2::new(a.x + 300.0, a.y),
            vel: Vec2::ZERO,
        });
        assert!(
            (m.settings.overlay.anchor_ratio - ratio).abs() < 1e-9,
            "the ratio was committed at release time"
        );
    }

    #[test]
    fn alt_at_the_press_reaims_rather_than_grabbing() {
        let mut m = model();
        // The middle of the plate: the spot that *without* Alt is a grab-and-swing.
        let c = m.rope.card_centre();
        m.on_input(Input::Press {
            at: c,
            alt: true,
        });
        assert!(
            m.is_repositioning(),
            "Alt at the press must aim at the hang point wherever the pointer is"
        );
        assert!(
            !m.rope.is_dragging(),
            "and it must not also grab the plate: one gesture, not two"
        );
    }

    #[test]
    fn a_reanchor_moves_the_window_under_the_card_instead_of_carrying_it() {
        let mut m = model();
        let a = m.rope.anchor;
        let before = m.rope.card_centre();
        m.on_input(Input::Press {
            at: a,
            alt: true,
        });
        let acts = m.on_input(Input::Move {
            at: Vec2::new(a.x, a.y + 64.0),
            vel: Vec2::ZERO,
            dt: 1.0 / 60.0,
        });
        assert!(acts.contains(&Action::Move(m.layout_frame)));
        assert!(
            (m.rope.card_centre() - before).len() < 1e-9,
            "the card moved with the window; the rope is what should carry it"
        );
        assert!(
            m.layout_frame.y0 > 0.5,
            "the window did not follow the hang point down: {:?}",
            m.layout_frame
        );
        assert!(!m.rope.sleeping, "a re-anchor must wake the solver");
        let ratio = m.settings.overlay.anchor_ratio;
        assert!(
            (ratio - 0.5).abs() < 1e-9,
            "a vertical drag must not change the horizontal pair: {ratio}"
        );
        m.on_input(Input::Release {
            at: Vec2::new(a.x, a.y + 64.0),
            vel: Vec2::ZERO,
        });
        let drop = m.settings.overlay.anchor_drop;
        assert!(
            (drop - 78.0).abs() < 1e-6,
            "the drop should read 64 + the inset it replaced, got {drop}"
        );
    }

    /// What a drag leaves behind has to be what the file holds: a pair, not a device coordinate. This
    /// is the round trip a restart performs, in one test.
    #[test]
    fn a_dragged_position_survives_the_settings_round_trip() {
        let mut m = model();
        let a = m.rope.anchor;
        m.on_input(Input::Press {
            at: a,
            alt: true,
        });
        m.on_input(Input::Move {
            at: Vec2::new(a.x - 400.0, a.y + 90.0),
            vel: Vec2::ZERO,
            dt: 1.0 / 60.0,
        });
        m.on_input(Input::Release {
            at: Vec2::new(a.x - 400.0, a.y + 90.0),
            vel: Vec2::ZERO,
        });
        let saved = m.settings.to_toml();
        let (back, warnings) = Settings::from_toml(&saved);
        assert!(warnings.is_empty(), "round trip complained: {warnings:?}");
        assert_eq!(back, m.settings, "the document did not survive its own writer");
        let mut m2 = Model::new(back);
        let mon = m.monitor.expect("the first model had a monitor");
        let frame = m2.relayout(&[mon], 0);
        assert_eq!(
            frame, m.layout_frame,
            "a restart put the clock somewhere the drag did not"
        );
        assert_eq!(m2.anchor(), m.anchor());
    }

    #[test]
    fn reset_position_clears_the_drop_and_brings_a_hidden_clock_back() {
        let mut m = model();
        let a = m.rope.anchor;
        m.on_input(Input::Press {
            at: a,
            alt: true,
        });
        m.on_input(Input::Move {
            at: Vec2::new(a.x + 20.0, a.y + 260.0),
            vel: Vec2::ZERO,
            dt: 1.0 / 60.0,
        });
        m.on_input(Input::Release {
            at: Vec2::new(a.x + 20.0, a.y + 260.0),
            vel: Vec2::ZERO,
        });
        m.on_command(Command::ToggleVisible);
        assert_eq!(m.state, State::Hidden);
        assert!(m.settings.overlay.anchor_drop > 100.0, "setup went wrong");
        let acts = m.on_command(Command::ResetPosition);
        assert_eq!(m.settings.overlay.anchor_drop, 0.0);
        assert_eq!(m.settings.overlay.anchor_ratio, 0.5);
        assert_eq!(m.settings.overlay.hang, 150.0);
        assert_eq!(m.state, State::Settled, "and it is showing again");
        assert!(acts.contains(&Action::Relayout));
        assert!(acts.contains(&Action::Save));
    }

    #[test]
    fn the_three_click_through_modes_answer_the_mouse_differently() {
        let mut m = model();
        m.tray_ok = true;
        let p = Vec2::new(4.0, 4.0); // inside the window, outside the plate and the ring
        let regions = |m: &Model| m.hit_regions();
        // The default: the object answers, the box around it does not.
        assert_eq!(m.settings.overlay.click_through, ClickThrough::Hover);
        assert!(!regions(&m).whole_window);
        assert!(!regions(&m).contains(p), "empty pixels must stay the desktop's");
        assert!(regions(&m).on_ring(m.rope.anchor));
        // Solid: the whole rectangle is a window again.
        m.on_command(Command::SetClickThrough(ClickThrough::Solid));
        assert!(regions(&m).whole_window);
        assert!(regions(&m).interactive);
        assert!(regions(&m).contains(p), "a normal window takes its own corners");
        // Always: nothing does, and the tray is the only way out.
        m.on_command(Command::SetClickThrough(ClickThrough::Always));
        assert!(!regions(&m).interactive);
        assert!(!regions(&m).whole_window);
        assert!(
            !regions(&m).contains(m.rope.card_centre()),
            "fully click-through must not answer even on the plate"
        );
        assert_eq!(m.settings.overlay.click_through, ClickThrough::Always);
        // And back, because the menu has to be able to undo it.
        m.on_command(Command::SetClickThrough(ClickThrough::Hover));
        assert!(regions(&m).on_plate(m.rope.card_centre()));
    }

    #[test]
    fn fully_click_through_needs_a_tray_to_come_back_through() {
        let mut m = model();
        m.tray_ok = false;
        let acts = m.on_command(Command::SetClickThrough(ClickThrough::Always));
        assert_eq!(
            m.settings.overlay.click_through,
            ClickThrough::Hover,
            "refuse the mode that can strand a clock when the tray is not there"
        );
        assert!(acts.is_empty(), "and say nothing to the window about it");
        assert_eq!(
            m.notice,
            Some(TRAY_REQUIRED),
            "but leave a reason the tooltip and --diag can show"
        );
        m.tray_ok = true;
        m.on_command(Command::SetClickThrough(ClickThrough::Always));
        assert_eq!(m.settings.overlay.click_through, ClickThrough::Always);
        assert_eq!(m.notice, None, "the refusal is not a scar");
    }

    #[test]
    fn a_monitor_switch_keeps_the_position_and_moves_the_window() {
        let mut m = model();
        let mons = two();
        m.on_command(Command::SetMonitor(1));
        assert_eq!(m.settings.overlay.monitor_index, 1);
        // The command answers with `Relayout`; the adapter is what re-asks for the list, so the test
        // does the same rather than assuming the model already knew about display #1.
        m.relayout(&two(), 0);
        assert_eq!(m.monitor.expect("placed").index, 1);
        assert!(
            m.layout_frame.x0 >= 1920.0 - 0.01,
            "the clock is not on the second display: {:?}",
            m.layout_frame
        );
        assert_eq!(m.settings.overlay.anchor_ratio, 0.5);
    }

    #[test]
    fn a_vanished_monitor_falls_back_without_touching_the_saved_pair() {
        let mut m = model();
        m.settings.overlay.monitor_index = 7;
        m.settings.overlay.anchor_ratio = 0.75;
        m.settings.overlay.anchor_drop = 96.0;
        let mons = two();
        m.relayout(&mons, 0);
        assert_eq!(
            m.monitor.expect("placed").index,
            0,
            "the primary is the fallback, not an empty screen"
        );
        assert_eq!(m.settings.overlay.anchor_ratio, 0.75);
        assert_eq!(m.settings.overlay.anchor_drop, 96.0);
        let b = mons[0].bounds;
        assert!(m.layout_frame.x0 >= b.x0 - 0.01 && m.layout_frame.x1 <= b.x1 + 0.01);
        let a = m.anchor_device();
        assert!(a.y > 0.0 && a.y < b.y1, "the hang point left the screen");
    }

    #[test]
    fn a_dpi_change_rescales_the_same_logical_position() {
        let mut m = model();
        let half = |m: &Model| {
            (
                m.layout_frame.w(),
                m.layout_frame.h(),
                m.layout_anchor.x,
                m.layout_anchor.y,
            )
        };
        let (w1, h1, ax1, ay1) = half(&m);
        let mut hdpi = monitor();
        hdpi.scale = 2.0;
        m.relayout(&[hdpi], 0);
        let (w2, h2, ax2, ay2) = half(&m);
        assert!(
            (w2 - w1 * 2.0).abs() < 1.0 && (h2 - h1 * 2.0).abs() < 1.0,
            "the window did not double with the scale: {w1}x{h1} -> {w2}x{h2}"
        );
        assert!(
            (ax2 - ax1 * 2.0).abs() < 1.0 && (ay2 - ay1 * 2.0).abs() < 1.0,
            "the anchor did not: {ax1},{ay1} -> {ax2},{ay2}"
        );
        assert_eq!(
            m.settings.overlay.anchor_ratio, 0.5,
            "and the document was not rewritten to device px"
        );
    }

    #[test]
    fn start_with_windows_is_applied_and_reported() {
        let mut m = model();
        let a = m.on_command(Command::ToggleLaunchAtLogin);
        assert!(m.settings.general.launch_at_login);
        assert!(
            a.contains(&Action::ApplyAutostart(true)),
            "the wish has to reach the registry: {a:?}"
        );
        assert!(a.contains(&Action::Save));
        let b = m.on_command(Command::ToggleLaunchAtLogin);
        assert!(!m.settings.general.launch_at_login);
        assert!(b.contains(&Action::ApplyAutostart(false)));
    }

    /// What the startup reconciliation is for: the registry, not the file, is the record of what
    /// Windows will actually do next logon, so when the two disagree the file is corrected.
    #[test]
    fn startup_adopts_a_login_entry_the_file_does_not_know_about() {
        let mut m = model();
        let acts = m.on_ready(true, true);
        assert!(m.settings.general.launch_at_login);
        assert!(
            acts.contains(&Action::Save),
            "and the corrected document has to be written back: {acts:?}"
        );
        assert!(m.tray_ok);
        // Agreeing costs nothing: no write, no save.
        let again = m.on_ready(true, true);
        assert_eq!(again, vec![Action::Relayout], "a quiet startup is a quiet startup");
    }

    #[test]
    fn opening_settings_shows_the_clock_it_describes() {
        let mut m = model();
        m.on_command(Command::ToggleVisible);
        let acts = m.on_command(Command::OpenSettings);
        assert!(m.settings.overlay.enabled);
        assert!(acts.contains(&Action::OpenSettings), "{acts:?}");
        let b = m.on_command(Command::About);
        assert!(b.contains(&Action::About));
    }

    #[test]
    fn a_format_command_repaints_now_not_on_the_next_minute() {
        let mut m = model();
        m.on_second((2026, 9, 15, 22, 42, 7));
        let before = m.text.as_str().to_string();
        m.on_command(Command::ToggleSeconds);
        let with = m.text.as_str().to_string();
        assert_ne!(before, with, "seconds on has to show at once: {with}");
        assert_eq!(with.matches(':').count(), 2, "{with}");
        m.on_command(Command::Toggle12Hour);
        let h24 = m.text.as_str().to_string();
        assert!(h24.starts_with("22:"), "{h24} is not 24-hour");
        assert!(m.text.suffix_str().is_empty(), "24-hour has no AM/PM");
        m.on_command(Command::Toggle12Hour);
        m.on_command(Command::ToggleMeridiem);
        assert_eq!(
            m.text.suffix_str(),
            "",
            "AM/PM off in a 12-hour clock is the whole point of the row"
        );
        m.on_command(Command::ToggleMeridiem);
        assert_eq!(m.text.suffix_str(), "PM");
    }

    #[test]
    fn the_diagnostics_print_the_saved_position_and_no_more() {
        let mut m = model();
        m.settings.overlay.anchor_drop = 96.0;
        m.settings.general.launch_at_login = true;
        let text = diag(&m.settings, &[], "read as written");
        for want in [
            "anchor",
            "drop 96.0",
            "on monitor      #0",
            "displays        1",
            "12-hour",
            "Transparent areas click through",
            "on top          on",
            "autostart",
            "HKCU",
        ] {
            assert!(text.contains(want), "missing {want:?} in:\n{text}");
        }
        assert!(
            text.contains("stand-in"),
            "the synthetic display has to be labelled as one"
        );
        // A bug report is written down and pasted around, so nothing in this text may name a person.
        for forbidden in ["C:\\Users", "/home/", "/root", "@"] {
            assert!(
                !text.to_lowercase().contains(&forbidden.to_lowercase()),
                "{forbidden:?} leaked into --diag"
            );
        }
        assert!(text.contains("%APPDATA%\\Hanglock\\settings.toml"));
    }

    #[test]
    fn bench_and_dump_produce_a_picture() {
        // The headless paths are how the design was reviewed with no desktop available; a panic here
        // means `--dump-scene` is broken, which costs a review cycle.
        let s = Settings::default();
        bench(&s, 2);
        let out = std::env::temp_dir().join("hanglock-test-scene.png");
        let n = dump_scene(&s, out.to_str().unwrap()).expect("dump_scene failed");
        let bytes = std::fs::read(&out).unwrap();
        assert!(n > 1000, "PNG too small: {n}");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
        let _ = std::fs::remove_file(&out);
    }
}

#[cfg(test)]
mod geometry {
    use super::*;
    use hanglock_core::rope::config::CardSpec;

    /// The cost model in `docs/gate-a.md` in one place: how big the window is, and how many pixels a
    /// present moves, at the scales Windows users actually run. These are exact counts, not guesses —
    /// they are what the presenter's memcpy does per frame.
    fn sizes(scale: f64) -> (f64, f64, f64, f64) {
        let cfg = hanglock_core::rope::config::RopeConfig::default();
        let card = CardSpec::default();
        let mut settings = Settings::default();
        settings.overlay.scale = 1.0;
        let box_logical = hanglock_core::placement::swept_box(
            &cfg,
            &card,
            settings.overlay.hang,
            settings.overlay.margin,
            1.0,
        );
        let mon = Monitor {
            index: 0,
            bounds: Rect::new(0.0, 0.0, 1920.0 / scale, 1080.0 / scale),
            work: Rect::new(0.0, 0.0, 1920.0 / scale, (1080.0 - 48.0) / scale),
            scale,
            taskbar_top: false,
            taskbar_auto_hidden: false,
            primary: true,
        };
        let p = place(
            &mon,
            &cfg,
            &card,
            settings.overlay.hang,
            1.0,
            0.5,
            0.0,
            settings.overlay.margin,
            true,
        );
        (box_logical.w(), box_logical.h(), p.frame.w(), p.frame.h())
    }

    #[test]
    fn window_and_present_sizes_are_as_documented() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (lw, lh, dw, dh) = sizes(scale);
            assert!(
                (dw - lw * scale).abs() < 1.0,
                "device width is not logical x scale at {scale}"
            );
            // The full-window present is the swinging cost; the digits' box is the settled cost.
            let full_bytes = dw * dh * 4.0;
            let digits_bytes = 260.0 * scale * 60.0 * scale * 4.0;
            println!(
                "scale {scale:>4}: logical {:.0}x{:.0}  device {:.0}x{:.0}  full present {:.0} KiB  settled present {:.0} KiB",
                lw, lh, dw, dh, full_bytes / 1024.0, digits_bytes / 1024.0
            );
            assert!(
                full_bytes < 3.0 * 1024.0 * 1024.0,
                "full present exceeds 3 MiB at {scale}: too big a window"
            );
            // The band is ~9x smaller than the full window at every scale; multiplied by the
            // 60:1 present *rate* the idle saving compounds to ~540x per second.
            assert!(
                digits_bytes * 6.0 < full_bytes,
                "the digits' band must stay a small slice of the window"
            );
        }
    }
}
