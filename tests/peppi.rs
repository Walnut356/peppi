use std::{
	fs::{self, File},
	io::Read,
	path::Path,
};

use pretty_assertions::assert_eq;
use serde_json::json;
use xxhash_rust::xxh3::xxh3_64;

use peppi::{
	model::{
		frame::transpose,
		game::{
			Bytes, DashBack, End, EndMethod, Language, Netplay, Player, PlayerType, Port,
			Scene, ShieldDrop, Start, Ucf,
		},
		game::immutable::Game,
		shift_jis::MeleeString,
		slippi::{Slippi, Version},
	},
	serde,
};

mod common;
use common::{game, get_path, read_game};

#[derive(Copy, Clone, Debug, PartialEq)]
struct Buttons {
	physical: u16,
	logical: u32,
}

fn button_seq(game: &Game) -> Vec<Buttons> {
	let mut last_buttons: Option<Buttons> = None;
	let mut button_seq = vec![];
	for idx in 0..game.frames.id.len() {
		let b = Buttons {
			logical: game.frames.port[0].leader.pre.buttons.values()[idx],
			physical: game.frames.port[0].leader.pre.buttons_physical.values()[idx],
		};
		if (b.physical > 0 || b.logical > 0) && Some(b) != last_buttons {
			button_seq.push(b);
			last_buttons = Some(b);
		}
	}
	button_seq
}

#[test]
fn slippi_old_version() {
	let game = game("v0.1");
	let players = game.start.players;

	assert_eq!(game.start.slippi.version, Version(0, 1, 0));
	assert_eq!(
		serde_json::Value::Object(game.metadata.unwrap()),
		json!({
			"startAt": "2018-01-24T06:19:54Z",
			"playedOn": "dolphin"
		})
	);

	assert_eq!(players.len(), 2);
	assert_eq!(players[0].character, 2); // Fox
	assert_eq!(players[1].character, 25); // Ganondorf
}

#[test]
fn basic_game() {
	let game = game("game");

	assert_eq!(
		serde_json::Value::Object(game.metadata.unwrap()),
		json!({
			"startAt": "2018-06-22T07:52:59Z",
			"lastFrame": 5085,
			"players": {
				"1": {
					"characters": {
						"1": 5209, // Fox
					}
				},
				"0": {
					"characters": {
						"18": 5209 // Marth
					}
				}
			},
			"playedOn": "dolphin"
		})
	);

	assert_eq!(
		game.start,
		Start {
			slippi: Slippi {
				version: Version(1, 0, 0)
			},
			bitfield: [50, 1, 134, 76],
			is_raining_bombs: false,
			is_teams: false,
			item_spawn_frequency: -1,
			self_destruct_score: -1,
			stage: 8,
			timer: 480,
			item_spawn_bitfield: [255, 255, 255, 255, 255],
			damage_ratio: 1.0,
			players: vec![
				Player {
					port: Port::P1,
					character: 9, // Marth
					r#type: PlayerType::Human,
					stocks: 4,
					costume: 3,
					team: None,
					handicap: 9,
					bitfield: 192,
					cpu_level: None,
					offense_ratio: 1.0,
					defense_ratio: 1.0,
					model_scale: 1.0,
					ucf: Some(Ucf {
						dash_back: None,
						shield_drop: None,
					}),
					name_tag: None,
					netplay: None,
				},
				Player {
					port: Port::P2,
					character: 2, // Fox
					r#type: PlayerType::Cpu,
					stocks: 4,
					costume: 0,
					team: None,
					handicap: 9,
					bitfield: 64,
					cpu_level: Some(1),
					offense_ratio: 1.0,
					defense_ratio: 1.0,
					model_scale: 1.0,
					ucf: Some(Ucf {
						dash_back: None,
						shield_drop: None,
					}),
					name_tag: None,
					netplay: None,
				},
			],
			random_seed: 3803194226,
			is_pal: None,
			is_frozen_ps: None,
			scene: None,
			language: None,
			bytes: Bytes(vec![
				1, 0, 0, 0, 50, 1, 134, 76, 195, 0, 0, 0, 0, 0, 0, 255, 255, 110, 0, 8, 0, 0, 1,
				224, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0,
				0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 9, 0, 4, 3, 0, 0, 0, 0, 9, 0, 120, 0, 192, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0,
				63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 2, 1, 4, 0, 0, 1, 0, 0, 9, 0, 120, 0,
				64, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0,
				26, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0, 64, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 63,
				128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 26, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0,
				64, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0,
				33, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0, 64, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63,
				128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 33, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0,
				64, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0,
				226, 176, 35, 114, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
			]),
		}
	);

	assert_eq!(
		game.end.unwrap(),
		End {
			method: EndMethod::Resolved,
			lras_initiator: None,
			bytes: Bytes(vec![3]),
		}
	);

	assert_eq!(game.frames.id.len(), 5209);
}

#[test]
fn ics() {
	let game = game("ics");
	assert_eq!(
		game.metadata.unwrap()["players"],
		json!({
			"1": {
				"characters": {
					"15": 344 // Jigglypuff
				}
			},
			"0": {
				"characters": {
					"11": 344, // Nana
					"10": 344 // Popo
				}
			}
		})
	);
	assert_eq!(game.start.players[0].character, 14);
	assert!(game.frames.port[0].follower.is_some());
}

#[test]
fn ucf() {
	assert_eq!(
		game("shield_drop").start.players[0].ucf,
		Some(Ucf {
			dash_back: None,
			shield_drop: Some(ShieldDrop::Ucf),
		})
	);
	assert_eq!(
		game("dash_back").start.players[0].ucf,
		Some(Ucf {
			dash_back: Some(DashBack::Ucf),
			shield_drop: None,
		})
	);
}

#[test]
fn buttons_lzrs() {
	let game = game("buttons_lrzs");
	assert_eq!(
		button_seq(&game),
		vec![
			Buttons {
				logical: 2147483648, // Trigger
				physical: 0,
			},
			Buttons {
				logical: 2147483712, // Trigger | L
				physical: 64,        // L
			},
			Buttons {
				logical: 2147483648, // Trigger
				physical: 0,
			},
			Buttons {
				logical: 2147483680, // Trigger | R
				physical: 32,        // R
			},
			Buttons {
				logical: 2147483920, // Trigger | A | Z
				physical: 16,        // Z
			},
			Buttons {
				logical: 4096,  // Start
				physical: 4096, // Start
			},
		]
	);
}

#[test]
fn buttons_abxy() {
	let game = game("buttons_abxy");
	assert_eq!(
		button_seq(&game),
		vec![
			Buttons {
				logical: 256,  // A
				physical: 256, // A
			},
			Buttons {
				logical: 512,  // B
				physical: 512, // B
			},
			Buttons {
				logical: 1024,  // X
				physical: 1024, // X
			},
			Buttons {
				logical: 2048,  // Y
				physical: 2048, // Y
			},
		]
	);
}

#[test]
fn dpad_udlr() {
	let game = game("dpad_udlr");
	assert_eq!(
		button_seq(&game),
		vec![
			Buttons {
				logical: 8,  // D-pad up
				physical: 8, // D-pad up
			},
			Buttons {
				logical: 4,  // D-pad down
				physical: 4, // D-pad down
			},
			Buttons {
				logical: 1,  // D-pad left
				physical: 1, // D-pad left
			},
			Buttons {
				logical: 2,  // D-pad right
				physical: 2, // D-pad right
			},
		]
	);
}

#[test]
fn cstick_udlr() {
	let game = game("cstick_udlr");
	assert_eq!(
		button_seq(&game),
		vec![
			Buttons {
				logical: 1048576, // C-stick up
				physical: 0,
			},
			Buttons {
				logical: 2097152, // C-stick down
				physical: 0,
			},
			Buttons {
				logical: 4194304, // C-stick left
				physical: 0,
			},
			Buttons {
				logical: 8388608, // C-stick right
				physical: 0,
			},
		]
	);
}

#[test]
fn joystick_udlr() {
	let game = game("joystick_udlr");
	assert_eq!(
		button_seq(&game),
		vec![
			Buttons {
				logical: 65536, // Joystick up
				physical: 0,
			},
			Buttons {
				logical: 131072, // Joystick down
				physical: 0,
			},
			Buttons {
				logical: 262144, // Joystick left
				physical: 0,
			},
			Buttons {
				logical: 524288, // Joystick right
				physical: 0,
			},
		]
	);
}

#[test]
fn nintendont() {
	let game = game("nintendont");
	assert_eq!(
		game.metadata.unwrap()["playedOn"],
		serde_json::Value::String("nintendont".to_string())
	);
}

#[test]
fn netplay() {
	let game = game("netplay");
	assert_eq!(
		game.metadata.unwrap()["players"],
		json!({
			"0": {
				"names": {
					"netplay": "abcdefghijk",
					"code": "ABCD#123"
				},
				"characters": {
					"13": 128,
				}
			},
			"1": {
				"names": {
					"netplay": "nobody",
					"code": "XX#000"
				},
				"characters": {
					"18": 128,
				}
			}
		})
	);
}

#[test]
fn console_name() {
	let game = game("console_name");
	assert_eq!(
		game.metadata.unwrap()["consoleNick"],
		serde_json::Value::String("Station 1".to_string())
	)
}

#[test]
fn v2() {
	let game = game("v2.0");
	assert_eq!(game.start.slippi.version, Version(2, 0, 1));
}

#[test]
fn v3_12() {
	let game = game("v3.12");

	assert_eq!(
		game.start,
		Start {
			slippi: Slippi {
				version: Version(3, 12, 0)
			},
			bitfield: [50, 1, 142, 76],
			is_raining_bombs: false,
			is_teams: false,
			item_spawn_frequency: -1,
			self_destruct_score: -1,
			stage: 3,
			timer: 480,
			item_spawn_bitfield: [255, 255, 255, 255, 255],
			damage_ratio: 1.0,
			players: vec![
				Player {
					port: Port::P1,
					character: 9, // Marth
					r#type: PlayerType::Human,
					stocks: 4,
					costume: 3,
					team: None,
					handicap: 9,
					bitfield: 192,
					cpu_level: None,
					offense_ratio: 1.0,
					defense_ratio: 1.0,
					model_scale: 1.0,
					ucf: Some(Ucf {
						dash_back: Some(DashBack::Ucf),
						shield_drop: Some(ShieldDrop::Ucf),
					}),
					name_tag: Some(MeleeString("".to_string())),
					netplay: Some(Netplay {
						name: MeleeString("xxxxxx".to_string()),
						code: MeleeString("XX＃111".to_string()),
						suid: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
					})
				},
				Player {
					port: Port::P2,
					character: 9, // Marth
					r#type: PlayerType::Human,
					stocks: 4,
					costume: 0,
					team: None,
					handicap: 9,
					bitfield: 192,
					cpu_level: None,
					offense_ratio: 1.0,
					defense_ratio: 1.0,
					model_scale: 1.0,
					ucf: Some(Ucf {
						dash_back: Some(DashBack::Ucf),
						shield_drop: Some(ShieldDrop::Ucf),
					}),
					name_tag: Some(MeleeString("".to_string())),
					netplay: Some(Netplay {
						name: MeleeString("yyyyyyyyyy".to_string()),
						code: MeleeString("YYYY＃222".to_string()),
						suid: Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string())
					})
				}
			],
			random_seed: 39656,
			bytes: Bytes(vec![
				3, 12, 0, 0, 50, 1, 142, 76, 195, 0, 0, 0, 0, 0, 0, 255, 255, 110, 0, 3, 0, 0, 1,
				224, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0,
				0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 9, 0, 4, 3, 0, 0, 0, 0, 9, 0, 120, 0, 192, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0,
				63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 9, 0, 4, 0, 0, 1, 0, 0, 9, 1, 120, 0,
				192, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0,
				21, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0, 192, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 63,
				128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 21, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0,
				192, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0,
				33, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0, 64, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63,
				128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 33, 3, 4, 0, 0, 255, 0, 0, 9, 0, 120, 0,
				64, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0, 63, 128, 0, 0,
				0, 0, 154, 232, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0,
				0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 8, 120, 120, 120,
				120, 120, 120, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 121, 121, 121, 121, 121, 121, 121, 121, 121, 121, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 88, 88, 129, 148, 49, 49, 49,
				0, 0, 0, 89, 89, 89, 89, 129, 148, 50, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97,
				97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 97, 0, 98, 98, 98, 98, 98, 98,
				98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98, 98,
				98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 1
			]),
			is_pal: Some(false),
			is_frozen_ps: Some(false),
			scene: Some(Scene { minor: 2, major: 8 }),
			language: Some(Language::English),
		}
	);
}

#[test]
fn unknown_event() {
	// shouldn't panic
	// TODO: check for warning
	game("unknown_event");
}

#[test]
fn corrupt_replay() {
	assert!(matches!(read_game(get_path("corrupt")), Err(_)));
}

#[test]
fn zelda_sheik_transformation() {
	let game = game("transform");
	assert_eq!(
		game.frames.port[1].leader.pre.state.values()[400],
		355, // Zelda Transform (Ground)
	);
}

#[test]
fn items() {
	let game = game("items");
	assert_eq!(
		game.frames.item.transpose_one(0, game.start.slippi.version),
		transpose::Item {
			id: 0,
			damage: 0,
			direction: 1.0,
			position: transpose::Position {
				x: -62.709_606,
				y: -1.493_274_9
			},
			state: 0,
			timer: 140.0,
			r#type: 99, // Peach Turnip
			velocity: transpose::Velocity { x: 0.0, y: 0.0 },
			misc: Some(transpose::ItemMisc(5, 5, 5, 5)),
			owner: Some(0),
		}
	);
	assert_eq!(
		game.frames
			.item
			.transpose_one(102, game.start.slippi.version),
		transpose::Item {
			id: 1,
			damage: 0,
			direction: -1.0,
			position: transpose::Position {
				x: 20.395_56,
				y: -1.493_274_9
			},
			state: 0,
			timer: 140.0,
			r#type: 99, // Peach Turnip
			velocity: transpose::Velocity { x: 0.0, y: 0.0 },
			misc: Some(transpose::ItemMisc(5, 0, 5, 5)),
			owner: Some(0),
		}
	);
	assert_eq!(
		game.frames
			.item
			.transpose_one(290, game.start.slippi.version),
		transpose::Item {
			id: 2,
			damage: 0,
			direction: 1.0,
			position: transpose::Position {
				x: -3.982_539_2,
				y: -1.493_274_9
			},
			state: 0,
			timer: 140.0,
			r#type: 99, // Peach Turnip
			velocity: transpose::Velocity { x: 0.0, y: 0.0 },
			misc: Some(transpose::ItemMisc(5, 0, 5, 5)),
			owner: Some(0),
		}
	);
}

fn hash(path: impl AsRef<Path>) -> u64 {
	let mut buf = Vec::new();
	let mut f = File::open(path).unwrap();
	f.read_to_end(&mut buf).unwrap();
	xxh3_64(&buf)
}

fn _round_trip(in_path: impl AsRef<Path> + Clone) {
	let game1 = read_game(in_path.clone()).unwrap();
	let out_path = "/tmp/peppi_test_round_trip.slp";
	let mut buf = File::create(out_path).unwrap();
	serde::ser::serialize(&mut buf, &game1).unwrap();
	let game2 = read_game(out_path).unwrap();

	assert_eq!(game1.start, game2.start);
	assert_eq!(game1.end, game2.end);
	assert_eq!(game1.metadata, game2.metadata);

	assert_eq!(game1.frames.id.len(), game2.frames.id.len());
	for idx in 0..game1.frames.id.len() {
		assert_eq!(
			game1.frames.transpose_one(idx, game1.start.slippi.version),
			game2.frames.transpose_one(idx, game2.start.slippi.version),
			"frame: {}",
			idx
		);
	}

	assert_eq!(hash(in_path), hash(out_path));

	fs::remove_file(out_path).unwrap();
}

#[test]
fn round_trip() {
	for entry in fs::read_dir("tests/data")
		.unwrap()
		.into_iter()
		.map(|e| e.unwrap())
		.filter(|e| match e.file_name().to_str().unwrap() {
			"unknown_event.slp" => false,
			"corrupt.slp" => false,
			_ => true,
		}) {
		println!("{:?}", entry.file_name());
		_round_trip(entry.path());
	}
}

#[test]
fn rollback() {
	let game = game("ics2");
	assert_eq!(game.frames.id.len(), 9530);
	assert_eq!(
		game.frames.id.values().clone().sliced(473, 4).as_slice(),
		[350, 351, 351, 352]
	);
	assert_eq!(
		game.frames.rollback_indexes_initial()[473..476],
		[473, 474, 476]
	);
	assert_eq!(
		game.frames.rollback_indexes_final()[473..476],
		[473, 475, 476]
	);
}