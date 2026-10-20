//! The mode consumes temporary builds through public MMORPG admission, not private state edits.
use mmorpg_core::{
    CharacterId, CharacterPersistence, ClassChoice, EphemeralCharacterBuild, MAX_PLAYER_LEVEL,
    PlayerClass, PlayerCombatPolicy, Sex, ZoneId, ZoneSimulation, greyhaven_vale,
};
use std::sync::Arc;

#[test]
fn every_class_can_enter_as_a_temporary_max_level_build_and_restore() {
    let content = Arc::new(
        Arc::unwrap_or_clone(greyhaven_vale::content())
            .with_player_combat(PlayerCombatPolicy::FreeForAll)
            .with_character_persistence(CharacterPersistence::Ephemeral),
    );
    let mut zone = ZoneSimulation::with_content(ZoneId::new(100), Arc::clone(&content)).unwrap();
    for (index, class) in PlayerClass::ALL.into_iter().enumerate() {
        let player = index as u32 + 1;
        let choice = ClassChoice {
            class,
            sex: Sex::Female,
        };
        let build = EphemeralCharacterBuild::standard(choice).unwrap();
        zone.admit_ephemeral(player, &build).unwrap();
        let view = zone.snapshot_for_player(player).unwrap();
        assert_eq!(view.viewer.level, MAX_PLAYER_LEVEL);
        assert_eq!(view.viewer.health, view.viewer.max_health);
        assert_eq!(view.viewer.class, Some(choice));
        assert_eq!(
            view.viewer.resource.unwrap().value,
            class.resource().start(MAX_PLAYER_LEVEL)
        );
        assert!(
            zone.capture_character(
                player,
                CharacterId::new(format!("account-{player}")).unwrap()
            )
            .is_err()
        );
    }
    assert!(zone.players_are_hostile(1, 2));
    let bytes = mmorpg_protocol::encode_canonical_snapshot(&zone.snapshot().unwrap()).unwrap();
    let mut restored = ZoneSimulation::from_snapshot(
        mmorpg_protocol::decode_canonical_snapshot(&bytes).unwrap(),
        content,
    )
    .unwrap();
    for _ in 0..20 {
        zone.advance_tick().unwrap();
        restored.advance_tick().unwrap();
        assert_eq!(zone.snapshot().unwrap(), restored.snapshot().unwrap());
    }
}
