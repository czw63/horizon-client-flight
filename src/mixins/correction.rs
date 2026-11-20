fn intercept_correction(
    game: &mut Game<'_>,
    packet: &play::PlayerPosition,
) -> ::core::ops::ControlFlow<Result<()>> {
    let matched = client_flight::correction::matches(
        packet.relatives,
        packet.position,
        packet.delta,
        packet.pitch,
    );
    crate::game::remote::fnr_probe::diagnostic(
        log::Level::Info,
        format_args!(
            "recv teleport id={} relatives={} pos={:?} delta={:?} yaw={:?} pitch={:?} matched={matched}",
            packet.teleport,
            packet.relatives,
            packet.position,
            packet.delta,
            packet.yaw,
            packet.pitch,
        ),
    );
    // Result<T> = std::result::Result<T, Box<dyn std::error::Error>> in horizon_client_core.
    let Some(player) = game.player.as_ref().filter(|_| game.remote.is_some()) else {
        client_flight::correction::clear();
        return ::core::ops::ControlFlow::Continue(());
    };
    let Some(interaction) = client_flight::correction::receive(
        packet.relatives,
        packet.position,
        packet.delta,
        packet.pitch,
    ) else {
        return ::core::ops::ControlFlow::Continue(());
    };
    crate::game::remote::fnr_probe::diagnostic(
        log::Level::Info,
        format_args!(
            "ack teleport id={} with pos={:?} yaw={:?} pitch={:?}",
            packet.teleport, player.position, game.yaw, game.pitch,
        ),
    );
    game.send(S::AcceptTeleportation(play::AcceptTeleportation {
        id: packet.teleport,
        x: player.position.x,
        y: player.position.y,
        z: player.position.z,
        yaw: game.yaw,
        pitch: game.pitch,
    }));
    // The acknowledgement must enter the connection before the deferred input.
    if let Some(interaction) = interaction {
        interaction();
    }
    ::core::ops::ControlFlow::Break(Ok(()))
}
