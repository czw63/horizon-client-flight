//! No-fall settings and send-position history, independent of game internals.
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static HISTORY: Mutex<History> = Mutex::new(History::new());

/// Enable only after testing against the intended server.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Forget history when no remote player is available.
pub fn reset() {
    *HISTORY.lock().unwrap_or_else(|error| error.into_inner()) = History::new();
}

/// Called on every remote_send_state, including while disabled.
pub fn observe(tick: i32, on_ground: bool, controls_camera: bool, eligible: bool) -> bool {
    HISTORY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .observe(tick, on_ground, controls_camera, eligible, is_enabled())
}

struct History {
    last_tick: Option<i32>,
    was_eligible_airborne: bool,
    was_on_ground: bool,
}

impl History {
    const fn new() -> Self {
        Self {
            last_tick: None,
            was_eligible_airborne: false,
            was_on_ground: false,
        }
    }

    fn observe(
        &mut self,
        tick: i32,
        on_ground: bool,
        controls_camera: bool,
        eligible: bool,
        enabled: bool,
    ) -> bool {
        let consecutive = self
            .last_tick
            .is_some_and(|last| tick == last.wrapping_add(1));
        let landing = enabled
            && consecutive
            && controls_camera
            && self.was_eligible_airborne
            && !self.was_on_ground
            && on_ground
            && eligible;
        self.last_tick = Some(tick);
        self.was_eligible_airborne = controls_camera && !on_ground && eligible;
        self.was_on_ground = on_ground;
        landing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_landing_sends_only_once() {
        let mut history = History::new();
        assert!(!history.observe(10, false, true, true, true));
        assert!(history.observe(11, true, true, true, true));
        assert!(!history.observe(12, true, true, true, true));
        assert!(!history.observe(13, false, true, true, true));
        assert!(history.observe(14, true, true, true, true));
    }

    #[test]
    fn first_call_and_nonconsecutive_ticks_do_not_land() {
        let mut history = History::new();
        assert!(!history.observe(10, true, true, true, true));
        for tick in [10, 11, 9, 100] {
            assert!(!history.observe(tick, false, true, true, true));
            assert!(!history.observe(tick, true, true, true, true));
        }
        assert!(!history.observe(101, false, true, true, true));
        assert!(!history.observe(103, true, true, true, true));
    }

    #[test]
    fn both_airborne_and_landing_must_be_eligible_and_controlled() {
        for (air_camera, air_eligible, land_camera, land_eligible) in [
            (false, true, true, true),
            (true, false, true, true),
            (true, true, false, true),
            (true, true, true, false),
        ] {
            let mut history = History::new();
            assert!(!history.observe(1, false, air_camera, air_eligible, true));
            assert!(!history.observe(2, true, land_camera, land_eligible, true));
            assert!(!history.observe(3, true, true, true, true));
        }
    }

    #[test]
    fn disabled_still_tracks_history_like_java() {
        let mut history = History::new();
        assert!(!history.observe(1, false, true, true, false));
        assert!(!history.observe(2, true, true, true, false));
        assert!(!history.observe(3, true, true, true, true));
        assert!(!history.observe(4, false, true, true, false));
        assert!(history.observe(5, true, true, true, true));
    }

    #[test]
    fn player_tick_wrap_matches_java_integer_wrap() {
        let mut history = History::new();
        assert!(!history.observe(i32::MAX, false, true, true, true));
        assert!(history.observe(i32::MIN, true, true, true, true));
    }

    #[test]
    fn toggle_is_disabled_by_default_and_publicly_settable() {
        assert!(!is_enabled());
        set_enabled(true);
        assert!(is_enabled());
        reset();
        assert!(!observe(1, false, true, true));
        assert!(observe(2, true, true, true));
        reset();
        assert!(!observe(3, true, true, true));
        set_enabled(false);
        assert!(
            crate::commands::run("cfly nofalltoggle")
                .unwrap()
                .contains("ON")
        );
        assert!(is_enabled());
        assert!(
            crate::commands::run("cfly nofalltoggle")
                .unwrap()
                .contains("OFF")
        );
        assert!(!is_enabled());
    }
}
