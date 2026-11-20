fn defer_probe_interaction(
    game: &mut Game<'_>,
    attempt: mod_api::InteractionAttempt,
) -> ::core::ops::ControlFlow<()> {
    if crate::game::remote::fnr_probe::defer_use(game, attempt) {
        ::core::ops::ControlFlow::Break(())
    } else {
        ::core::ops::ControlFlow::Continue(())
    }
}
