fn replay_probe_interaction(
    game: &mut Game<'_>,
    packet: &C,
) -> ::core::ops::ControlFlow<Result<()>> {
    let C::PlayerPosition(p) = packet else {
        return ::core::ops::ControlFlow::Continue(());
    };
    game.observe_client_packet(packet);
    crate::game::remote::fnr_probe::check_player(game);
    let result = game.remote_teleport(p);
    if result.is_ok() {
        crate::game::remote::fnr_probe::replay_use(game);
    }
    ::core::ops::ControlFlow::Break(result)
}
