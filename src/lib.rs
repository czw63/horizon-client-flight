//! Client Flight for Horizon: client-side flight, elytra boost and no-fall,
//! ported from the Fabric mod Client-Flight-Mod.
//!
//! State and settings live in this crate; the points that need game
//! internals are `[[native.mixin]]` helpers under `src/mixins/`.
use mod_api::Hooks;

pub mod commands;
pub mod correction;
pub mod nofall;
pub mod probe_geometry;

#[derive(Default)]
pub struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_client_commands(
        &self,
        _host: &mut dyn mod_api::ClientHost,
        event: &mut mod_api::RegisterClientCommandsEvent<'_>,
    ) {
        event
            .commands
            .extend(commands::USAGES.map(mod_api::Command::new));
    }

    fn command(
        &self,
        host: &mut dyn mod_api::ClientHost,
        event: &mut mod_api::CommandEvent<'_>,
    ) -> mod_api::Flow {
        let Some(reply) = commands::run(event.command) else {
            return mod_api::Flow::Continue;
        };
        let _ = host.chat_line(mod_api::Text::literal(format!("[Client Flight] {reply}")));
        mod_api::Flow::Cancel
    }
}

#[cfg(test)]
mod flight_velocity_tests;
