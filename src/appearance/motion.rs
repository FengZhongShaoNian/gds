//! One application motion preference combined with the desktop's reduced-motion setting.
use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Duration;

use zgui::reactive::{LocalStorage, RenderEffect, RwSignal, Signal, on_cleanup_local, prelude::*};
use zgui::view::time::{FrameHandle, Timers};
use zgui::vocab::Timestamp;

#[cfg(test)]
mod tests;

pub const INLINE_DURATION_MS: u64 = 140;
pub const WORKSPACE_DURATION_MS: u64 = 160;
pub const WORKSPACE_EASING: &str = "cubic-bezier(0, 0, 0.2, 1)";
const SETTLE_EPSILON: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub duration_ms: u64,
    pub easing: &'static str,
}

impl Transition {
    pub const fn ms(duration_ms: u64, easing: &'static str) -> Self {
        Self {
            duration_ms,
            easing,
        }
    }

    pub fn css_value(self, reduced: bool) -> String {
        if reduced {
            "0ms".to_owned()
        } else {
            format!("{}ms {}", self.duration_ms, self.easing)
        }
    }
}

pub const INLINE: Transition = Transition::ms(INLINE_DURATION_MS, WORKSPACE_EASING);
pub const WORKSPACE: Transition = Transition::ms(WORKSPACE_DURATION_MS, WORKSPACE_EASING);

/// Standard GDS `ease-out cubic` curve without spring overshoot (`1 - (1 - t)^3`).
#[must_use]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub enabled: bool,
    pub system_reduced: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: true,
            system_reduced: false,
        }
    }
}

impl Policy {
    pub fn reduced(self) -> bool {
        !self.enabled || self.system_reduced
    }

    pub(crate) fn write_css_declarations(self, out: &mut String) {
        let reduced = self.reduced();
        let (fast, inline, normal, slow) = if reduced {
            (0, 0, 0, 0)
        } else {
            (100, INLINE_DURATION_MS, WORKSPACE_DURATION_MS, 240)
        };
        let _ = writeln!(
            out,
            "    --gds-motion-reduced: {};",
            if reduced { 1 } else { 0 }
        );
        let _ = writeln!(out, "    --gds-motion-none: 0ms;");
        let _ = writeln!(out, "    --gds-motion-fast: {fast}ms;");
        let _ = writeln!(out, "    --gds-motion-inline: {inline}ms;");
        let _ = writeln!(out, "    --gds-motion-normal: {normal}ms;");
        let _ = writeln!(out, "    --gds-motion-slow: {slow}ms;");
        if reduced {
            let _ = writeln!(out, "    --zui-motion-duration-fast: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-normal: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-slow: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-slower: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-slowest: 0ms;");
        }
        let _ = writeln!(
            out,
            "    --gds-motion-ease-standard: cubic-bezier(0.4, 0, 0.2, 1);"
        );
        let _ = writeln!(out, "    --gds-motion-ease-out: {WORKSPACE_EASING};");
        let _ = writeln!(out, "    --gds-motion-ease-in: cubic-bezier(0.4, 0, 1, 1);");
        let _ = writeln!(out, "    --gds-motion-ease-linear: linear;");
    }
}

pub fn enabled() -> bool {
    crate::app::Gds::current().appearance().motion().enabled()
}

struct Flight {
    from: f32,
    to: f32,
    start: Option<Timestamp>,
    last_frame: Option<Timestamp>,
    pending: Option<FrameHandle>,
    in_flight: bool,
    generation: u64,
}

impl Flight {
    fn stop(&mut self) -> Option<FrameHandle> {
        self.in_flight = false;
        self.start = None;
        self.last_frame = None;
        self.pending.take()
    }
}

fn arm_frame(
    clock: &Timers,
    state: &Rc<RefCell<Flight>>,
    current: RwSignal<f32, LocalStorage>,
    duration: Duration,
    generation: u64,
) {
    enum Step {
        Ignore,
        Continue(f32),
        Settle(f32),
    }

    let clock_next = clock.clone();
    let state_cb = Rc::clone(state);
    let handle = clock.request_frame(move |at| {
        let step = {
            let mut st = state_cb.borrow_mut();
            if !st.in_flight || st.generation != generation {
                Step::Ignore
            } else {
                st.pending = None;
                st.last_frame = Some(at);
                let origin = *st.start.get_or_insert(at);
                let elapsed = at.saturating_since(origin);
                if elapsed >= duration || duration.is_zero() {
                    let target_value = st.to;
                    st.in_flight = false;
                    st.start = None;
                    st.last_frame = None;
                    Step::Settle(target_value)
                } else {
                    let raw_t = (elapsed.as_secs_f64() / duration.as_secs_f64()) as f32;
                    let next_value = st.from + (st.to - st.from) * ease_out_cubic(raw_t);
                    Step::Continue(next_value)
                }
            }
        };
        match step {
            Step::Ignore => {}
            Step::Settle(val) => {
                if current.get_untracked() != val {
                    current.set(val);
                }
            }
            Step::Continue(val) => {
                if (current.get_untracked() - val).abs() > 1e-6 {
                    current.set(val);
                }
                arm_frame(&clock_next, &state_cb, current, duration, generation);
            }
        }
    });

    let mut st = state.borrow_mut();
    if st.in_flight && st.generation == generation {
        st.pending = Some(handle);
    }
}

/// Frame-driven `ease-out cubic` target interpolator (`140ms` inline / `160ms` workspace).
///
/// Retargets smoothly from the currently displayed value when reversed mid-flight, snaps
/// immediately when reduced motion is active or no window frame clock is mounted, and schedules
/// zero frame callbacks once settled.
#[must_use]
pub fn use_transition(
    target: Signal<f32, LocalStorage>,
    duration_ms: u64,
) -> Signal<f32, LocalStorage> {
    let initial = target.get_untracked();
    let current = RwSignal::new_local(initial);
    let state = Rc::new(RefCell::new(Flight {
        from: initial,
        to: initial,
        start: None,
        last_frame: None,
        pending: None,
        in_flight: false,
        generation: 0,
    }));

    let state_fx = Rc::clone(&state);
    let effect = RenderEffect::new(move |_| {
        let goal = target.get();
        let reduced =
            crate::app::Gds::try_current().is_some_and(|gds| gds.appearance().motion().reduced());
        let displayed = current.get_untracked();

        if reduced || duration_ms == 0 || !goal.is_finite() || !displayed.is_finite() {
            let cancelled = state_fx.borrow_mut().stop();
            drop(cancelled);
            if goal.is_finite() && displayed != goal {
                current.set(goal);
            }
            return;
        }

        if (goal - displayed).abs() <= SETTLE_EPSILON {
            let cancelled = state_fx.borrow_mut().stop();
            drop(cancelled);
            if displayed != goal {
                current.set(goal);
            }
            return;
        }

        let already_heading_there = {
            let st = state_fx.borrow();
            st.in_flight && (st.to - goal).abs() <= SETTLE_EPSILON
        };
        if already_heading_there {
            return;
        }

        let Some(clock) = Timers::current() else {
            let cancelled = state_fx.borrow_mut().stop();
            drop(cancelled);
            if displayed != goal {
                current.set(goal);
            }
            return;
        };

        let (cancelled, generation) = {
            let mut st = state_fx.borrow_mut();
            let prev = st.pending.take();
            st.generation = st.generation.wrapping_add(1);
            let generation = st.generation;
            st.from = displayed;
            st.to = goal;
            st.start = if st.in_flight { st.last_frame } else { None };
            st.in_flight = true;
            (prev, generation)
        };
        drop(cancelled);
        arm_frame(
            &clock,
            &state_fx,
            current,
            Duration::from_millis(duration_ms),
            generation,
        );
    });

    on_cleanup_local(move || {
        let cancelled = state.borrow_mut().stop();
        drop(cancelled);
        drop(effect);
    });

    current.into()
}
