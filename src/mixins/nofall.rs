fn prepare_safe_landing(game: &mut Game<'_>) {
    let Some(p) = game.player.as_ref().filter(|_| game.remote.is_some()) else {
        client_flight::nofall::reset();
        return;
    };
    // Query the landing position, not the cached pre-move climbing flag.
    let climbing = !p.flying
        && game
            .local_player()
            .and_then(|player| game.living(player.into()))
            .and_then(|living| game.living_is_climbing(living))
            .unwrap_or(true);
    let eligible = game.riding.is_none()
        && !p.fall_flying
        && !p.fluids.was_touching_water
        && !p.is_in_lava()
        && !p.swimming
        && !climbing;
    let on_ground = p.ground.on_ground;
    if !client_flight::nofall::observe(
        p.health.tick_count,
        on_ground,
        game.controls_camera(),
        eligible,
    ) {
        return;
    }
    let (x, y, z) = (p.position.x, p.position.y, p.position.z);
    let collision = u8::from(p.collision.horizontal) << 1;
    for (y, on_ground) in [(y, false), (y + 0.01, false), (y, true)] {
        game.send(S::MovePlayerPos(play::MovePlayerPos {
            x,
            y,
            z,
            flags: u8::from(on_ground) | collision,
        }));
        // The original mod's MC 26.3 branch ends each synthetic move's tick.
        game.send(S::ClientTickEnd(Empty));
    }
}
