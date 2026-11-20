//! Client Flight for Horizon: client-side flight, elytra boost and no-fall,
//! ported from the Fabric mod Client-Flight-Mod.
//!
//! State and settings live in this crate; the four points that need game
//! internals are `[[native.mixin]]` helpers under `src/mixins/`.
use mod_api::Hooks;

pub mod correction;
pub mod nofall;

#[derive(Default)]
pub struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
