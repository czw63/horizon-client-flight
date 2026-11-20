//! CollisionProbe state. Geometry selection and input interception are not wired yet.
use std::collections::VecDeque;
use std::sync::Mutex;

pub type Interaction = Box<dyn FnOnce() + Send + 'static>;

static PROBE: Mutex<Probe> = Mutex::new(Probe::new());
const HISTORY_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SentMove {
    pub tick: Option<i32>,
    pub position: Option<[f64; 3]>,
    pub rotation: Option<[f32; 2]>,
    pub flags: u8,
}

/// `client_flying` must be Flight.isClientFlying's full eligibility predicate.
#[derive(Clone, Copy)]
pub struct Player {
    pub tick: i32,
    pub position: [f64; 3],
    pub pitch: f32,
    pub client_flying: bool,
}

/// A future caller sends PosRot(origin, marker_pitch), TickEnd, Pos(target), TickEnd.
#[derive(Clone, Copy, Debug)]
pub struct ProbeMove {
    pub origin: [f64; 3],
    pub marker_pitch: f32,
    pub target: [f64; 3],
}

/// No target is invented when the caller cannot perform Java's collision queries.
pub fn tick(player: Option<Player>, collision_target: Option<[f64; 3]>) -> Option<ProbeMove> {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .tick(player, collision_target)
}

pub fn is_awaiting_correction() -> bool {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .expected_position
        .is_some()
}

pub fn defer_interaction(interaction: Interaction) -> bool {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .defer_interaction(interaction)
}

pub fn clear() {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

pub fn finish() {
    clear();
}

pub fn matches(relatives: i32, position: [f64; 3], delta: [f64; 3], pitch: f32) -> bool {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .matches(relatives, position, delta, pitch)
}

/// None: vanilla handles it. Some(None): consume it without a deferred interaction.
/// The callback is removed from the mutex before the helper sends the ACK or runs it.
pub fn receive(
    relatives: i32,
    position: [f64; 3],
    delta: [f64; 3],
    pitch: f32,
) -> Option<Option<Interaction>> {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .receive(relatives, position, delta, pitch)
}

/// Called only at session.send, after the packet-cancellation gate.
/// History is diagnostic, NOT permission to suppress a server teleport.
pub fn record_sent_move(packet: SentMove) {
    let mut probe = PROBE.lock().unwrap_or_else(|error| error.into_inner());
    if probe.sent_moves.len() == HISTORY_LIMIT {
        probe.sent_moves.pop_front();
    }
    probe.sent_moves.push_back(packet);
}

pub fn sent_moves() -> Vec<SentMove> {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .sent_moves
        .iter()
        .copied()
        .collect()
}

struct Probe {
    expected_position: Option<[f64; 3]>,
    expected_pitch: f32,
    expected_tick: i32,
    sequence: i32,
    pending_interaction: Option<Interaction>,
    last_probe_tick: i32,
    sent_moves: VecDeque<SentMove>,
}

impl Probe {
    const fn new() -> Self {
        Self {
            expected_position: None,
            expected_pitch: 0.0,
            expected_tick: -100,
            sequence: 0,
            pending_interaction: None,
            last_probe_tick: -100,
            sent_moves: VecDeque::new(),
        }
    }

    fn clear(&mut self) {
        self.expected_position = None;
        self.expected_tick = -100;
        self.pending_interaction = None;
    }

    fn tick(&mut self, player: Option<Player>, target: Option<[f64; 3]>) -> Option<ProbeMove> {
        let Some(player) = player.filter(|p| p.client_flying) else {
            self.clear();
            self.last_probe_tick = -100;
            return None;
        };
        if self.expected_position.is_some() {
            if player.tick.wrapping_sub(self.expected_tick) > 40 {
                self.clear();
            }
            return None;
        }
        if player.tick.wrapping_sub(self.expected_tick) < 2
            || player.tick.wrapping_sub(self.last_probe_tick) < 2
        {
            return None;
        }
        let target = target?;
        self.expected_position = Some(player.position);
        self.expected_tick = player.tick;
        self.sequence = self.sequence.wrapping_add(1);
        let pitch = player.pitch;
        let mut marker = if pitch == 90.0 {
            pitch.next_down()
        } else {
            pitch.next_up()
        };
        for _ in 0..((self.sequence & 63) + 1) {
            marker = if pitch >= 89.0 {
                marker.next_down()
            } else {
                marker.next_up()
            };
        }
        self.expected_pitch = marker;
        self.last_probe_tick = player.tick;
        Some(ProbeMove {
            origin: player.position,
            marker_pitch: marker,
            target,
        })
    }

    fn matches(&self, relatives: i32, position: [f64; 3], delta: [f64; 3], pitch: f32) -> bool {
        let Some(expected) = self.expected_position else {
            return false;
        };
        // Java requires an EMPTY relative set, including rotation/velocity bits.
        // Inspect raw packet delta, not teleport_velocity's resolved velocity.
        let distance: [f64; 3] = std::array::from_fn(|i| position[i] - expected[i]);
        relatives == 0
            && delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2] == 0.0
            && distance[0] * distance[0] + distance[1] * distance[1] + distance[2] * distance[2]
                < 1.0e-8
            && java_float_bits(pitch) == java_float_bits(self.expected_pitch)
    }

    fn defer_interaction(&mut self, interaction: Interaction) -> bool {
        if self.expected_position.is_none() {
            return false;
        }
        self.pending_interaction = Some(interaction);
        true
    }

    fn receive(
        &mut self,
        relatives: i32,
        position: [f64; 3],
        delta: [f64; 3],
        pitch: f32,
    ) -> Option<Option<Interaction>> {
        let matches = self.matches(relatives, position, delta, pitch);
        let interaction = self.pending_interaction.take();
        self.clear();
        matches.then_some(interaction)
    }
}

fn java_float_bits(value: f32) -> u32 {
    if value.is_nan() {
        0x7fc0_0000
    } else {
        value.to_bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn player(tick: i32) -> Player {
        Player {
            tick,
            position: [0.0, 64.0, 0.0],
            pitch: 30.0,
            client_flying: true,
        }
    }

    fn arm(probe: &mut Probe, tick: i32) -> ProbeMove {
        probe
            .tick(Some(player(tick)), Some([0.0, 63.9375, 0.0]))
            .unwrap()
    }

    #[test]
    fn strict_java_match_uses_raw_delta_and_all_relative_bits() {
        let mut probe = Probe::new();
        let request = arm(&mut probe, 1);
        assert!(probe.matches(0, request.origin, [0.0; 3], request.marker_pitch));
        for bit in 0..9 {
            assert!(!probe.matches(1 << bit, request.origin, [0.0; 3], request.marker_pitch));
        }
        assert!(!probe.matches(0, request.origin, [0.0, 0.01, 0.0], request.marker_pitch));
        assert!(!probe.matches(0, [0.001, 64.0, 0.0], [0.0; 3], request.marker_pitch));
        assert!(probe.matches(0, [0.00001, 64.0, 0.0], [0.0; 3], request.marker_pitch));
        assert!(!probe.matches(0, request.origin, [0.0; 3], 30.0));
    }

    #[test]
    fn matching_receive_clears_and_coalesces_interactions() {
        let mut probe = Probe::new();
        let counter = Arc::new(AtomicUsize::new(0));
        assert!(!probe.defer_interaction(Box::new(|| panic!("not waiting"))));
        let request = arm(&mut probe, 1);
        assert!(probe.defer_interaction(Box::new(|| panic!("replaced"))));
        let count = counter.clone();
        assert!(probe.defer_interaction(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })));
        let interaction = probe
            .receive(0, request.origin, [0.0; 3], request.marker_pitch)
            .unwrap()
            .unwrap();
        assert!(probe.expected_position.is_none());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        interaction();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(
            probe
                .receive(0, request.origin, [0.0; 3], request.marker_pitch)
                .is_none()
        );
    }

    #[test]
    fn unrelated_teleport_discards_pending_input() {
        let mut probe = Probe::new();
        let request = arm(&mut probe, 1);
        probe.defer_interaction(Box::new(|| panic!("must not execute")));
        assert!(probe.receive(0, request.origin, [0.0; 3], 30.0).is_none());
        assert!(probe.expected_position.is_none());
        assert!(probe.pending_interaction.is_none());
    }

    #[test]
    fn timeout_and_mc263_two_tick_throttle() {
        let mut probe = Probe::new();
        arm(&mut probe, 1);
        probe.clear();
        assert!(probe.tick(Some(player(2)), Some([0.0; 3])).is_none());
        arm(&mut probe, 3);
        assert!(probe.tick(Some(player(43)), None).is_none());
        assert!(probe.expected_position.is_some());
        assert!(probe.tick(Some(player(44)), None).is_none());
        assert!(probe.expected_position.is_none());
        arm(&mut probe, 45);
        probe.tick(None, None);
        assert!(probe.expected_position.is_none());
        assert_eq!(probe.last_probe_tick, -100);
    }

    #[test]
    fn no_geometry_does_not_arm_or_invent_a_target() {
        let mut probe = Probe::new();
        assert!(probe.tick(Some(player(1)), None).is_none());
        assert!(probe.expected_position.is_none());
        let request = arm(&mut probe, 2);
        assert_eq!(request.marker_pitch.to_bits(), 30.0_f32.to_bits() + 3);
    }

    #[test]
    fn pitch_marker_matches_java_at_vertical_limits_and_wrap() {
        for pitch in [-90.0, 0.0, 89.0, 90.0] {
            let mut probe = Probe::new();
            let request = probe
                .tick(Some(Player { pitch, ..player(1) }), Some([0.0; 3]))
                .unwrap();
            let expected = match pitch {
                90.0 => pitch.to_bits() - 3,
                89.0 => pitch.to_bits() - 1,
                -90.0 => pitch.to_bits() - 3,
                _ => 3,
            };
            assert_eq!(request.marker_pitch.to_bits(), expected);
        }
        let mut probe = Probe::new();
        probe.sequence = i32::MAX;
        probe.expected_tick = i32::MAX - 2;
        probe.last_probe_tick = i32::MAX - 2;
        let request = arm(&mut probe, i32::MAX);
        assert_eq!(probe.sequence, i32::MIN);
        assert_eq!(request.marker_pitch.to_bits(), 30.0_f32.to_bits() + 2);
        assert!(probe.tick(Some(player(i32::MIN)), Some([0.0; 3])).is_none());
        probe.tick(Some(player(i32::MIN + 39)), None);
        assert!(probe.expected_position.is_some());
        probe.tick(Some(player(i32::MIN + 40)), None);
        assert!(probe.expected_position.is_none());
    }

    #[test]
    fn sent_packet_history_is_bounded_and_does_not_arm_correction() {
        clear();
        for i in 0..130 {
            record_sent_move(SentMove {
                tick: Some(i),
                position: Some([0.0, i as f64, 0.0]),
                rotation: None,
                flags: 0,
            });
        }
        let history = sent_moves();
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(history.first().unwrap().tick, Some(2));
        assert_eq!(history.last().unwrap().position, Some([0.0, 129.0, 0.0]));
        assert!(!is_awaiting_correction());
    }
}
