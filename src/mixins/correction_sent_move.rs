fn record_sent_move(game: &Game<'_>, packet: &S) {
    let (position, rotation, flags) = match packet {
        S::MovePlayerPosRot(p) => (Some([p.x, p.y, p.z]), Some([p.yaw, p.pitch]), p.flags),
        S::MovePlayerPos(p) => (Some([p.x, p.y, p.z]), None, p.flags),
        S::MovePlayerRot(p) => (None, Some([p.yaw, p.pitch]), p.flags),
        S::MovePlayerStatus(p) => (None, None, p.flags),
        _ => return,
    };
    client_flight::correction::record_sent_move(client_flight::correction::SentMove {
        tick: game.player.as_ref().map(|p| p.health.tick_count),
        position,
        rotation,
        flags,
    });
}
