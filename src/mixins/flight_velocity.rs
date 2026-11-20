fn keep_flight_velocity(game: &mut Game<'_>, packet: &C) -> ::core::ops::ControlFlow<Result<()>> {
    if !client_flight::commands::flight_enabled() {
        return ::core::ops::ControlFlow::Continue(());
    }
    let C::SetEntityMotion(p) = packet else {
        return ::core::ops::ControlFlow::Continue(());
    };
    if !game.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
        return ::core::ops::ControlFlow::Continue(());
    }
    // may_fly alone is permission, not active flight.
    if !game.player.as_ref().is_some_and(|player| {
        player.may_fly && player.flying && player.health.hurt.hurt_time > 0 && p.velocity[1] < 0.0
    }) {
        return ::core::ops::ControlFlow::Continue(());
    }
    // Preserve apply_packet's observation before replacing just this branch.
    game.observe_client_packet(packet);
    if game.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
        if let Some(player) = game.player.as_mut() {
            let y = if player.may_fly && player.flying && player.health.hurt.hurt_time > 0 {
                player.velocity.y
            } else {
                p.velocity[1]
            };
            player.velocity = Vec3::new(p.velocity[0], y, p.velocity[2]);
        }
    } else {
        game.remote_motion(p.id, p.velocity);
    }
    ::core::ops::ControlFlow::Break(Ok(()))
}
