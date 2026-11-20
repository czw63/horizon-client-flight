//! Compiled inside game::remote's replacement mirror, never into the mod API crate.
use super::*;
use client_flight::{correction, probe_geometry};
use horizon_client::collisions::{EntityCollisions, SceneCollisions, no_collision_with_context};
use horizon_core::geometry::Vec3;
use horizon_core::position::BlockPos;
use horizon_world::collision_context::{
    CollisionContext, CollisionEntity, CollisionItem, EntityContextKind,
};
use horizon_world::entity_obstruction::EntityIdentity;
use std::cell::RefCell;

thread_local! {
    // The callback owns the pending snapshot. Only a matching correction moves it here,
    // after the existing correction mixin has sent AcceptTeleportation.
    static READY: RefCell<Option<DeferredUse>> = const { RefCell::new(None) };
    static FLIGHT_OWNER: RefCell<Option<mod_api::PlayerRef>> = const { RefCell::new(None) };
    static PROBE_PLAYER: RefCell<Option<mod_api::PlayerRef>> = const { RefCell::new(None) };
}

struct DeferredUse {
    player: mod_api::PlayerRef,
    main: Option<horizon_client::items::ItemStack>,
    off: Option<horizon_client::items::ItemStack>,
    pos: [i32; 3],
    face: [i32; 3],
    attempt: mod_api::InteractionAttempt,
}

fn vec(at: [f64; 3]) -> Vec3 {
    Vec3::new(at[0], at[1], at[2])
}

fn diagnostic(level: log::Level, message: std::fmt::Arguments<'_>) {
    if log::log_enabled!(target: "client_flight", level) {
        log::log!(target: "client_flight", level, "[client_flight] {message}");
    } else {
        eprintln!("[client_flight] {level} {message}");
    }
}

/// Validate the entire batch before sending even its first packet or TickEnd.
pub fn finite_movement_batch(group: &str, tick: i32, fields: &[(&str, f64)]) -> bool {
    let mut valid = true;
    for &(field, value) in fields {
        if !value.is_finite() {
            valid = false;
            diagnostic(
                log::Level::Warn,
                format_args!(
                    "drop movement batch group={group} tick={tick} field={field} value={value:?}"
                ),
            );
        }
    }
    if !valid {
        correction::clear();
    }
    valid
}

fn collision_target(game: &Game<'_>) -> Option<[f64; 3]> {
    let player = game.player.as_ref()?;
    let feet = player.position;
    let bounds = player.bounds();
    let entities = game.player_entity_collisions();
    let main = game.inventory[game.selected]
        .as_ref()
        .map_or(CollisionItem::EMPTY, |s| CollisionItem {
            item: &s.item,
            count: s.count as i32,
        });
    let context = CollisionContext::for_entity(
        CollisionEntity {
            identity: EntityIdentity(player.entity_id as u64),
            kind: EntityContextKind::Player,
            y: feet.y,
            descending: player.shift_key_down,
            main_hand: Some(main),
        },
        false,
    );
    let subject = horizon_client::collisions::PowderSnowSubject {
        main_hand: Some(main),
        ..player.powder_snow_subject()
    };
    probe_geometry::find_collision_target(
        feet.components(),
        f64::from(player.dimensions.height),
        f64::from(player.dimensions.width) / 2.0,
        |start, end| {
            probe_geometry::ray(start, end, |cell, a, b| {
                let pos = BlockPos::new(cell[0], cell[1], cell[2]);
                let mut scene = SceneCollisions {
                    world: &game.world,
                    binding: game.collision,
                    subject: Some(subject),
                };
                let shape = game
                    .collision
                    .collision_shape_with_context(
                        &mut scene,
                        game.world.block(cell[0], cell[1], cell[2]),
                        pos,
                        &context,
                    )
                    .ok()?;
                let hit = shape.clip(vec(a), vec(b), pos)?;
                Some(probe_geometry::Hit {
                    position: hit.location.components(),
                    normal: hit.direction.normal(),
                })
            })
        },
        |target| {
            let moved = bounds.move_by(vec(target).subtract(feet));
            let mut world = EntityCollisions {
                blocks: SceneCollisions {
                    world: &game.world,
                    binding: game.collision,
                    subject: Some(subject),
                },
                entities: &entities,
                border: player.border.as_ref().map(|b| (b, [feet.x, feet.z])),
            };
            !no_collision_with_context(&mut world, moved, context)
        },
    )
}

pub fn tick(game: &mut Game<'_>) {
    check_player(game);
    let identity = game.local_player();
    let enabled = client_flight::commands::flight_enabled();
    let server_may_fly = game
        .remote
        .as_ref()
        .and_then(|r| r.abilities.as_ref())
        .is_some_and(|a| a.flags & 4 != 0);
    let client_owned = FLIGHT_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if identity.is_none() || *owner != identity {
            *owner = None;
        }
        if let Some(player) = game.player.as_mut().filter(|_| game.remote.is_some()) {
            if enabled {
                player.may_fly = true;
                *owner = if server_may_fly { None } else { identity };
            } else if owner.is_some() {
                // Revoke only our grant, never creative/server flight permission.
                player.may_fly = server_may_fly;
                if !server_may_fly {
                    player.flying = false;
                }
                *owner = None;
            }
        }
        enabled && identity.is_some() && *owner == identity
    });
    let player = game.player.as_ref().map(|p| correction::Player {
        tick: p.health.tick_count,
        position: p.position.components(),
        pitch: game.pitch,
        // Player.onClimbable() is false while ability-flying, regardless of the cached flag.
        client_flying: client_owned
            && p.may_fly
            && p.flying
            && !game.creative
            && !game.spectator()
            && !p.ground.on_ground
            && !p.is_passenger
            && game.riding.is_none()
            && !p.fall_flying
            && !p.fluids.was_touching_water
            && !p.is_in_lava(),
    });
    let Some(request) = correction::tick_with_target(player, || {
        let target = collision_target(game);
        if target.is_none() {
            diagnostic(
                log::Level::Debug,
                format_args!(
                    "probe no collision target tick={:?}",
                    player.map(|p| p.tick)
                ),
            );
        }
        target
    }) else {
        return;
    };
    if !finite_movement_batch(
        "probe",
        request.tick,
        &[
            ("origin.x", request.origin[0]),
            ("origin.y", request.origin[1]),
            ("origin.z", request.origin[2]),
            ("target.x", request.target[0]),
            ("target.y", request.target[1]),
            ("target.z", request.target[2]),
            ("marker_pitch", f64::from(request.marker_pitch)),
            ("game.yaw", f64::from(game.yaw)),
        ],
    ) {
        return;
    }
    PROBE_PLAYER.with(|owner| *owner.borrow_mut() = identity);
    let flags = game.player.as_ref().map_or(0, |p| {
        u8::from(p.ground.on_ground) | (u8::from(p.collision.horizontal) << 1)
    });
    diagnostic(
        log::Level::Info,
        format_args!(
            "probe send origin={:?} target={:?} marker_pitch={:?} sequence={} tick={} yaw={:?} flags={flags:#04x}",
            request.origin,
            request.target,
            request.marker_pitch,
            request.sequence,
            request.tick,
            game.yaw,
        ),
    );
    let [x, y, z] = request.origin;
    game.send(S::MovePlayerPosRot(play::MovePlayerPosRot {
        x,
        y,
        z,
        yaw: game.yaw,
        pitch: request.marker_pitch,
        flags,
    }));
    game.send(S::ClientTickEnd(horizon_protocol::packets::Empty));
    let [x, y, z] = request.target;
    game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }));
    game.send(S::ClientTickEnd(horizon_protocol::packets::Empty));
}

pub fn check_player(game: &Game<'_>) {
    if correction::is_awaiting_correction()
        && (!client_flight::commands::flight_enabled()
            || PROBE_PLAYER.with(|owner| *owner.borrow() != game.local_player()))
    {
        correction::clear();
    }
}

pub fn defer_use(game: &Game<'_>, attempt: mod_api::InteractionAttempt) -> bool {
    if !client_flight::commands::flight_enabled()
        || game.player.is_none()
        || game.remote.as_ref().is_none_or(|r| r.is_destroying)
        || game.hands_busy()
        || game.entity_targeted()
        || !correction::is_awaiting_correction()
    {
        return false;
    }
    let (Some(player), Some(hit)) = (game.local_player(), game.pick()) else {
        return false;
    };
    let action = DeferredUse {
        player,
        main: game.inventory[game.selected].clone(),
        off: game.offhand.clone(),
        pos: hit.pos,
        face: hit.face,
        attempt,
    };
    correction::defer_interaction(Box::new(move || {
        READY.with(|ready| *ready.borrow_mut() = Some(action));
    }))
}

fn same_stack(
    expected: Option<&horizon_client::items::ItemStack>,
    actual: Option<&horizon_client::items::ItemStack>,
) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(a), Some(b)) => a.matches(b),
        _ => false,
    }
}

pub fn replay_use(game: &mut Game<'_>) {
    let Some(action) = READY.with(|ready| ready.borrow_mut().take()) else {
        return;
    };
    if game.local_player() != Some(action.player)
        || game
            .player
            .as_ref()
            .is_none_or(|p| p.health.health() <= 0.0)
        || game.is_paused()
        || game.screen != Screen::Playing
        || game.observed_screen_view().is_some()
        || game.overlay_holds_view()
        || !same_stack(action.main.as_ref(), game.inventory[game.selected].as_ref())
        || !same_stack(action.off.as_ref(), game.offhand.as_ref())
        || game.entity_targeted()
        || !game
            .pick()
            .is_some_and(|hit| hit.pos == action.pos && hit.face == action.face)
    {
        return;
    }
    game.remote_use(action.attempt);
}
