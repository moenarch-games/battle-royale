use battle_royale_core::{
    BattleMatchId, BattleRoyaleMatch, ClassChoice, DROP_DURATION_TICKS, EliminationCause,
    EphemeralCharacterBuild, LOBBY_COUNTDOWN_TICKS, MatchCommand, MatchPhase, ZoneCommand,
};
use battle_royale_game_server::BattleRoyaleGameServerAdapter;
use battle_royale_protocol::{
    decode_canonical_snapshot, encode_canonical_snapshot, encode_command,
};
use game_server::{CommandOutcome, MatchRuntime, RECONNECT_TOKEN_BYTES, ReconnectToken};
fn token(value: u8) -> ReconnectToken {
    ReconnectToken([value; RECONNECT_TOKEN_BYTES])
}
#[test]
fn shared_hit_elimination_winner_duplicate_reconnect_and_restore_form_one_match() {
    let mut runtime = MatchRuntime::new_with_replay_capture(
        BattleRoyaleGameServerAdapter::new(BattleMatchId::new(42)),
        300,
    );
    let attacker = runtime.admit(token(1)).unwrap();
    let victim = runtime.admit(token(2)).unwrap();
    let build = EphemeralCharacterBuild::standard(ClassChoice {
        class: battle_royale_core::PlayerClass::Arcanist,
        sex: battle_royale_core::Sex::Female,
    })
    .unwrap();
    runtime
        .submit_command(
            attacker.player_id,
            attacker.connection_epoch,
            1,
            &encode_command(MatchCommand::SelectBuild(build)),
        )
        .unwrap();
    for _ in 0..LOBBY_COUNTDOWN_TICKS + DROP_DURATION_TICKS {
        runtime.advance_tick().unwrap();
    }
    let before = decode_canonical_snapshot(&runtime.snapshot().unwrap().payload).unwrap();
    let health = before.zone.players[1].combat.health;
    // Offensive abilities fire where the caster faces (free aim), so turn toward the victim.
    let [from, to] = [0, 1].map(|index| before.zone.players[index].position);
    let yaw = (to[0] - from[0]).atan2(to[2] - from[2]) / std::f32::consts::TAU * 65_536.0;
    let face = MatchCommand::Character(ZoneCommand::Move {
        forward: 0,
        strafe: 0,
        facing: (yaw.round() as i32).rem_euclid(65_536) as u16,
    });
    runtime
        .submit_command(
            attacker.player_id,
            attacker.connection_epoch,
            2,
            &encode_command(face),
        )
        .unwrap();
    runtime.advance_tick().unwrap();
    let firebolt = MatchCommand::Character(ZoneCommand::UseAbility {
        ability: 9,
        target: Some(battle_royale_core::EntityRef::Player(victim.player_id)),
    });
    runtime
        .submit_command(
            attacker.player_id,
            attacker.connection_epoch,
            3,
            &encode_command(firebolt),
        )
        .unwrap();
    assert_eq!(
        runtime
            .submit_command(
                attacker.player_id,
                attacker.connection_epoch,
                3,
                &encode_command(firebolt)
            )
            .unwrap(),
        CommandOutcome::IgnoredStale
    );
    for _ in 0..15 {
        runtime.advance_tick().unwrap();
    }
    let checkpoint = decode_canonical_snapshot(&runtime.snapshot().unwrap().payload).unwrap();
    assert!(checkpoint.zone.players[0].combat.abilities.cast.is_some());
    let mut expected = BattleRoyaleMatch::from_snapshot(checkpoint).unwrap();
    assert!(runtime.disconnect(attacker.player_id, attacker.connection_epoch));
    runtime.freeze_for_recovery();
    let image = runtime.recovery_image().unwrap();
    let mut restored = MatchRuntime::restore_from_recovery(
        BattleRoyaleGameServerAdapter::new(BattleMatchId::new(42)),
        image,
    )
    .unwrap();
    let reconnected = restored.reconnect(token(1), token(3)).unwrap();
    let victim_reconnected = restored.reconnect(token(2), token(4)).unwrap();
    assert_eq!(victim_reconnected.player_id, victim.player_id);
    assert_eq!(reconnected.player_id, attacker.player_id);
    assert_eq!(reconnected.connection_epoch, attacker.connection_epoch + 1);
    let mut hit = false;
    let mut sequence = 4;
    for elapsed in 0..1_600 {
        if elapsed > 0 && elapsed % 30 == 0 {
            restored
                .submit_command(
                    attacker.player_id,
                    reconnected.connection_epoch,
                    sequence,
                    &encode_command(firebolt),
                )
                .unwrap();
            expected
                .apply_command(attacker.player_id, sequence, firebolt)
                .unwrap();
            sequence += 1;
        }
        restored.advance_tick().unwrap();
        expected.advance_tick().unwrap();
        let bytes = restored.snapshot().unwrap().payload;
        let canonical = decode_canonical_snapshot(&bytes).unwrap();
        assert_eq!(
            bytes,
            encode_canonical_snapshot(&expected.snapshot().unwrap()).unwrap()
        );
        hit |= canonical.zone.players[1].combat.health < health;
        if canonical.phase == MatchPhase::Finished {
            assert!(hit);
            assert_eq!(canonical.winner, Some(attacker.player_id));
            assert_eq!(
                canonical.players[1].elimination_cause,
                Some(EliminationCause::Combat)
            );
            assert_eq!(canonical.zone.players[1].combat.health, 0);
            assert!(
                expected
                    .apply_command(
                        victim.player_id,
                        99,
                        MatchCommand::Character(ZoneCommand::ReleaseSpirit)
                    )
                    .is_err()
            );
            return;
        }
    }
    panic!("actual MMORPG combat must finish the match");
}
