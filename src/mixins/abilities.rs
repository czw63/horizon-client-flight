fn apply_remote_abilities_keep_flight(game: &mut Game<'_>) {
    let Some(player) = game.player.as_mut() else {
        return;
    };
    let Some(a) = game.remote.as_ref().and_then(|r| r.abilities.as_ref()) else {
        return;
    };
    player.invulnerable = a.flags & 1 != 0;
    player.flying = a.flags & 2 != 0;
    player.may_fly = a.flags & 4 != 0 || client_flight::commands::flight_enabled();
    player.flying_speed = a.flying_speed;
}
