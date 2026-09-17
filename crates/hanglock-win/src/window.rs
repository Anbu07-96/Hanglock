//! The overlay window: styles, message handling, and the two timers that drive everything.
//!
//! ## Where the idle budget comes from
//!
//! There is no thread, no wait loop, and no cursor poll in this file. When the clock is settled the
//! only thing outstanding is a 1 Hz `SetTimer`, so the process sits inside `GetMessageW` and the
//! OS charges us nothing at all until a message arrives. That is the structural advantage over the
//! reference implementation, which must tick at 30 Hz while settled purely to *notice* the pointer
//! has arrived; on Windows, per-pixel hit testing delivers `WM_MOUSEMOVE` and `WM_NCHITTEST`
//! itself, so hover is free and an idle Hanglock costs zero frames rather than 30 of them.
//!
//! ## Lifetime of the state pointer
//!
//! `Runtime` lives on `run`'s stack for the whole message loop and its address is stored in
//! `GWL_USERDATA`. That is sound here because the loop cannot outlive the frame, and nothing can
//! re-enter it after `run` returns. `CreateWindowExW` sends `WM_CREATE` synchronously, *before* the
//! address is written, so the wndproc must tolerate a null userdata — it does, by deferring to
//! `DefWindowProcW`.
//!
//! ## Two timers
//!
//! `TICK` runs only while the rope is moving (its interval is the fps cap) and `SECOND` runs always,
//! because a clock's steady state is one digit update a second and that is a separate question from
//! whether the physics needs a frame. Keeping them apart is what lets a settled clock cost a small
//! rect copy per second instead of a full-rate animation loop with nothing in it.

use crate::displays;
use crate::surface::Surface;
use crate::sys;
use crate::tray::{self, Tray};
use hanglock_core::ids::ClickThrough;
use hanglock_core::placement::{Monitor, Rect};
use hanglock_core::vec2::Vec2;
use hanglock_platform::panel::{Group, RowId, Step};
use hanglock_platform::{Command, Dialogs, Input, OverlayHost, SystemEvent};

pub const TIMER_TICK: usize = 1;
pub const TIMER_SECOND: usize = 2;
const CLASS_NAME: &str = "HanglockOverlay";

/// Cursor the hook wants. Mapped from the two states the interaction has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Arrow,
    /// Open hand: grabbable, not held.
    Grab,
    /// Closed hand: held, dragging.
    Grabbing,
    /// The re-anchor affordance.
    Move,
}

/// The shapes the window is interactive over, in device px: the plate's oriented rectangle and the
/// ring it hangs from. Handed to `WM_NCHITTEST` by the app, so the clickable region cannot drift from
/// the painted one — the two are computed from the same node positions every frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct HitShape {
    pub centre: Vec2,
    pub hw: f64,
    pub hh: f64,
    pub theta: f64,
    /// False turns the whole window transparent to the mouse: `click_through = "always"`.
    pub interactive: bool,
    /// Extra reach, device px. Generous, because the tip of a finger on a trackpad is not a pixel.
    pub pad: f64,
    /// The hang ring, and its half-size: grabbing this moves the clock instead of swinging it. Left
    /// zero by a caller with no ring, which degrades to plate-only hit testing.
    pub anchor: Vec2,
    pub anchor_radius: f64,
    /// Whether the *whole* window answers the mouse, plate and empty space alike. `false` is what makes
    /// a clock with a soft edge feel like part of the wallpaper; `true` is `click_through = "solid"`,
    /// where the plate's bounding rectangle is a window even where nothing is drawn, which is what a
    /// user wants when the card overlaps a busy background and the anti-aliased border keeps stealing
    /// a click.
    pub whole_window: bool,
}

impl HitShape {
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        if !self.interactive {
            return false;
        }
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let (dx, dy) = (p.x - self.centre.x, p.y - self.centre.y);
        let lx = dx * cs + dy * sn;
        let ly = -dx * sn + dy * cs;
        if lx.abs() <= self.hw + self.pad && ly.abs() <= self.hh + self.pad {
            return true;
        }
        (p.x - self.anchor.x).abs() <= self.anchor_radius
            && (p.y - self.anchor.y).abs() <= self.anchor_radius
    }

    /// Whether the point is on the ring and not on the plate — the re-anchor gesture.
    #[must_use]
    pub fn on_anchor(&self, p: Vec2) -> bool {
        if !self.interactive {
            return false;
        }
        let (cs, sn) = (self.theta.cos(), self.theta.sin());
        let (dx, dy) = (p.x - self.centre.x, p.y - self.centre.y);
        let on_plate = (dx * cs + dy * sn).abs() <= self.hw + self.pad
            && (-dx * sn + dy * cs).abs() <= self.hh + self.pad;
        !on_plate
            && (p.x - self.anchor.x).abs() <= self.anchor_radius
            && (p.y - self.anchor.y).abs() <= self.anchor_radius
    }
}

/// What a backend must let an application do. Implemented by the app; called from the wndproc.
pub trait AppHook {
    /// First thing after the window exists. Set the frame, present, choose the tick rate.
    fn on_ready(&mut self, host: &mut Host);
    /// A frame is due, because the rope is awake.
    fn on_frame(&mut self, host: &mut Host, dt: f64);
    /// The wall clock advanced a second. Also the wake signal for a settled rope.
    fn on_second(&mut self, host: &mut Host);
    fn on_input(&mut self, host: &mut Host, input: Input);
    fn on_command(&mut self, host: &mut Host, cmd: Command);
    fn on_system(&mut self, host: &mut Host, event: SystemEvent);
    /// Queried by `WM_NCHITTEST`, so it must not need to mutate anything.
    fn hit_shape(&self) -> HitShape;
    /// What a click on one row of the settings window asks for. The window has no idea what a setting
    /// is: it reports a row and a step, and the app decides — because the answer has to go through the
    /// same `on_command` the tray uses, or the two surfaces would hold two different ideas of what a
    /// checkbox does.
    fn panel_command(&mut self, host: &mut Host, id: RowId, step: Step);
    fn cursor(&self) -> Cursor {
        Cursor::Arrow
    }
    /// Whether the rope moved this tick: the host keeps the tick timer alive only while it is true.
    fn wants_ticks(&self) -> bool {
        false
    }
    /// Tick rate to run at while `wants_ticks`, clamped by the user's setting.
    fn preferred_rate(&self) -> u32 {
        60
    }
    /// The checkmark state for the menu. A snapshot, so the tray never reaches into settings, and
    /// built with `&mut Host` only because the monitor list and the tray's presence are the two facts
    /// the menu cannot know from the document alone.
    fn menu_state(&self, host: &mut Host) -> tray::MenuState;
}

/// The environment handed back to the hook: the window, the surface, the tray, the clock.
pub struct Host {
    hwnd: sys::HWND,
    surface: Surface,
    /// Monotonic delta source for the physics. `GetTickCount` steps on a 10-16 ms quantum, which a
    /// 240 Hz accumulator would notice as judder in the swing; nothing else here needs a clock, so
    /// there is one instance and it lives where the ticks arrive.
    clock: crate::time::PerfClock,
    tray: Option<Tray>,
    /// Last known client rect, device px. Maintained locally so a query never has to round-trip to
    /// the window server — this is called from the hit test, which the OS runs on its own schedule.
    frame: Rect,
    scale: f64,
    topmost: bool,
    visible: bool,
    /// Which input styles are already on the HWND, so a style write happens on change only.
    input: ClickThrough,
    rate_hz: u32,
    tick_ms: u32,
    /// The settings window, or null. `HWND` rather than an `Option` because it is a Win32 handle whose
    /// liveness is answered by `IsWindow`, not by this field: a window the user closed with its `x` is
    /// gone without any message reaching here, and `settings_open` is where that is checked.
    panel: sys::HWND,
    /// The `Runtime` this host lives in, for the settings window's click path. Same lifetime rule as
    /// the window's own `GWLP_USERDATA`, documented at the top of this file.
    runtime: *mut core::ffi::c_void,
    instance: sys::HMODULE,
    /// Set by `run` for the concrete app type, because opening a window needs a window procedure, and
    /// a window procedure needs to know which app to call. A function pointer rather than a generic
    /// method so [`Dialogs`] can be implemented on `Host`, which is not generic.
    open_panel: crate::panel::OpenFn,
    /// The screen cursor position at the last move message. Velocity is measured from this and not from
    /// the message's own coordinates, which are window-local and wrap once the pointer is captured and
    /// outside the swept box; see `WM_MOUSEMOVE`.
    last_cursor: sys::POINT,
    /// True while the left button is held and captured, so a drag that leaves the window keeps
    /// arriving — which it must, because the pointer can outrun the swept box mid-throw and an
    /// uncaptured drag would strand the plate mid-swing with no release to end it.
    captured: bool,
    exit: i32,
}

#[derive(Clone, Copy)]
pub struct OverlayConfig {
    /// Initial size, device px. `on_ready` normally replaces it with a `place()`-derived rect.
    pub frame: Rect,
    pub scale: f64,
    pub title: &'static str,
    pub tooltip: &'static str,
    pub topmost: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            frame: Rect::new(0.0, 0.0, 640.0, 320.0),
            scale: 1.0,
            title: "Hanglock",
            tooltip: "Hanglock",
            topmost: true,
        }
    }
}

pub struct Runtime<A> {
    pub(crate) app: A,
    pub(crate) host: Host,
}

/// Register the class and create the layered window. `Err(2)`/`Err(3)` are the process exit
/// codes this contract reserves for class- and window-creation failure, kept distinct.
///
/// # Safety
/// `instance` must be a live module handle for this process — `GetModuleHandleW(null)` gives
/// one, and the class name and window procedure are ours.
unsafe fn spawn_window<A: AppHook + 'static>(
    instance: sys::HMODULE,
    cfg: OverlayConfig,
) -> Result<sys::HWND, u8> {
    // SAFETY: caller's contract — a live instance handle for this process.
    unsafe {
        let class = sys::wide(CLASS_NAME);
        let wc = sys::WNDCLASSEXW {
            cb_size: std::mem::size_of::<sys::WNDCLASSEXW>() as u32,
            // CS_HREDRAW|CS_VREDRAW would force a full invalidate on resize; a layered window
            // has no paint cycle to invalidate, so no class styles at all.
            style: 0,
            lpfn_wnd_proc: Some(wndproc::<A>),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: instance,
            h_icon: std::ptr::null_mut(),
            h_cursor: std::ptr::null_mut(),
            h_br_background: std::ptr::null_mut(),
            lpsz_menu_name: std::ptr::null(),
            lpsz_class_name: class.as_ptr(),
            h_icon_sm: std::ptr::null_mut(),
        };
        if sys::RegisterClassExW(&wc) == 0 {
            return Err(2);
        }
        let title = sys::wide(cfg.title);
        let ex = sys::WS_EX_LAYERED
            | sys::WS_EX_TOOLWINDOW
            | sys::WS_EX_NOACTIVATE
            | if cfg.topmost { sys::WS_EX_TOPMOST } else { 0 };
        let f = cfg.frame;
        let hwnd = sys::CreateWindowExW(
            ex,
            class.as_ptr(),
            title.as_ptr(),
            sys::WS_POPUP,
            f.x0 as i32,
            f.y0 as i32,
            f.w().max(1.0) as i32,
            f.h().max(1.0) as i32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return Err(3);
        }
        Ok(hwnd)
    }
}

/// The `Runtime`, before it has a window: everything the overlay owns starts empty here, and the parts
/// that cannot be defaulted — the frame from the config, and the panel constructor the app crate installs
/// once `A` is known — are set by [`run`], which is where those inputs exist.
fn new_runtime<A: AppHook + 'static>(app: A, cfg: &OverlayConfig) -> Runtime<A> {
    Runtime {
        app,
        host: Host {
            hwnd: std::ptr::null_mut(),
            surface: Surface::new(),
            clock: crate::time::PerfClock::new(),
            tray: None,
            frame: cfg.frame,
            scale: cfg.scale.max(0.25),
            topmost: cfg.topmost,
            visible: false,
            input: ClickThrough::Hover,
            rate_hz: 0,
            tick_ms: 0,
            panel: std::ptr::null_mut(),
            runtime: std::ptr::null_mut(),
            instance: std::ptr::null_mut(),
            open_panel: |_, _, _| std::ptr::null_mut(),
            last_cursor: sys::POINT { x: 0, y: 0 },
            captured: false,
            exit: 0,
        },
    }
}

/// Create the overlay, wire it to `app`, and run the message loop. Returns the process exit code.
pub fn run<A: AppHook + 'static>(app: A, cfg: OverlayConfig) -> i32 {
    // Must happen before any window is created: after the first window exists the process's DPI
    // context is fixed, and setting it later silently succeeds while changing nothing.
    unsafe {
        sys::apply_dpi_awareness();
    }
    let instance = unsafe { sys::GetModuleHandleW(std::ptr::null()) };
    let mut rt = new_runtime(app, &cfg);
    let me: *mut Runtime<A> = &mut rt;

    let f = cfg.frame;
    let hwnd = match unsafe { spawn_window::<A>(instance, cfg) } {
        Ok(h) => h,
        Err(code) => return i32::from(code),
    };
    rt.host.hwnd = hwnd;
    rt.host.instance = instance;
    rt.host.runtime = me.cast();
    rt.host.open_panel = |state, module, groups| unsafe {
        crate::panel::open(state, module, groups, crate::panel::answer::<A>)
    };
    unsafe {
        sys::SetWindowLongPtrW(hwnd, sys::GWLP_USERDATA, me as isize);
    }
    rt.host.surface.resize(f.w() as u32, f.h() as u32);
    rt.host.tray = Some(unsafe { Tray::new(hwnd, instance) });
    if let Some(t) = rt.host.tray.as_mut() {
        t.install(cfg.tooltip);
    }

    unsafe {
        // The system's timer resolution is 10-16 ms by default, which would make a 16.7 ms frame
        // arrive in bursts. Raised for the life of the process and released on the way out; the
        // cost is a slightly less sleepy tick source, and it buys a smooth swing.
        sys::timeBeginPeriod(1);
        sys::SetTimer(hwnd, TIMER_SECOND, 1000, None);
    }
    rt.host.show(true);
    rt.app.on_ready(&mut rt.host);
    rt.host.sync_tick_timer();

    let mut msg = sys::MSG::default();
    loop {
        let got = unsafe { sys::GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if got == 0 {
            break;
        }
        if got < 0 {
            // -1 means the call failed; treat it as a quit rather than spinning.
            break;
        }
        // Tab, Shift+Tab, the arrows inside a radio group, Space and Esc. Handled here rather than by
        // a dialog class because the settings window is a plain window with plain children, and this
        // one call is the whole of what makes it keyboard-navigable. Scoped to the panel's own message
        // queue: the overlay must never have its input run through a dialog manager, because a loop that
        // swallowed one message while the pointer was over the clock would show up as a dropped drag.
        let mut handled = false;
        unsafe {
            if !rt.host.panel.is_null() && sys::GetAncestor(msg.hwnd, sys::GA_ROOT) == rt.host.panel
            {
                handled = sys::IsDialogMessageW(rt.host.panel, &msg) != 0;
            }
        }
        if !handled {
            unsafe {
                sys::TranslateMessage(&msg);
                sys::DispatchMessageW(&msg);
            }
        }
    }
    unsafe {
        sys::KillTimer(hwnd, TIMER_TICK);
        sys::KillTimer(hwnd, TIMER_SECOND);
        sys::timeEndPeriod(1);
        if let Some(t) = rt.host.tray.as_mut() {
            t.uninstall();
        }
        if rt.host.settings_open() {
            // The `Panel` the window owns is freed in its own `WM_NCDESTROY`, so it has to be destroyed
            // while the `Runtime` it points back at is still on the stack.
            sys::DestroyWindow(rt.host.panel);
        }
        sys::SetWindowLongPtrW(hwnd, sys::GWLP_USERDATA, 0);
        sys::DestroyWindow(hwnd);
    }
    rt.host.exit
}

impl Host {
    /// The frame clock asks this after every tick: it keeps the `TICK` timer alive only while the
    /// rope actually wants frames, and drops it to nothing when the object settles.
    pub fn sync_tick_timer(&mut self) {
        let want = if self.rate_hz == 0 {
            0
        } else {
            (1000 / self.rate_hz.max(1)).max(8)
        };
        if want != self.tick_ms {
            unsafe {
                if want == 0 {
                    sys::KillTimer(self.hwnd, TIMER_TICK);
                } else {
                    sys::SetTimer(self.hwnd, TIMER_TICK, want, None);
                }
            }
            self.tick_ms = want;
        }
    }

    pub fn set_rate(&mut self, hz: u32) {
        self.rate_hz = hz;
    }

    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate_hz
    }

    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    #[must_use]
    pub fn hwnd(&self) -> sys::HWND {
        self.hwnd
    }

    /// `WS_EX_LAYERED` is on in every mode, because it is what the painted surface is presented
    /// through; the difference between the two interactive modes is therefore not a style at all but
    /// what `WM_NCHITTEST` answers, which is why this writes one bit and `HitShape::whole_window`
    /// carries the other half.
    #[must_use]
    pub fn input_mode(&self) -> ClickThrough {
        self.input
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Push a painted frame. `rect` should be the painter's dirty bounds; `None` presents all of it.
    pub fn present(&mut self, pixels: &[u8], rect: Option<Rect>) {
        unsafe { self.surface.present(self.hwnd, pixels, rect) };
    }

    #[must_use]
    pub fn surface_size(&self) -> (u32, u32) {
        self.surface.size()
    }

    #[must_use]
    pub fn monitors(&self) -> Vec<Monitor> {
        let mut v = unsafe { displays::enumerate(self.hwnd) };
        if v.is_empty() {
            v.push(unsafe { displays::monitor_for(self.hwnd, 0) });
        }
        v
    }

    #[must_use]
    pub fn monitor_under(&self) -> Monitor {
        unsafe { displays::monitor_for(self.hwnd, 0) }
    }

    #[must_use]
    pub fn now_ms(&self) -> i64 {
        crate::time::unix_ms()
    }

    #[must_use]
    pub fn local_offset_secs(&self) -> i32 {
        crate::time::local_offset_secs()
    }

    /// `(year, month, day, hour, minute, second)` as the OS displays them.
    #[must_use]
    pub fn local_fields(&self) -> (u32, u32, u32, u32, u32, u32) {
        crate::time::local_fields()
    }

    #[must_use]
    pub fn elapsed(&mut self) -> f64 {
        self.clock.tick()
    }

    /// The tray menu, or the card's context menu: the same list, from the same code.
    #[must_use]
    pub fn popup_menu(&mut self, at_screen: (i32, i32), st: tray::MenuState) -> Option<Command> {
        self.tray.as_mut().and_then(|t| t.show_menu(at_screen, &st))
    }

    pub fn set_tooltip(&mut self, text: &str) {
        if let Some(t) = self.tray.as_mut() {
            t.set_tooltip(text);
        }
    }

    #[must_use]
    pub fn tray_installed(&self) -> bool {
        self.tray.as_ref().is_some_and(tray::Tray::is_installed)
    }

    pub fn quit(&mut self, code: i32) {
        self.exit = code;
        unsafe {
            sys::PostQuitMessage(code);
        }
    }

    fn set_cursor(c: Cursor) {
        let id = match c {
            Cursor::Arrow => sys::IDC_ARROW,
            Cursor::Grab | Cursor::Grabbing | Cursor::Move => sys::IDC_SIZEALL,
        };
        unsafe {
            let cur = sys::LoadCursorW(std::ptr::null_mut(), id);
            if !cur.is_null() {
                sys::SetCursor(cur);
            }
        }
    }
}

impl OverlayHost for Host {
    fn set_topmost(&mut self, topmost: bool) {
        if self.topmost == topmost {
            return;
        }
        self.topmost = topmost;
        unsafe {
            sys::SetWindowPos(
                self.hwnd,
                if topmost {
                    sys::HWND_TOPMOST
                } else {
                    sys::HWND_NOTOPMOST
                },
                0,
                0,
                0,
                0,
                sys::SWP_NOMOVE | sys::SWP_NOSIZE | sys::SWP_NOACTIVATE | sys::SWP_NOREDRAW,
            );
        }
    }

    fn set_frame(&mut self, frame: Rect) {
        let w = frame.w().max(1.0) as i32;
        let h = frame.h().max(1.0) as i32;
        let (cw, ch) = self.surface.size();
        if cw != w.max(1) as u32 || ch != h.max(1) as u32 {
            self.surface.resize(w.max(1) as u32, h.max(1) as u32);
        }
        if (frame.x0 - self.frame.x0).abs() > 0.5
            || (frame.y0 - self.frame.y0).abs() > 0.5
            || self.frame.w() as i32 != w
            || self.frame.h() as i32 != h
        {
            unsafe {
                sys::SetWindowPos(
                    self.hwnd,
                    if self.topmost {
                        sys::HWND_TOPMOST
                    } else {
                        sys::HWND_NOTOPMOST
                    },
                    frame.x0 as i32,
                    frame.y0 as i32,
                    w,
                    h,
                    sys::SWP_NOACTIVATE | sys::SWP_NOREDRAW | sys::SWP_NOOWNERZORDER,
                );
            }
        }
        self.frame = frame;
    }

    /// The three mouse modes are two styles, and the pair has to be written together.
    ///
    /// `Hover` is `WS_EX_LAYERED | WS_EX_TRANSPARENT`: layered gives the per-pixel alpha and the
    /// per-pixel click-through the hit test answers, transparent says the window is not interested in
    /// what it did not claim. `Solid` drops `WS_EX_TRANSPARENT` and keeps the layered surface, so every
    /// pixel of the frame belongs to us — which is why the app's hit shape then answers `HTCLIENT` for
    /// the whole rectangle. `Always` keeps both bits and the overlay stops being a target at all, which
    /// is the mode the tray has to be able to undo (see `tray_present`).
    fn set_input_mode(&mut self, mode: ClickThrough) {
        if self.input == mode {
            return;
        }
        self.input = mode;
        unsafe {
            let ex = sys::GetWindowLongPtrW(self.hwnd, sys::GWL_EXSTYLE);
            let bit = sys::WS_EX_TRANSPARENT as isize;
            let next = if mode.ignores_input() {
                ex | bit
            } else {
                ex & !bit
            };
            if next != ex {
                sys::SetWindowLongPtrW(self.hwnd, sys::GWL_EXSTYLE, next);
            }
        }
    }

    /// Show or hide. Hiding keeps the window — a `ShowWindow(SW_HIDE)` costs nothing to keep alive and
    /// re-showing is instant — and while hidden the tick timer is off, so a hidden app still costs
    /// nothing at all.
    fn show(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        unsafe {
            if visible {
                // Show without activating: `SW_SHOWNA` is the only variant that does not steal focus.
                sys::ShowWindow(self.hwnd, sys::SW_SHOWNA);
                sys::UpdateWindow(self.hwnd);
            } else {
                sys::ShowWindow(self.hwnd, sys::SW_HIDE);
            }
        }
    }

    #[must_use]
    fn frame(&self) -> Rect {
        self.frame
    }

    fn tray_present(&self) -> bool {
        self.tray_installed()
    }
}

impl Dialogs for Host {
    /// Create the settings window, or raise the one that is already up.
    ///
    /// The rows arrive from the app rather than being read here, which is what keeps this crate free of
    /// `Settings`: the backend draws what it is told and reports clicks, and never decides what a value
    /// means.
    fn show_settings(&mut self, groups: Vec<Group>) {
        if self.settings_open() {
            unsafe { crate::panel::focus(self.panel) };
            self.sync_settings(&groups);
            return;
        }
        let state = self.runtime;
        let module = self.instance;
        self.panel = unsafe { (self.open_panel)(state, module, groups) };
    }

    fn sync_settings(&mut self, groups: &[Group]) {
        if self.settings_open() {
            unsafe { crate::panel::refresh(self.panel, groups.to_vec()) };
        }
    }

    /// `IsWindow` rather than a flag, because the user closes this window with its own `x` or with Esc,
    /// and no message of that closure reaches this struct: the handle is the state, and the only honest
    /// question to ask it is whether it still names a window.
    #[must_use]
    fn settings_open(&self) -> bool {
        !self.panel.is_null() && (unsafe { sys::IsWindow(self.panel) }) != 0
    }

    fn close_settings(&mut self) {
        if self.settings_open() {
            unsafe {
                sys::DestroyWindow(self.panel);
            }
        }
        self.panel = std::ptr::null_mut();
    }

    /// A modal message box, on purpose: the About text cannot change, so there is nothing to keep in
    /// sync, and a window the user has to remember to close would be a second thing to manage.
    fn about(&mut self, text: &str) {
        let body = sys::wide(text);
        let title = sys::wide("About Hanglock");
        unsafe {
            sys::MessageBoxW(
                self.hwnd,
                body.as_ptr(),
                title.as_ptr(),
                sys::MB_OK | sys::MB_ICONINFORMATION,
            );
        }
    }
}

/// The one window procedure. Every branch here either translates an OS event into something the
/// model understands, or answers a question the model can answer cheaply.
unsafe extern "system" fn wndproc<A: AppHook + 'static>(
    hwnd: sys::HWND,
    msg: u32,
    w: usize,
    l: isize,
) -> isize {
    // SAFETY: the contract of a window procedure — `hwnd` is live, and GWLP_USERDATA either
    // holds the null sentinel or the one `Runtime` pointer installed for it, which outlives
    // every message the OS dispatches to this window.
    let ud = unsafe { sys::GetWindowLongPtrW(hwnd, sys::GWLP_USERDATA) };
    if ud == 0 {
        // Includes WM_CREATE/WM_NCCREATE, which CreateWindowExW sends before the pointer exists.
        return unsafe { sys::DefWindowProcW(hwnd, msg, w, l) };
    }
    let rt = unsafe { &mut *(ud as *mut Runtime<A>) };
    let host = &mut rt.host;
    let app = &mut rt.app;

    if let Some(answer) = unsafe { on_pointer::<A>(hwnd, msg, w, l, host, app) } {
        return answer;
    }

    match msg {
        sys::WM_TIMER => {
            if w == TIMER_TICK {
                let dt = host.elapsed();
                app.on_frame(host, dt);
                if !app.wants_ticks() {
                    host.rate_hz = 0;
                    host.sync_tick_timer();
                }
            } else if w == TIMER_SECOND {
                app.on_second(host);
                if app.wants_ticks() && host.rate_hz == 0 {
                    host.set_rate(app.preferred_rate());
                    host.sync_tick_timer();
                }
            }
            0
        }
        sys::WM_DPICHANGED => {
            let suggested = unsafe { &*(l as *const sys::WINRECT) };
            let m = host.monitor_under();
            host.scale = m.scale;
            let rect = Rect::new(
                suggested.left as f64,
                suggested.top as f64,
                suggested.right as f64,
                suggested.bottom as f64,
            );
            app.on_system(host, SystemEvent::DpiChanged);
            // The suggested rect is a hint sized for the new DPI; the app has its own idea of the
            // swept box, so `on_system` re-places and we only honour the monitor's scale here.
            let _ = rect;
            0
        }
        sys::WM_DISPLAYCHANGE | sys::WM_SETTINGCHANGE => {
            app.on_system(host, SystemEvent::DisplaysChanged);
            0
        }
        sys::WM_POWERBROADCAST => {
            // 6 = resume on AC, 7 = resume automatically. Only a resume matters: the point is to
            // drop the accumulator and resync, so a machine asleep for nine hours does not wake to a
            // rope that thinks it just missed 777 600 frames.
            if w == 6 || w == 7 {
                app.on_system(host, SystemEvent::Resumed);
            }
            0
        }
        sys::WM_TRAYICON => {
            let (mouse, _id) = tray::tray_event(l);
            if mouse == sys::WM_LBUTTONUP || mouse == sys::WM_LBUTTONDBLCLK_TRAY {
                app.on_command(host, Command::ToggleVisible);
                host.sync_tick_timer();
            } else if mouse == sys::WM_RBUTTONUP {
                let mut pt = sys::POINT { x: 0, y: 0 };
                unsafe { sys::GetCursorPos(&mut pt) };
                let st = app.menu_state(host);
                if let Some(cmd) = host.popup_menu((pt.x, pt.y), st) {
                    app.on_command(host, cmd);
                    host.sync_tick_timer();
                }
            }
            0
        }
        sys::WM_DESTROY => {
            unsafe { sys::PostQuitMessage(host.exit) };
            0
        }
        _ => unsafe { sys::DefWindowProcW(hwnd, msg, w, l) },
    }
}
/// Pointer-shaped messages — hit-testing, the cursor, and the whole drag grammar — resolved
/// against the model's hit box. Returns `None` for every other message, so `wndproc` keeps
/// the non-pointer list.
///
/// # Safety
/// `host`/`app` must be the disjoint fields of the live `Runtime` for `hwnd`, exactly as
/// `wndproc` hands them; the Win32 calls act on that window's state.
unsafe fn on_pointer<A: AppHook + 'static>(
    hwnd: sys::HWND,
    msg: u32,
    w: usize,
    l: isize,
    host: &mut Host,
    app: &mut A,
) -> Option<isize> {
    match msg {
        sys::WM_NCHITTEST => {
            let (x, y) = (l as i16 as i32 as f64, (l >> 16) as i16 as i32 as f64);
            let f = host.frame;
            let local = Vec2::new(x - f.x0, y - f.y0);
            let shape = app.hit_shape();
            // The layered surface already lets clicks fall through fully transparent pixels;
            // answering explicitly is what makes the answer independent of anti-aliased edge
            // coverage, which is never exactly zero — and it is the half of `Solid` mode that is not a
            // window style.
            Some(if shape.whole_window || shape.contains(local) {
                sys::HTCLIENT
            } else {
                sys::HTTRANSPARENT
            })
        }
        sys::WM_SETCURSOR => {
            Host::set_cursor(app.cursor());
            Some(1)
        }
        sys::WM_MOUSEMOVE => {
            let (x, y) = (l as i16 as i32 as f64, (l >> 16) as i16 as i32 as f64);
            let p = Vec2::new(x, y);
            let dt = host.elapsed();
            let mut pt = sys::POINT { x: 0, y: 0 };
            unsafe { sys::GetCursorPos(&mut pt) };
            // Velocity from the *screen* cursor, not from the message pair: with capture on, a
            // pointer dragged off the window keeps producing moves whose deltas are meaningless
            // (wrapped 16-bit coordinates), and the throw would end with a phantom flick.
            let prev = host.last_cursor;
            let vel = if dt > 1e-5 && prev.x != 0 {
                Vec2::new((pt.x - prev.x) as f64 / dt, (pt.y - prev.y) as f64 / dt)
            } else {
                Vec2::ZERO
            };
            host.last_cursor = pt;
            let f = host.frame;
            app.on_input(
                host,
                Input::Move {
                    at: Vec2::new(p.x - f.x0, p.y - f.y0),
                    vel,
                    dt,
                },
            );
            Some(0)
        }
        sys::WM_LBUTTONDOWN => {
            unsafe { sys::SetCapture(hwnd) };
            host.captured = true;
            let (x, y) = (l as i16 as i32 as f64, (l >> 16) as i16 as i32 as f64);
            let f = host.frame;
            // Alt is read here, at the press, and not from a modifier mask on later moves: a re-anchor
            // has to be decided by the button-down that starts the gesture, or pressing Alt halfway
            // through a swing would teleport the hang point to the cursor. `GetKeyState` answers for the
            // calling thread's keyboard state, which is the thread this message arrived on.
            let alt = (unsafe { sys::GetKeyState(sys::VK_MENU) } & sys::KEY_DOWN_MASK) != 0;
            app.on_input(
                host,
                Input::Press {
                    at: Vec2::new(x - f.x0, y - f.y0),
                    alt,
                },
            );
            Some(0)
        }
        sys::WM_LBUTTONUP => {
            unsafe { sys::ReleaseCapture() };
            host.captured = false;
            let (x, y) = (l as i16 as i32 as f64, (l >> 16) as i16 as i32 as f64);
            let f = host.frame;
            app.on_input(
                host,
                Input::Release {
                    at: Vec2::new(x - f.x0, y - f.y0),
                    vel: Vec2::ZERO,
                },
            );
            Some(0)
        }
        sys::WM_RBUTTONUP => {
            let mut pt = sys::POINT { x: 0, y: 0 };
            unsafe { sys::GetCursorPos(&mut pt) };
            let st = app.menu_state(host);
            if let Some(cmd) = host.popup_menu((pt.x, pt.y), st) {
                app.on_command(host, cmd);
                host.sync_tick_timer();
            }
            Some(0)
        }
        sys::WM_MOUSEWHEEL => {
            let delta = ((w >> 16) as u16 as i16) as i32 / 120;
            if delta != 0 {
                app.on_input(host, Input::Wheel { delta });
                host.sync_tick_timer();
            }
            Some(0)
        }
        _ => None,
    }
}
