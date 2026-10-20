//! Pinned public API proof for MMORPG #339, not the runtime match migration (#13).
use mmorpg_core::{
    EntityRef, PlayerCombatPolicy, ZoneCommand, ZoneId, ZoneSimulation, greyhaven_vale,
};
use std::sync::Arc;

#[test]
fn pinned_mmorpg_combat_cast_hit_and_recovery_are_usable_by_the_mode() {
    let safe = greyhaven_vale::content();
    assert_eq!(safe.player_combat_policy(), PlayerCombatPolicy::Safe);
    let content =
        Arc::new(Arc::unwrap_or_clone(safe).with_player_combat(PlayerCombatPolicy::FreeForAll));
    let mut zone = ZoneSimulation::with_content(ZoneId::new(100), Arc::clone(&content)).unwrap();
    for (player, class) in [(1, 2), (2, 0)] {
        zone.add_player(player).unwrap();
        zone.apply_command(player, 1, ZoneCommand::ChooseClass { class, sex: 0 })
            .unwrap();
    }
    zone.advance_tick().unwrap();
    assert!(zone.players_are_hostile(1, 2));
    let health = zone.snapshot_for_player(2).unwrap().viewer.health;
    zone.apply_command(
        1,
        2,
        ZoneCommand::UseAbility {
            ability: 9,
            target: Some(EntityRef::Player(2)),
        },
    )
    .unwrap();
    for _ in 0..15 {
        zone.advance_tick().unwrap();
    }
    assert!(zone.snapshot_for_player(1).unwrap().viewer.cast.is_some());
    let checkpoint = mmorpg_protocol::encode_canonical_snapshot(&zone.snapshot().unwrap()).unwrap();
    let state = mmorpg_protocol::decode_canonical_snapshot(&checkpoint).unwrap();
    let mut restored = ZoneSimulation::from_snapshot(state, content).unwrap();
    let mut damaged = false;
    for _ in 0..120 {
        zone.advance_tick().unwrap();
        restored.advance_tick().unwrap();
        assert_eq!(zone.snapshot().unwrap(), restored.snapshot().unwrap());
        damaged |= zone.snapshot_for_player(2).unwrap().viewer.health < health;
    }
    assert!(
        damaged,
        "the shared core, not a mode-specific combat loop, resolves the hit"
    );
}
