fn tick_collision_probe(game: &mut Game<'_>) {
    // AFTER_CALL: the normal position/pitch packet must precede the marker probe.
    crate::game::remote::fnr_probe::tick(game);
}
