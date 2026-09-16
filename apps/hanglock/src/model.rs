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

use hanglock_core::clock::format::{FaceOptions, FaceText, Civil};
use hanglock_core::ids::{ClickThrough, PostureKind};
use hanglock_core::placement::{place, Monitor, Rect};
use hanglock_core::rope::config::{CardSpec, Posture};
use hanglock_core::rope::Rope;
use hanglock_core::scene::Scene;
use hanglock_core::settings::Settings;
use hanglock_core::vec2::Vec2;
use hanglock_platform::Input;
use hanglock_platform::Command;
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
    /// Stop the message loop.
    Quit(i32),
    /// Nothing.
    None,
}

/// The cursor the adapter should show. Named here rather than imported from the backend so the model
/// can be tested without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Arrow,
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
}

impl HitRegions {
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
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
        (p.x - self.anchor.x).abs() <= self.anchor_radius && (p.y - self.anchor.y).abs() <= self.anchor_radius
    }

    #[must_use]
    pub fn on_ring(&self, p: Vec2) -> bool {
        if !self.interactive {
            return false;
        }
        let (dx, dy) = ((p.x - self.anchor.x).abs(), (p.y - self.anchor.y).abs());
        (dx <= self.anchor_radius && dy <= self.anchor_radius) && !self.on_plate(p)
    }

    #[must_use]
    pub fn on_plate(&self, p: Vec2) -> bool {
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let (dx, dy) = (p.x - self.plate_centre.x, p.y - self.plate_centre.y);
        ((dx * cs + dy * sn).abs() <= self.plate_hw) && ((-dx * sn + dy * cs).abs() <= self.plate_hh)
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
    /// The digits' box from the *previous* present. A present must cover both boxes: the text got
    /// narrower (`10:42` to `9:42`), and presenting only the new one would leave a fragment of the
    /// old `1` on screen for the next fifty-nine seconds.
    last_text_bounds: Rect,
    dirty_rects: [bool; 2],
    reposition: Option<(f64, f64)>,
    pub hover_ring: bool,
    pub saved: bool,
    pub counters: Counters,
    pub seconds_since_last_text: f64,
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

const HANG_STEPS: [f64; 6] = [70.0, 110.0, 150.0, 190.0, 230.0, 260.0];

impl Model {
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let theme = Theme::default();
        let card = card_for(&settings);
        let rope = Rope::new(
            hanglock_core::rope::config::RopeConfig::default(),
            card,
            posture_of(settings),
            Vec2::new(0.0, 0.0),
            1.0,
        );
        let text = face_of(&settings, (2026, 1, 1, 10, 42, 7));
        let m = Self {
            settings,
            rope,
            canvas: Canvas::new(1, 1),
            theme,
            state: State::Settled,
            text,
            layout_frame: Rect::new(0.0, 0.0, 1.0, 1.0),
            layout_scale: 1.0,
            monitor: None,
            last_text_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty_rects: [false; 2],
            reposition: None,
            hover_ring: false,
            saved: false,
            counters: Counters::default(),
            seconds_since_last_text: 0.0,
        };
        m
    }

    /// True while a ring drag is moving the whole clock. The adapter uses it to decide whether a
    /// release commits a position or simply ends a swing.
    #[must_use]
    pub fn is_repositioning(&self) -> bool {
        self.reposition.is_some()
    }

    /// Recompute where the overlay goes from the current monitor list. Returns the frame the
    /// adapter must apply, in device px.
    pub fn relayout(&mut self, monitors: &[Monitor], primary: u32) -> Rect {
        let chosen = pick(monitors, self.settings.overlay.monitor_index, primary);
        self.layout_scale = chosen.scale;
        self.monitor = Some(chosen);
        self.layout_frame = self.compute_frame(&chosen);
        let (w, h) = (self.layout_frame.w().max(1.0), self.layout_frame.h().max(1.0));
        let scale = self.layout_scale;
        let card = card_for(&self.settings);
        let anchor = self.anchor_in_frame(self.layout_frame, scale);
        // The rope lives in device px, and its host rect is the frame: the reach clamp plus this
        // rect are what keep the plate on the display it belongs to.
        self.rope.scale = scale;
        self.rope.host = Some(Rect::new(0.0, 0.0, w, h));
        self.rope.card = card;
        self.rope.anchor = anchor;
        self.rope.refit(scale, self.settings.overlay.hang);
        self.rope.set_anchor(anchor);
        self.canvas.resize(w as u32, h as u32);
        self.layout_frame
    }

    fn compute_frame(&self, m: &Monitor) -> Rect {
        let card = card_for(&self.settings);
        let p = place(
            m,
            &self.rope.cfg,
            &card,
            self.settings.overlay.hang,
            self.settings.overlay.scale,
            self.settings.overlay.anchor_ratio,
            self.settings.overlay.margin,
            self.settings.overlay.respect_taskbar,
        );
        p.frame
    }

    /// The anchor within the frame, device px: horizontally the frame's centre unless the user has
    /// nudged the clock along the top edge, vertically a fixed inset for the clamp.
    fn anchor_in_frame(&self, frame: Rect, scale: f64) -> Vec2 {
        Vec2::new(frame.w() * 0.5, 14.0 * scale)
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
        let f = self.layout_frame;
        Some(Rect::new(u.x0.max(f.x0), u.y0.max(f.y0), u.x1.min(f.x1), u.y1.min(f.y1)))
    }

    pub fn on_input(&mut self, input: Input) -> Vec<Action> {
        if self.state == State::Hidden {
            return Vec::new();
        }
        let mut out = Vec::new();
        match input {
            Input::Press { at } => {
                if self.rope.begin_drag(at, 10.0) {
                    self.state = State::Swinging;
                    out.push(Action::PresentFull);
                } else if self.ring_contains(at) {
                    let x0 = self.layout_frame.x0;
                    self.reposition = Some((at.x, x0));
                }
            }
            Input::Move { at, vel, dt } => {
                let _ = dt;
                if let (Some((start_x, _)), Some(m)) = (self.reposition, self.monitor) {
                    {
                        let dx = at.x - start_x;
                        let scale = self.layout_scale.max(0.25);
                        let logical_dx = dx / scale;
                        let ratio = (self.settings.overlay.anchor_ratio + logical_dx / m.bounds_logical().w().max(1.0)).clamp(0.0, 1.0);
                        self.settings.overlay.anchor_ratio = ratio;
                        let frame = self.compute_frame(&m);
                        self.layout_frame = frame;
                        let anchor = self.anchor_in_frame(frame, scale);
                        self.rope.host = Some(Rect::new(0.0, 0.0, frame.w().max(1.0), frame.h().max(1.0)));
                        self.rope.anchor = anchor;
                        self.rope.wake();
                        out.push(Action::Move(frame));
                        out.push(Action::PresentFull);
                    }
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
            Input::Release { at, mut vel, dt } => {
                let _ = at;
                let _ = dt;
                if let Some((_, _)) = self.reposition.take() {
                    out.push(Action::Save);
                    out.push(Action::Relayout);
                } else if self.rope.is_dragging() {
                    // The adapter's `vel` is only valid for moves; on release the last move's
                    // velocity is the truth, so the drag keeps whatever it was tracking.
                    vel = self.rope.drag.vel;
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

    pub fn on_command(&mut self, cmd: Command) -> Vec<Action> {
        use hanglock_core::settings::limits;
        let mut out = Vec::new();
        let o = &mut self.settings.overlay;
        let f = &mut self.settings.face;
        match cmd {
            Command::ToggleVisible => {
                o.enabled = !o.enabled;
                self.state = if o.enabled { State::Settled } else { State::Hidden };
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
                f.posture = p;
                self.rope.att.posture = posture_of(&self.settings);
                self.rope.wake();
                out.push(Action::Save);
                out.push(Action::PresentFull);
            }
            Command::HangUp | Command::HangDown => {
                let dir = if matches!(cmd, Command::HangUp) { 1 } else { -1 };
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
                let dir = if matches!(cmd, Command::Bigger) { 1.0 } else { -1.0 };
                o.scale = (o.scale + 0.15 * dir).clamp(limits::SCALE.0, limits::SCALE.1);
                self.rope.card = card_for(&self.settings);
                out.push(Action::Relayout);
                out.push(Action::Save);
            }
            Command::ResetPosition => {
                o.anchor_ratio = 0.5;
                o.hang = 150.0;
                o.scale = 1.0;
                out.push(Action::Relayout);
                out.push(Action::Save);
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

    pub fn on_system(&mut self, event: hanglock_platform::SystemEvent) -> Vec<Action> {
        match event {
            hanglock_platform::SystemEvent::DisplaysChanged | hanglock_platform::SystemEvent::Relayout => vec![Action::Relayout],
            hanglock_platform::SystemEvent::DpiChanged => vec![Action::Relayout],
            hanglock_platform::SystemEvent::Resumed => {
                // Drop the accumulated deficit rather than paying it: the rope has not been moving
                // while the machine slept, and "catching up" would look like a teleport. Re-read
                // the time, stay settled, and let the next real tick be an ordinary one.
                self.rope.acc_reset();
                if let Some(m) = self.monitor {
                    self.layout_frame = self.compute_frame(&m);
                }
                vec![Action::Relayout]
            }
        }
    }

    // ---------------------------------------------------------------------------------------------
    // painting and presentation
    // ---------------------------------------------------------------------------------------------

    #[must_use]
    pub fn scene(&self) -> Scene {
        Scene::new(&self.rope, &self.rope.cfg, self.text, self.settings.overlay.opacity)
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
            interactive: self.settings.overlay.click_through == ClickThrough::Hover,
        }
    }

    #[must_use]
    pub fn cursor(&self) -> CursorKind {
        if self.rope.is_dragging() {
            if self.reposition.is_some() {
                CursorKind::Move
            } else {
                CursorKind::Grabbing
            }
        } else if self.reposition.is_some() {
            CursorKind::Move
        } else if self.hover_ring {
            CursorKind::Move
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

    #[must_use]
    pub fn menu_state(&self) -> (bool, bool, bool, bool, PostureKind) {
        (
            self.settings.overlay.enabled,
            self.settings.overlay.topmost,
            self.settings.face.seconds,
            self.settings.face.hour12,
            self.settings.face.posture,
        )
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

fn card_for(s: &Settings) -> CardSpec {
    // The card's *design* size is fixed; `overlay.scale` is a user multiplier applied where the model
    // already carries a scale (the display's), so one product of scales reaches the painter and the
    // solver together.
    let base = CardSpec::default();
    let k = s.overlay.scale;
    CardSpec { width: base.width * k, height: base.height * k, bracket: base.bracket, corner: base.corner, hang: s.overlay.hang }
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
    let c = Civil { year: i64::from(year), month, day, hour, minute, second };
    hanglock_core::clock::format::format(
        &c,
        &FaceOptions { hour12: s.face.hour12, seconds: s.face.seconds, meridiem: s.face.meridiem },
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
    println!("  paint            {:>8.1} us/frame   (budget 400 us)", per / 1000.0);
    println!("  buffer           {:>8.1} KiB", (w * h * 4) as f64 / 1024.0);
    println!("  present copy     {:>8.1} KiB/frame full-window", (w * h * 4) as f64 / 1024.0);
    println!("  solve+paint/sec  {:>8.1} ms of a 1000 ms budget at 60 Hz", per * 60.0 / 1e6);
}

pub fn diag(settings: &Settings) -> String {
    let mut m = Model::new(*settings);
    let m0 = Monitor {
        index: 0,
        bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        work: Rect::new(0.0, 0.0, 1920.0, 1040.0),
        scale: settings.overlay.scale,
        taskbar_top: false,
        taskbar_auto_hidden: false,
        primary: true,
    };
    let frame = m.relayout(&[m0], 0);
    let mut out = String::new();
    out.push_str(&format!("hanglock {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("  settings file   {}\n", crate::store::path().display()));
    out.push_str(&format!("  frame (device)  {:.0},{:.0} {:.0}x{:.0}\n", frame.x0, frame.y0, frame.w(), frame.h()));
    out.push_str(&format!("  anchor (device) {:.1},{:.1}\n", m.rope.anchor.x, m.rope.anchor.y));
    out.push_str(&format!("  hang            {:.0} logical px, {:.1} device px, {} links\n", m.settings.overlay.hang, m.rope.hang, m.rope.cfg.segments));
    out.push_str(&format!("  posture         {}\n", m.settings.face.posture.as_str()));
    out.push_str(&format!("  click through   {}\n", m.settings.overlay.click_through.as_str()));
    out.push_str(&format!("  state           {:?}, sleeping={}\n", m.state, m.rope.sleeping));
    m.paint();
    out.push_str(&format!("  canvas          {}x{}, present_rect {:?}", m.canvas.size().0, m.canvas.size().1, m.canvas.present_rect().is_some()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanglock_core::settings::Settings;

    fn model() -> Model {
        let mut m = Model::new(Settings::default());
        let mon = Monitor {
            index: 0,
            bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            work: Rect::new(0.0, 0.0, 1920.0, 1040.0),
            scale: 1.0,
            taskbar_top: false,
            taskbar_auto_hidden: false,
            primary: true,
        };
        m.relayout(&[mon], 0);
        m
    }

    #[test]
    fn the_frame_fits_the_display_and_the_anchor_is_inside_it() {
        let m = model();
        let f = m.layout_frame;
        assert!(f.x0 >= -0.01 && f.x1 <= 1920.0 + 0.01, "frame {:?} escaped the monitor", f);
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
            m.rope.move_drag(Vec2::new(m.rope.anchor.x + 120.0, m.rope.anchor.y + 140.0 + i as f64), Vec2::new(600.0, 0.0));
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
        assert!(m.wants_ticks() == false, "the app never stopped asking for frames");
        assert!(actions_with_motion < 400, "too many presented frames while settling: {actions_with_motion}");
        assert!(m.state == State::Settled);
    }

    #[test]
    fn a_second_that_changed_nothing_presents_nothing() {
        let mut m = model();
        // No seconds displayed: the minute boundary changes nothing visible.
        assert!(!m.settings.face.seconds);
        let a = m.on_second((2026, 9, 15, 22, 42, 0));
        let b = m.on_second((2026, 9, 15, 22, 42, 59));
        assert!(
            a.iter().all(|x| matches!(x, Action::None)),
            "an unchanged face must not present: {a:?}"
        );
        assert!(b.iter().all(|x| matches!(x, Action::None)) || b.iter().any(|x| matches!(x, Action::PresentRect(_))));
    }

    #[test]
    fn a_second_that_changed_the_digits_presents_only_their_box() {
        let mut m = model();
        m.settings.face.seconds = true;
        m.text = FaceText::default();
        let a = m.on_second((2026, 9, 15, 22, 42, 7));
        let rect = a.iter().find_map(|x| match x {
            Action::PresentRect(r) => Some(*r),
            _ => None,
        });
        let rect = rect.expect("a changed face must present a rect while settled");
        let f = m.layout_frame;
        let digits_area = (rect.x1 - rect.x0) * (rect.y1 - rect.y0);
        let whole = f.w() * f.h();
        assert!(digits_area > 0.0);
        assert!(digits_area < whole * 0.35, "presenting {:.0}% of the window for a digit change", digits_area / whole * 100.0);
    }

    #[test]
    fn the_wheel_steps_the_hang_and_asks_for_a_relayout() {
        let mut m = model();
        let before = m.settings.overlay.hang;
        let a = m.on_input(Input::Wheel { delta: 1 });
        assert!(a.contains(&Action::Relayout), "a longer hang needs a taller window: {a:?}");
        assert!(m.settings.overlay.hang > before);
        let top = m.settings.overlay.hang;
        m.on_input(Input::Wheel { delta: 1 });
        m.on_input(Input::Wheel { delta: 1 });
        assert_eq!(m.settings.overlay.hang, top, "the longest step must stick at the end of the range");
        for _ in 0..10 {
            m.on_input(Input::Wheel { delta: -1 });
        }
        assert!(m.settings.overlay.hang < before, "the wheel must be able to come back");
    }

    #[test]
    fn clicking_through_always_disables_hit_testing() {
        let mut m = model();
        assert!(m.hit_regions().interactive);
        m.on_command(Command::SetPosture(PostureKind::Locked));
        m.settings.overlay.click_through = ClickThrough::Always;
        assert!(!m.hit_regions().interactive);
        let p = m.rope.card_centre();
        assert!(!m.hit_regions().contains(p), "with click_through=always nothing may be clickable");
    }

    #[test]
    fn the_ring_and_the_plate_are_both_reachable_and_distinguishable() {
        let m = model();
        let r = m.hit_regions();
        let centre = r.plate_centre;
        assert!(r.on_plate(centre), "the plate's own centre must be on the plate");
        let a = r.anchor;
        assert!(r.on_ring(a), "the ring must be grabbable: it is how the clock is moved");
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
            assert!(a.contains(&Action::Save), "{cmd:?} changed settings without persisting");
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
        assert_eq!(before, after, "a resume must not move the object; the deficit is dropped");
    }

    #[test]
    fn moving_the_ring_moves_the_clock_and_persists_a_ratio() {
        let mut m = model();
        let a = m.rope.anchor;
        m.on_input(Input::Press { at: Vec2::new(a.x, a.y) });
        assert!(m.is_repositioning(), "pressing the ring must start a reposition");
        let moved = m.on_input(Input::Move { at: Vec2::new(a.x + 300.0, a.y), vel: Vec2::ZERO, dt: 1.0 / 60.0 });
        assert!(moved.iter().any(|x| matches!(x, Action::Move(_))), "the window must follow the ring");
        let ratio = m.settings.overlay.anchor_ratio;
        assert!(ratio > 0.5, "dragging right must increase the ratio, got {ratio}");
        m.on_input(Input::Release { at: Vec2::new(a.x + 300.0, a.y), vel: Vec2::ZERO, dt: 1.0 / 60.0 });
        assert!((m.settings.overlay.anchor_ratio - ratio).abs() < 1e-9, "the ratio was committed at release time");
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

    /// The cost model in `docs/gate-a.md` in one place: how big the window is, and how many pixels a
    /// present moves, at the scales Windows users actually run. These are exact counts, not guesses —
    /// they are what the presenter's memcpy does per frame.
    fn sizes(scale: f64) -> (f64, f64, f64, f64) {
        let cfg = hanglock_core::rope::config::RopeConfig::default();
        let card = CardSpec::default();
        let mut settings = Settings::default();
        settings.overlay.scale = 1.0;
        let box_logical = hanglock_core::placement::swept_box(&cfg, &card, settings.overlay.hang, settings.overlay.margin, 1.0);
        let mon = Monitor {
            index: 0,
            bounds: Rect::new(0.0, 0.0, 1920.0 / scale, 1080.0 / scale),
            work: Rect::new(0.0, 0.0, 1920.0 / scale, (1080.0 - 48.0) / scale),
            scale,
            taskbar_top: false,
            taskbar_auto_hidden: false,
            primary: true,
        };
        let p = place(&mon, &cfg, &card, settings.overlay.hang, 1.0, 0.5, settings.overlay.margin, true);
        (box_logical.w(), box_logical.h(), p.frame.w(), p.frame.h())
    }

    #[test]
    fn window_and_present_sizes_are_as_documented() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (lw, lh, dw, dh) = sizes(scale);
            assert!((dw - lw * scale).abs() < 1.0, "device width is not logical x scale at {scale}");
            // The full-window present is the swinging cost; the digits' box is the settled cost.
            let full_bytes = dw * dh * 4.0;
            let digits_bytes = 260.0 * scale * 60.0 * scale * 4.0;
            println!(
                "scale {scale:>4}: logical {:.0}x{:.0}  device {:.0}x{:.0}  full present {:.0} KiB  settled present {:.0} KiB",
                lw, lh, dw, dh, full_bytes / 1024.0, digits_bytes / 1024.0
            );
            assert!(full_bytes < 3.0 * 1024.0 * 1024.0, "full present exceeds 3 MiB at {scale}: too big a window");
            assert!(digits_bytes * 30.0 < full_bytes, "the 1 Hz present must be far cheaper than a 60 Hz full present");
        }
    }
}
