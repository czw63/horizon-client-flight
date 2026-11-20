// Standalone harness for the actual helper, without a generated Cargo SDK.
use std::ops::ControlFlow;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

struct Hurt {
    hurt_time: i32,
}

struct Health {
    hurt: Hurt,
}

struct Player {
    may_fly: bool,
    flying: bool,
    health: Health,
    velocity: Vec3,
}

struct Remote {
    entity_id: i32,
}

struct Motion {
    id: i32,
    velocity: [f64; 3],
}

enum C {
    SetEntityMotion(Motion),
    Other,
}

struct Game<'a> {
    player: Option<Player>,
    remote: Option<Remote>,
    observations: usize,
    on_observe: Option<fn(&mut Game<'_>)>,
    other_motion: Option<(i32, [f64; 3])>,
    _lifetime: &'a (),
}

impl Game<'_> {
    fn observe_client_packet(&mut self, _: &C) {
        self.observations += 1;
        if let Some(callback) = self.on_observe {
            callback(self);
        }
    }

    fn remote_motion(&mut self, id: i32, velocity: [f64; 3]) {
        self.other_motion = Some((id, velocity));
    }
}

include!("mixins/flight_velocity.rs");

fn game() -> Game<'static> {
    Game {
        player: Some(Player {
            may_fly: true,
            flying: true,
            health: Health {
                hurt: Hurt { hurt_time: 10 },
            },
            velocity: Vec3::new(0.25, 0.125, -0.5),
        }),
        remote: Some(Remote { entity_id: 7 }),
        observations: 0,
        on_observe: None,
        other_motion: None,
        _lifetime: &(),
    }
}

fn packet(id: i32, y: f64) -> C {
    C::SetEntityMotion(Motion {
        id,
        velocity: [1.25, y, -2.5],
    })
}

#[test]
fn only_active_hurt_flight_with_negative_y_is_intercepted() {
    for may_fly in [false, true] {
        for flying in [false, true] {
            for hurt_time in [-1, 0, 1, 10] {
                for y in [
                    -f64::INFINITY,
                    -0.8,
                    -0.0,
                    0.0,
                    0.6,
                    f64::INFINITY,
                    f64::NAN,
                ] {
                    let mut game = game();
                    let player = game.player.as_mut().unwrap();
                    player.may_fly = may_fly;
                    player.flying = flying;
                    player.health.hurt.hurt_time = hurt_time;
                    let before = player.velocity;
                    let packet = packet(7, y);
                    let result = keep_flight_velocity(&mut game, &packet);
                    let eligible = may_fly && flying && hurt_time > 0 && y < 0.0;
                    assert_eq!(result.is_break(), eligible);
                    assert_eq!(game.observations, usize::from(eligible));
                    assert_eq!(
                        game.player.as_ref().unwrap().velocity,
                        if eligible {
                            Vec3::new(1.25, before.y, -2.5)
                        } else {
                            before
                        }
                    );
                    let C::SetEntityMotion(p) = packet else {
                        unreachable!()
                    };
                    assert_eq!(p.velocity[1].to_bits(), y.to_bits());
                    assert_eq!(p.velocity[0], 1.25);
                    assert_eq!(p.velocity[2], -2.5);
                }
            }
        }
    }
}

#[test]
fn unrelated_packets_and_missing_player_or_connection_continue_untouched() {
    for case in 0..4 {
        let mut game = game();
        let packet = match case {
            0 => C::Other,
            1 => packet(8, -0.8),
            2 => {
                game.player = None;
                packet(7, -0.8)
            }
            _ => {
                game.remote = None;
                packet(7, -0.8)
            }
        };
        assert!(keep_flight_velocity(&mut game, &packet).is_continue());
        assert_eq!(game.observations, 0);
        assert!(game.other_motion.is_none());
        if let Some(player) = game.player {
            assert_eq!(player.velocity, Vec3::new(0.25, 0.125, -0.5));
        }
    }
}

#[test]
fn preserves_current_y_including_descent_and_server_xz() {
    for current_y in [-0.3, -0.0, 0.0, 0.7] {
        let mut game = game();
        game.player.as_mut().unwrap().velocity.y = current_y;
        assert!(matches!(
            keep_flight_velocity(&mut game, &packet(7, -0.8)),
            ControlFlow::Break(Ok(()))
        ));
        let velocity = game.player.unwrap().velocity;
        assert_eq!(velocity.y.to_bits(), current_y.to_bits());
        assert_eq!((velocity.x, velocity.z), (1.25, -2.5));
        assert_eq!(game.observations, 1);
    }
}

#[test]
fn packet_observers_run_once_before_assignment_and_can_change_state() {
    let mut game = game();
    game.on_observe = Some(|game| {
        let player = game.player.as_mut().unwrap();
        assert_eq!(player.velocity.y, 0.125);
        player.flying = false;
    });
    assert!(keep_flight_velocity(&mut game, &packet(7, -0.8)).is_break());
    assert_eq!(game.player.unwrap().velocity, Vec3::new(1.25, -0.8, -2.5));
    assert_eq!(game.observations, 1);
}

#[test]
fn observer_identity_change_uses_original_remote_entity_path() {
    let mut game = game();
    game.on_observe = Some(|game| game.remote = None);
    assert!(keep_flight_velocity(&mut game, &packet(7, -0.8)).is_break());
    assert_eq!(game.other_motion, Some((7, [1.25, -0.8, -2.5])));
    assert_eq!(game.player.unwrap().velocity, Vec3::new(0.25, 0.125, -0.5));
    assert_eq!(game.observations, 1);
}
