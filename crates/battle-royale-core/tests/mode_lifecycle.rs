use battle_royale_core::*;
use mmorpg_core::{PlayerClass, Sex};
fn start(simulation: &mut BattleRoyaleMatch) {
    simulation.add_player(1).unwrap();
    simulation.add_player(2).unwrap();
    for _ in 0..LOBBY_COUNTDOWN_TICKS {
        simulation.advance_tick().unwrap();
    }
    assert_eq!(simulation.phase(), MatchPhase::Drop);
    assert!(simulation.add_player(3).is_err());
    for _ in 0..DROP_DURATION_TICKS {
        simulation.advance_tick().unwrap();
    }
    assert_eq!(simulation.phase(), MatchPhase::Combat);
}
#[test]
fn lobby_selection_movement_and_phase_lock_share_real_character_state() {
    let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(7));
    simulation.add_player(1).unwrap();
    for _ in 0..LOBBY_COUNTDOWN_TICKS + 5 {
        simulation.advance_tick().unwrap();
    }
    assert_eq!(simulation.phase(), MatchPhase::Waiting);
    let build = EphemeralCharacterBuild::standard(ClassChoice {
        class: PlayerClass::Arcanist,
        sex: Sex::Female,
    })
    .unwrap();
    simulation
        .apply_command(1, 1, MatchCommand::SelectBuild(build))
        .unwrap();
    let view = simulation.snapshot_for_player(1).unwrap();
    assert_eq!(view.zone.viewer.class, Some(build.class()));
    assert_eq!(view.zone.viewer.level, mmorpg_core::MAX_PLAYER_LEVEL);
    simulation
        .apply_command(1, 2, MatchCommand::SetMovement { x: 1, z: 0 })
        .unwrap();
    simulation.advance_tick().unwrap();
    let before = simulation.snapshot().unwrap();
    assert!(before.zone.players[0].position[0] > -3600.0);
    assert!(
        simulation
            .apply_command(1, 2, MatchCommand::SetMovement { x: 0, z: 1 })
            .is_err()
    );
    assert_eq!(simulation.snapshot().unwrap(), before);
    simulation.add_player(2).unwrap();
    for _ in 0..LOBBY_COUNTDOWN_TICKS {
        simulation.advance_tick().unwrap();
    }
    assert_eq!(simulation.phase(), MatchPhase::Drop);
    let before = simulation.snapshot().unwrap();
    for command in [
        MatchCommand::SelectBuild(build),
        MatchCommand::Character(ZoneCommand::ChooseClass { class: 0, sex: 0 }),
        MatchCommand::Character(ZoneCommand::UnequipItem { equipment_slot: 0 }),
        MatchCommand::Character(ZoneCommand::ReleaseSpirit),
    ] {
        assert!(simulation.apply_command(1, 3, command).is_err());
        assert_eq!(simulation.snapshot().unwrap(), before);
    }
    let view = simulation.snapshot_for_player(1).unwrap();
    assert_eq!(view.acknowledged_sequence, 2);
    assert_eq!(view.players.len(), 2);
}
#[test]
fn retirement_storm_and_finished_state_preserve_existing_mode_rules() {
    let mut lobby = BattleRoyaleMatch::new(BattleMatchId::new(13));
    lobby.add_player(1).unwrap();
    assert!(lobby.remove_player(1));
    assert_eq!(lobby.player_count(), 0);
    assert!(lobby.snapshot().unwrap().zone.players.is_empty());
    let mut active = BattleRoyaleMatch::new(BattleMatchId::new(14));
    start(&mut active);
    let before = active.storm_snapshot();
    for _ in 0..STORM_DELAY_TICKS + 1 {
        active.advance_tick().unwrap();
    }
    let shrinking = active.storm_snapshot();
    assert_eq!(before.radius, INITIAL_STORM_RADIUS_UNITS);
    assert!(shrinking.damage_active && shrinking.radius < before.radius);
    assert_eq!(before.center, shrinking.center);
    assert!(active.remove_player(1));
    assert_eq!(active.phase(), MatchPhase::Finished);
    assert_eq!(active.winner(), Some(2));
    let finished = active.snapshot().unwrap();
    assert_eq!(
        finished.players[0].elimination_cause,
        Some(EliminationCause::SessionExpired)
    );
    assert_eq!(finished.zone.players.len(), 1);
    BattleRoyaleMatch::from_snapshot(finished.clone()).unwrap();
    assert!(active.remove_player(2));
    assert_eq!(active.snapshot().unwrap(), finished);
    active.advance_tick().unwrap();
    let later = active.snapshot().unwrap();
    assert_eq!(later.zone, finished.zone);
    assert!(
        active
            .apply_command(2, 1, MatchCommand::SetMovement { x: 1, z: 0 })
            .is_err()
    );
}

#[test]
fn actual_storm_damage_and_elimination_continue_through_a_complete_checkpoint() {
    let mut simulation = BattleRoyaleMatch::new(BattleMatchId::new(11));
    start(&mut simulation);
    let initial = simulation.snapshot().unwrap().zone.players[0].combat.health;
    let mut damaged = false;
    for _ in 0..1600 {
        simulation.advance_tick().unwrap();
        let state = simulation.snapshot().unwrap();
        damaged |= state.zone.players.iter().any(|p| p.combat.health < initial);
        if damaged {
            let mut restored = BattleRoyaleMatch::from_snapshot(state).unwrap();
            for _ in 0..300 {
                simulation.advance_tick().unwrap();
                restored.advance_tick().unwrap();
                let expected = simulation.snapshot().unwrap();
                assert_eq!(expected, restored.snapshot().unwrap());
                if simulation.phase() == MatchPhase::Finished {
                    assert!(
                        expected
                            .players
                            .iter()
                            .any(|p| p.elimination_cause == Some(EliminationCause::Storm))
                    );
                    assert!(expected.zone.players.iter().any(|p| p.combat.health == 0));
                    return;
                }
            }
            panic!("ongoing storm must eliminate a character");
        }
    }
    panic!("shrinking storm must apply real shared-zone damage");
}
