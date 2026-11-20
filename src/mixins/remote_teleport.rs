fn overwrite_remote_teleport(game: &mut Game<'_>, p: &play::PlayerPosition) -> Result<()> {
    use play::relative as rel;
    let (had_player, current) = game
        .player
        .as_ref()
        .map(|pl| (true, pl.position))
        .unwrap_or((false, Vec3::ZERO));
    let velocity = game
        .player
        .as_ref()
        .map(|pl| pl.velocity)
        .unwrap_or(Vec3::ZERO);
    let pick = |bit: i32, base: f64, v: f64| if p.relatives & bit != 0 { base + v } else { v };
    let position = Vec3::new(
        pick(rel::X, current.x, p.position[0]),
        pick(rel::Y, current.y, p.position[1]),
        pick(rel::Z, current.z, p.position[2]),
    );
    let yaw = if p.relatives & rel::Y_ROT != 0 {
        game.yaw + p.yaw
    } else {
        p.yaw
    };
    let relative_pitch = p.relatives & rel::X_ROT != 0;
    let pitch = position_packet_pitch(game.pitch, p.pitch, relative_pitch);
    let delta = horizon_client::player_sync::teleport_velocity(
        velocity,
        (game.yaw, game.pitch),
        (yaw, pitch),
        p,
    );
    let corrected_position = [position.x, position.y, position.z];
    let corrected_delta = [delta.x, delta.y, delta.z];
    let hit = had_player
        && game.remote.is_some()
        && client_flight::correction::matches(
            p.relatives,
            corrected_position,
            corrected_delta,
            pitch,
        );
    crate::game::remote::fnr_probe::diagnostic(
        log::Level::Info,
        format_args!(
            "recv teleport id={} relatives={} pos={:?} delta={:?} yaw={:?} pitch={:?} matched={hit}",
            p.teleport, p.relatives, p.position, p.delta, p.yaw, p.pitch,
        ),
    );
    if !hit {
        client_flight::correction::clear();
    }
    // BlockStatePredictionHandler.onTeleport: a prior prediction keeps its block, not its remembered player position.
    if let Some(r) = game.remote.as_mut() {
        r.last_teleport_sequence = r.sequence;
    }
    // The server's answer to a passage crossed (`cross_passage`), or any other.
    game.passages.crossing = None;
    // handleMovePlayer: a passenger keeps its own position and look (its vehicle
    // carries it), so a vehicle a datapack teleports every tick doesn't snap the
    // rider's view; the teleport is still accepted, with where the player is.
    if !hit
        && game.riding.is_some()
        && let Some(player) = game.player.as_ref()
    {
        let at = player.position;
        game.send(S::AcceptTeleportation(play::AcceptTeleportation {
            id: p.teleport,
            x: at.x,
            y: at.y,
            z: at.z,
            yaw: game.yaw,
            pitch: game.pitch,
        }));
        game.stop_remote_mining();
        game.mining = None;
        return Ok(());
    }
    let position = if hit { current } else { position };
    match game.player.as_mut() {
        Some(player) => {
            if !hit {
                player.position = position;
                player.velocity = delta;
            }
        }
        None => {
            let mut player = PlayerMotion::new(game.motion, position)?;
            // The id the server gave, for the hooks the motion fires.
            player.entity_id = game.own_entity_id();
            // Player.tick derives noPhysics from the current game mode.
            player.spectator = game.spectator();
            player.velocity = delta;
            game.take_carried_attributes(&mut player)?;
            game.hook.discard();
            game.player_state.attachments.retire();
            game.resident_entities.local_player.renew();
            game.player = Some(player);
            game.bind_local_player_residency();
            game.input_player_replaced();
            game.sway = Default::default();
            game.eye = 1.62;
            game.previous_eye = 1.62;
            game.clock.reset();
            game.apply_remote_health();
            game.apply_remote_abilities();
        }
    }
    entity_section_steps::player_teleport(game, had_player, current, position);
    if !game.replay.playing() {
        game.previous_position = position;
    }
    // A probe hit keeps the player's own look: the server's angles are the ones it remembered,
    // and applying them snaps the view back every time the probe fires.
    if !hit {
        game.yaw = yaw;
        game.pitch = pitch;
    }
    // setValuesFromPositionPacket: setOldPosAndRot gives xRotO the same
    // change, so a relative pitch keeps the model's ease and an absolute
    // one snaps it.
    if !hit
        && let Some(m) = game.player_model.as_mut()
    {
        m.previous_head_pitch =
            position_packet_pitch(m.previous_head_pitch, p.pitch, relative_pitch);
        m.head_pitch = pitch;
    }
    if let Some(id) = game.remote.as_mut().and_then(|r| r.ghast_dismount.take()) {
        game.place_ghast_under(id, position);
    }
    if hit {
        crate::game::remote::fnr_probe::diagnostic(
            log::Level::Info,
            format_args!(
                "ack teleport id={} server_pos={:?} local_pos={:?} ack_yaw={:?} ack_pitch={:?}",
                p.teleport, corrected_position, current, game.yaw, game.pitch,
            ),
        );
    }
    // handleMovePlayer does not change LocalPlayer.sendPosition's baseline:
    // the next tick must still report the corrected state.
    //
    // ServerboundAcceptTeleportationPacket feeds the reported position straight back into
    // ServerGamePacketListenerImpl.handlePlayerPositionChange, so a probe hit has to report the
    // position the server asked for, not the one the player kept. Reporting the airborne
    // position re-triggers the movement check and the server sends another teleport at once.
    let (ack_x, ack_y, ack_z, ack_yaw, ack_pitch) = if hit {
        (
            corrected_position[0],
            corrected_position[1],
            corrected_position[2],
            game.yaw,
            game.pitch,
        )
    } else {
        (position.x, position.y, position.z, yaw, pitch)
    };
    game.send(S::AcceptTeleportation(play::AcceptTeleportation {
        id: p.teleport,
        x: ack_x,
        y: ack_y,
        z: ack_z,
        yaw: ack_yaw,
        pitch: ack_pitch,
    }));
    // Then MultiPlayerGameMode.stopDestroyBlock: a dig the teleport (an
    // anticheat's setback, a /tp) interrupts is aborted on the wire, and the
    // attack strength starts again.
    game.stop_remote_mining();
    game.mining = None;
    if hit
        && let Some(Some(interaction)) = client_flight::correction::receive(
            p.relatives,
            corrected_position,
            corrected_delta,
            pitch,
        )
    {
        interaction();
    }
    Ok(())
}
