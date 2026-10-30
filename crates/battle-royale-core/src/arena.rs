//! Mode-authored arena geometry; all character rules belong to the shared zone.
use mmorpg_core::{
    CharacterPersistence, PlayerCombatPolicy, SpawnGrid, StaticCollider, ZoneContent,
    ZoneDefinition, ZoneError,
};
use std::sync::{Arc, OnceLock};

pub(crate) fn content() -> Result<Arc<ZoneContent>, ZoneError> {
    static CONTENT: OnceLock<Result<Arc<ZoneContent>, ZoneError>> = OnceLock::new();
    CONTENT.get_or_init(build_content).clone()
}

fn build_content() -> Result<Arc<ZoneContent>, ZoneError> {
    // Keep the existing flat arena and 8 m spawn separation. Sixteen columns
    // also keep every spawn slot validated by the shared zone inside its bounds.
    let definition = ZoneDefinition::with_spawn_grid(
        [0, -1, 0],
        SpawnGrid {
            origin: [-3_600, -3_600],
            columns: 16,
            spacing: 800,
        },
        vec![StaticCollider {
            id: 1,
            position: [0, -50, 0],
            half_extents: [32_000, 50, 32_000],
        }],
    )?;
    let content = ZoneContent::from_definition(definition)
        .with_creature_abilities(Vec::new())?
        .with_character_persistence(CharacterPersistence::Ephemeral)
        .with_player_combat(PlayerCombatPolicy::FreeForAll);
    Ok(Arc::new(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmorpg_core::{
        ClassChoice, EphemeralCharacterBuild, PlayerClass, Sex, ZoneId, ZoneSimulation,
    };

    #[test]
    fn the_mode_arena_admits_full_capacity_through_shared_character_rules() {
        let content = content().unwrap();
        let mut zone =
            ZoneSimulation::with_content(ZoneId::new(100), Arc::clone(&content)).unwrap();
        for player in 1..=crate::MAX_PLAYERS_PER_MATCH as u32 {
            let build = EphemeralCharacterBuild::standard(ClassChoice {
                class: PlayerClass::ALL[(player as usize - 1) % PlayerClass::ALL.len()],
                sex: Sex::Male,
            })
            .unwrap();
            zone.admit_ephemeral(player, &build).unwrap();
        }
        assert!(zone.players_are_hostile(1, 2));
        let initial = zone.snapshot().unwrap();
        assert_eq!(initial.players.len(), crate::MAX_PLAYERS_PER_MATCH);
        assert!(initial.creatures.is_empty());
        let mut restored = ZoneSimulation::from_snapshot(initial, content).unwrap();
        zone.advance_tick().unwrap();
        restored.advance_tick().unwrap();
        assert_eq!(zone.snapshot().unwrap(), restored.snapshot().unwrap());
    }
}
