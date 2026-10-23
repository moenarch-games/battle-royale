//! Explicit real-network acceptance, using the same host and wire contracts as the MMORPG client.
use battle_royale_core::{
    BattleMatchId, ClassChoice, EntityRef, EphemeralCharacterBuild, MatchCommand, MatchPhase,
    PlayerClass, Sex, ZoneCommand,
};
use battle_royale_game_server::{build_match_host, route_match_id};
use battle_royale_protocol::{decode_snapshot, encode_command};
use game_server::{
    BrowserRoutePrefix, MatchHostWebTransportConfig, WELCOME_BYTES, Welcome, decode_welcome,
    serve_match_host_with_shutdown,
};
use std::{path::PathBuf, process::Command, time::Duration};
use tokio::sync::mpsc;
use wtransport::{ClientConfig, Connection, Endpoint, Identity};
struct TestDirectory(PathBuf);
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn welcome(connection: &Connection) -> Welcome {
    let mut stream = connection.accept_uni().await.unwrap();
    let mut bytes = [0; WELCOME_BYTES];
    stream.read_exact(&mut bytes).await.unwrap();
    decode_welcome(&bytes).unwrap()
}
async fn snapshot(connection: &Connection) -> battle_royale_core::MatchSnapshot {
    let bytes = connection.receive_datagram().await.unwrap();
    let frame = game_server::decode_snapshot(&bytes).unwrap();
    decode_snapshot(&frame.payload).unwrap()
}
fn send(connection: &Connection, sequence: u32, command: MatchCommand) {
    connection
        .send_datagram(game_server::encode_command(sequence, &encode_command(command)).unwrap())
        .unwrap();
}
#[tokio::test]
#[ignore = "real UDP/TLS acceptance; run explicitly with --ignored; requires openssl"]
async fn two_real_clients_select_builds_fight_and_receive_the_same_winner() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let directory = TestDirectory(
            std::env::temp_dir().join(format!("battle-royale-hosted-pvp-{}", std::process::id())),
        );
        std::fs::create_dir(&directory.0).unwrap();
        let certificate = directory.0.join("cert.pem");
        let key = directory.0.join("key.pem");
        let result = Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "ec",
                "-pkeyopt",
                "ec_paramgen_curve:prime256v1",
                "-nodes",
                "-days",
                "10",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=DNS:localhost,IP:127.0.0.1",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&certificate)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "test certificate generation failed"
        );
        let identity = Identity::load_pemfiles(&certificate, &key).await.unwrap();
        let hash = identity.certificate_chain().as_slice()[0].hash();
        let reservation = std::net::UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let prefix = BrowserRoutePrefix::new("/game").unwrap();
        let match_id = BattleMatchId::new(42);
        let url = format!(
            "https://127.0.0.1:{port}{}",
            prefix.match_path(&route_match_id(match_id).unwrap())
        );
        let config = MatchHostWebTransportConfig {
            port,
            certificate_pem: certificate,
            private_key_pem: key,
            route_prefix: prefix,
            drain_grace: Duration::ZERO,
            allowed_origins: None,
        };
        let (shutdown, receiver) = mpsc::channel(1);
        let server = tokio::spawn(serve_match_host_with_shutdown(
            build_match_host([match_id], 300).unwrap(),
            config,
            receiver,
        ));
        let endpoint = Endpoint::client(
            ClientConfig::builder()
                .with_bind_default()
                .with_server_certificate_hashes([hash])
                .build(),
        )
        .unwrap();
        let mut attempt = 0;
        let attacker = loop {
            match tokio::time::timeout(Duration::from_millis(500), endpoint.connect(&url)).await {
                Ok(Ok(connection)) => break connection,
                _ if attempt < 4 => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                _ => panic!("host did not start"),
            }
        };
        let first = welcome(&attacker).await;
        let victim = endpoint.connect(&url).await.unwrap();
        let second = welcome(&victim).await;
        assert_ne!(first.player_id, second.player_id);
        let build = EphemeralCharacterBuild::standard(ClassChoice {
            class: PlayerClass::Arcanist,
            sex: Sex::Female,
        })
        .unwrap();
        send(&attacker, 1, MatchCommand::SelectBuild(build));
        let mut selected = false;
        loop {
            let view = snapshot(&attacker).await;
            if view.acknowledged_sequence >= 1 {
                assert_eq!(view.zone.viewer.class, Some(build.class()));
                selected = true;
            }
            if view.phase == MatchPhase::Combat {
                assert!(selected);
                break;
            }
        }
        let initial = snapshot(&victim).await.zone.viewer.health;
        let firebolt = MatchCommand::Character(ZoneCommand::UseAbility {
            ability: 9,
            target: Some(EntityRef::Player(second.player_id)),
        });
        let mut sequence = 2;
        let mut last_cast = 0;
        let mut hit = false;
        loop {
            let view = snapshot(&attacker).await;
            if view.phase == MatchPhase::Combat
                && (last_cast == 0 || view.tick.saturating_sub(last_cast) >= 90)
            {
                send(&attacker, sequence, firebolt);
                sequence += 1;
                last_cast = view.tick;
            }
            let other = snapshot(&victim).await;
            hit |= other.zone.viewer.health < initial;
            if view.phase == MatchPhase::Finished || other.phase == MatchPhase::Finished {
                assert!(hit, "the victim must observe actual shared health damage");
                let mut a = view;
                let mut b = other;
                while a.phase != MatchPhase::Finished {
                    a = snapshot(&attacker).await;
                }
                while b.phase != MatchPhase::Finished {
                    b = snapshot(&victim).await;
                }
                assert_eq!(a.winner, Some(first.player_id));
                assert_eq!(b.winner, a.winner);
                assert!(b.zone.viewer.dead);
                break;
            }
        }
        shutdown.send(()).await.unwrap();
        server.await.unwrap().unwrap();
    })
    .await
    .expect("two-client hosted PvP completed within 45 seconds");
}
