use std::{
	collections::HashMap,
	fs::{self, File},
	io::{self, Read, Result, Write},
	path::{Path, PathBuf},
};

use byteorder::ReadBytesExt;
use log::{debug, info};
use arrow2::array::MutableArray;

type BE = byteorder::BigEndian;

use crate::{
	model::{
		enums::{character, costume, stage},
		frame,
		game::{self, Game, Netplay, Player, PlayerType, MAX_PLAYERS, NUM_PORTS},
		primitives::Port,
		shift_jis::MeleeString,
		slippi,
	},
	ubjson,
};

pub(super) const PAYLOADS_EVENT_CODE: u8 = 0x35;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub(super) enum Event {
	GameStart = 0x36,
	FramePre = 0x37,
	FramePost = 0x38,
	GameEnd = 0x39,
	FrameStart = 0x3A,
	Item = 0x3B,
	FrameEnd = 0x3C,
	GeckoCodes = 0x3D,
}

struct MutableGame {
	pub start: game::Start,
	pub end: Option<game::End>,
	pub frames: frame::MutableFrame,
	pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
	pub gecko_codes: Option<game::GeckoCodes>,
}

impl From<MutableGame> for Game {
	fn from(game: MutableGame) -> Self {
		Game {
			start: game.start,
			end: game.end,
			frames: game.frames.into(),
			metadata: game.metadata,
			gecko_codes: game.gecko_codes,
		}
	}
}

fn if_more<F, T>(r: &mut &[u8], f: F) -> Result<Option<T>>
where
	F: FnOnce(&mut &[u8]) -> Result<T>,
{
	Ok(match r.is_empty() {
		true => None,
		_ => Some(f(r)?),
	})
}

/// Reads the Event Payloads event, which must come first in the raw stream
/// and tells us the sizes for all other events to follow.
/// Returns the number of bytes read by this function, plus a map of event
/// codes to payload sizes. This map uses raw event codes as keys (as opposed
/// to `Event` enum values) for forwards compatibility, as it allows us to
/// skip unknown event types.
fn payload_sizes<R: Read>(r: &mut R) -> Result<(usize, HashMap<u8, u16>)> {
	let code = r.read_u8()?;
	if code != PAYLOADS_EVENT_CODE {
		return Err(err!("expected event payloads, but got: {}", code));
	}

	// Size in bytes of the subsequent list of payload-size kv pairs.
	// Each pair is 3 bytes, so this size should be divisible by 3.
	// However the value includes this size byte itself, so it's off-by-one.
	let size = r.read_u8()?;
	if size % 3 != 1 {
		return Err(err!("invalid payload size: {}", size));
	}

	let mut sizes = HashMap::new();
	for _ in (0..size - 1).step_by(3) {
		let code = r.read_u8()?;
		let size = r.read_u16::<BE>()?;
		sizes.insert(code, size);
	}

	debug!(
		"Event payload sizes: {{{}}}",
		sizes
			.iter()
			.map(|(k, v)| format!("0x{:x}: {}", k, v))
			.collect::<Vec<_>>()
			.join(", ")
	);

	Ok((1 + size as usize, sizes)) // +1 byte for the event code
}

#[allow(clippy::too_many_arguments)]
fn player(
	port: Port,
	v0: &[u8; 36],
	is_teams: bool,
	v1_0: Option<[u8; 8]>,
	v1_3: Option<[u8; 16]>,
	v3_9_name: Option<[u8; 31]>,
	v3_9_code: Option<[u8; 10]>,
	v3_11: Option<[u8; 29]>,
) -> Result<Option<Player>> {
	let mut r = &v0[..];
	let mut unmapped = [0; 15];

	let character = character::External(r.read_u8()?);
	let r#type = game::PlayerType(r.read_u8()?);
	let stocks = r.read_u8()?;
	let costume = costume::Costume::from(r.read_u8()?, character);
	r.read_exact(&mut unmapped[0..3])?;
	let team_shade = r.read_u8()?;
	let handicap = r.read_u8()?;
	let team_color = r.read_u8()?;
	let team = {
		match is_teams {
			true => Some(game::Team {
				color: game::TeamColor(team_color),
				shade: game::TeamShade(team_shade),
			}),
			false => None,
		}
	};
	r.read_exact(&mut unmapped[3..5])?;
	let bitfield = r.read_u8()?;
	r.read_exact(&mut unmapped[5..7])?;
	let cpu_level = {
		let cpu_level = r.read_u8()?;
		match r#type {
			PlayerType::CPU => Some(cpu_level),
			_ => None,
		}
	};
	r.read_exact(&mut unmapped[7..15])?;
	let offense_ratio = r.read_f32::<BE>()?;
	let defense_ratio = r.read_f32::<BE>()?;
	let model_scale = r.read_f32::<BE>()?;
	// total bytes: 0x24

	// v1.0
	let ucf = match v1_0 {
		Some(v1_0) => {
			let mut r = &v1_0[..];
			Some(game::Ucf {
				dash_back: match r.read_u32::<BE>()? {
					0 => None,
					db => Some(game::DashBack(db)),
				},
				shield_drop: match r.read_u32::<BE>()? {
					0 => None,
					sd => Some(game::ShieldDrop(sd)),
				},
			})
		}
		_ => None,
	};

	// v1_3
	let name_tag = v1_3
		.map(|v1_3| MeleeString::try_from(v1_3.as_slice()))
		.transpose()?;

	// v3.9
	let netplay = v3_9_name
		.zip(v3_9_code)
		.map(|(name, code)| {
			let suid = v3_11
				.map(|v3_11| {
					let first_null = v3_11.iter().position(|&x| x == 0).unwrap_or(28);
					let result = std::str::from_utf8(&v3_11[0..first_null]);
					result.map(String::from).map_err(|_| err!("invalid suid"))
				})
				.transpose()?;
			Result::Ok(Netplay {
				name: MeleeString::try_from(name.as_slice())?,
				code: MeleeString::try_from(code.as_slice())?,
				suid,
			})
		})
		.transpose()?;

	Ok(match r#type {
		PlayerType::HUMAN | PlayerType::CPU | PlayerType::DEMO => Some(Player {
			port,
			character,
			r#type,
			stocks,
			costume,
			team,
			handicap,
			bitfield,
			cpu_level,
			offense_ratio,
			defense_ratio,
			model_scale,
			// v1_0
			ucf,
			// v1_3
			name_tag,
			// v3.9
			netplay,
		}),
		_ => None,
	})
}

fn player_bytes_v3_11(r: &mut &[u8]) -> Result<[u8; 29]> {
	let mut buf = [0; 29];
	r.read_exact(&mut buf)?;
	Ok(buf)
}

fn player_bytes_v3_9_name(r: &mut &[u8]) -> Result<[u8; 31]> {
	let mut buf = [0; 31];
	r.read_exact(&mut buf)?;
	Ok(buf)
}

fn player_bytes_v3_9_code(r: &mut &[u8]) -> Result<[u8; 10]> {
	let mut buf = [0; 10];
	r.read_exact(&mut buf)?;
	Ok(buf)
}

fn player_bytes_v1_3(r: &mut &[u8]) -> Result<[u8; 16]> {
	let mut buf = [0; 16];
	r.read_exact(&mut buf)?;
	Ok(buf)
}

fn player_bytes_v1_0(r: &mut &[u8]) -> Result<[u8; 8]> {
	let mut buf = [0; 8];
	r.read_exact(&mut buf)?;
	Ok(buf)
}

pub(crate) fn game_start(r: &mut &[u8]) -> Result<game::Start> {
	let raw_bytes = r.to_vec();
	let slippi = slippi::Slippi {
		version: slippi::Version(r.read_u8()?, r.read_u8()?, r.read_u8()?),
	};
	r.read_u8()?; // unused (build number)

	let mut unmapped = [0; 73];
	let bitfield = {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		buf
	};
	r.read_exact(&mut unmapped[0..2])?;
	let is_raining_bombs = r.read_u8()? != 0;
	r.read_exact(&mut unmapped[2..3])?;
	let is_teams = r.read_u8()? != 0;
	r.read_exact(&mut unmapped[3..5])?;
	let item_spawn_frequency = r.read_i8()?;
	let self_destruct_score = r.read_i8()?;
	r.read_exact(&mut unmapped[5..6])?;
	let stage = stage::Stage(r.read_u16::<BE>()?);
	let timer = r.read_u32::<BE>()?;
	r.read_exact(&mut unmapped[6..21])?;
	let item_spawn_bitfield = {
		let mut buf = [0; 5];
		r.read_exact(&mut buf)?;
		buf
	};
	r.read_exact(&mut unmapped[21..29])?;
	let damage_ratio = r.read_f32::<BE>()?;
	r.read_exact(&mut unmapped[29..73])?;
	// @0x65
	let mut players_v0 = [[0; 36]; MAX_PLAYERS];
	for p in &mut players_v0 {
		r.read_exact(p)?;
	}
	// @0x13d
	let random_seed = r.read_u32::<BE>()?;

	let players_v1_0 = match r.is_empty() {
		true => [None, None, None, None],
		_ => [
			Some(player_bytes_v1_0(r)?),
			Some(player_bytes_v1_0(r)?),
			Some(player_bytes_v1_0(r)?),
			Some(player_bytes_v1_0(r)?),
		],
	};

	let players_v1_3 = match r.is_empty() {
		true => [None, None, None, None],
		_ => [
			Some(player_bytes_v1_3(r)?),
			Some(player_bytes_v1_3(r)?),
			Some(player_bytes_v1_3(r)?),
			Some(player_bytes_v1_3(r)?),
		],
	};

	let is_pal = if_more(r, |r| Ok(r.read_u8()? != 0))?;
	let is_frozen_ps = if_more(r, |r| Ok(r.read_u8()? != 0))?;
	let scene = if_more(r, |r| {
		Ok(game::Scene {
			minor: r.read_u8()?,
			major: r.read_u8()?,
		})
	})?;

	let players_v3_9 = match r.is_empty() {
		true => ([None, None, None, None], [None, None, None, None]),
		_ => (
			[
				Some(player_bytes_v3_9_name(r)?),
				Some(player_bytes_v3_9_name(r)?),
				Some(player_bytes_v3_9_name(r)?),
				Some(player_bytes_v3_9_name(r)?),
			],
			[
				Some(player_bytes_v3_9_code(r)?),
				Some(player_bytes_v3_9_code(r)?),
				Some(player_bytes_v3_9_code(r)?),
				Some(player_bytes_v3_9_code(r)?),
			],
		),
	};

	let players_v3_11 = match r.is_empty() {
		true => [None, None, None, None],
		_ => [
			Some(player_bytes_v3_11(r)?),
			Some(player_bytes_v3_11(r)?),
			Some(player_bytes_v3_11(r)?),
			Some(player_bytes_v3_11(r)?),
		],
	};

	let mut players = Vec::<Player>::new();
	for n in 0..NUM_PORTS {
		if let Some(p) = player(
			Port::try_from(n as u8).unwrap(),
			&players_v0[n],
			is_teams,
			players_v1_0[n],
			players_v1_3[n],
			players_v3_9.0[n],
			players_v3_9.1[n],
			players_v3_11[n],
		)? {
			players.push(p);
		}
	}

	let language = if_more(r, |r| Ok(game::Language(r.read_u8()?)))?;

	Ok(game::Start {
		slippi,
		bitfield,
		is_raining_bombs,
		is_teams,
		item_spawn_frequency,
		self_destruct_score,
		stage,
		timer,
		item_spawn_bitfield,
		damage_ratio,
		players,
		random_seed,
		raw_bytes,
		// v1.5
		is_pal,
		// v2.0
		is_frozen_ps,
		// v3.7
		scene,
		// v3.12
		language,
	})
}

pub(crate) fn game_end(r: &mut &[u8]) -> Result<game::End> {
	let raw_bytes = r.to_vec();
	Ok(game::End {
		method: game::EndMethod(r.read_u8()?),
		raw_bytes: raw_bytes,
		// v2.0
		lras_initiator: if_more(r, |r| Ok(Port::try_from(r.read_u8()?).ok()))?,
	})
}

fn expect_bytes<R: Read>(r: &mut R, expected: &[u8]) -> Result<()> {
	let mut actual = vec![0; expected.len()];
	r.read_exact(&mut actual)?;
	if expected == actual.as_slice() {
		Ok(())
	} else {
		Err(err!("expected: {:?}, got: {:?}", expected, actual))
	}
}

#[derive(Default)]
struct SplitAccumulator {
	raw: Vec<u8>,
	actual_size: u32,
}

fn handle_splitter_event(buf: &[u8], accumulator: &mut SplitAccumulator) -> Result<Option<u8>> {
	assert_eq!(buf.len(), 516);
	let actual_size = (&buf[512..514]).read_u16::<BE>()?;
	assert!(actual_size <= 512);
	let wrapped_event = buf[514];
	let is_final = buf[515] != 0;

	// bytes beyond `actual_size` are meaningless,
	// but save them anyway for lossless round-tripping
	accumulator.raw.extend_from_slice(&buf[0..512]);
	accumulator.actual_size += actual_size as u32;

	Ok(match is_final {
		true => Some(wrapped_event),
		_ => None,
	})
}

fn debug_write_event<P: AsRef<Path>>(buf: &[u8], code: u8, event_counts: &mut HashMap<u8, usize>, debug_dir: P) -> Result<()> {
	// write the event's raw data to "{debug_dir}/{code}/{count}",
	// where `count` is how many of that event we've seen already
	let code_dir = debug_dir.as_ref().join(format!("{}", code));
	fs::create_dir_all(&code_dir)?;
	let count = event_counts.get(&code).unwrap_or(&0);
	let mut f = File::create(code_dir.join(format!("{}", count)))?;
	f.write_all(buf)?;
	event_counts.insert(code, count + 1);
	Ok(())
}

/// Parses a single event from the raw stream. If the event is one of the
/// supported `Event` types, calls the corresponding `Handler` callback with
/// the parsed event.
///
/// Returns the number of bytes read by this function.
fn pre_start_event<R: Read, P: AsRef<Path>>(
	mut r: R,
	payload_sizes: &HashMap<u8, u16>,
	event_counts: &mut HashMap<u8, usize>,
	debug_dir: Option<P>,
) -> Result<(usize, Option<game::Start>)> {
	let code = r.read_u8()?;
	debug!("Event: {:#x}", code);

	let size = *payload_sizes
		.get(&code)
		.ok_or_else(|| err!("unknown event: {}", code))? as usize;
	let mut buf = vec![0; size];
	r.read_exact(&mut buf)?;

	if let Some(d) = debug_dir {
		debug_write_event(&buf, code, event_counts, d)?;
	}

	let event = Event::try_from(code).ok();
	let mut start = None;
	if let Some(event) = event {
		use Event::*;
		match event {
			GameStart => start = Some(game_start(&mut &*buf)?),
			_ => return Err(err!("Invalid event before start: {}", code)),
		};
	}

	Ok((size + 1, start)) // +1 byte for the event code
}

fn post_start_event<R: Read, P: AsRef<Path>>(
	mut r: R,
	payload_sizes: &HashMap<u8, u16>,
	splitter_accumulator: &mut SplitAccumulator,
	event_counts: &mut HashMap<u8, usize>,
	debug_dir: Option<P>,
	game: &mut MutableGame,
) -> Result<(usize, Option<game::End>)> {
	let mut code = r.read_u8()?;
	debug!("Event: {:#x}", code);

	let size = *payload_sizes
		.get(&code)
		.ok_or_else(|| err!("unknown event: {}", code))? as usize;
	let mut buf = vec![0; size];
	r.read_exact(&mut buf)?;

	if code == 0x10 {
		// message splitter
		if let Some(wrapped_event) = handle_splitter_event(&buf, splitter_accumulator)? {
			code = wrapped_event;
			buf.clear();
			buf.append(&mut splitter_accumulator.raw);
		}
	};

	if let Some(d) = debug_dir {
		debug_write_event(&buf, code, event_counts, d)?;
	}

	let event = Event::try_from(code).ok();
	let mut end = None;
	if let Some(event) = event {
		use Event::*;
		match event {
			GeckoCodes => game.gecko_codes = Some(game::GeckoCodes {
				bytes: buf.to_vec(),
				actual_size: splitter_accumulator.actual_size,
			}),
			GameStart => return Err(err!("Duplicate start event")),
			GameEnd => end = Some(game_end(&mut &*buf)?),
			FrameStart => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				game.frames.id.push(Some(id));
				game.frames.start.read_push(r, game.start.slippi.version)?;
			},
			FramePre => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				let port = r.read_u8()?;
				let is_follower = r.read_u8()? != 0;
				assert_eq!(id, *game.frames.id.values().last().unwrap());
				match is_follower {
					true => game.frames.port[port as usize].follower.as_mut().unwrap().pre.read_push(r, game.start.slippi.version)?,
					_ => game.frames.port[port as usize].leader.pre.read_push(r, game.start.slippi.version)?,
				};
			},
			FramePost => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				let port = r.read_u8()?;
				let is_follower = r.read_u8()? != 0;
				assert_eq!(id, *game.frames.id.values().last().unwrap());
				match is_follower {
					true => game.frames.port[port as usize].follower.as_mut().unwrap().post.read_push(r, game.start.slippi.version)?,
					_ => game.frames.port[port as usize].leader.post.read_push(r, game.start.slippi.version)?,
				};
			},
			FrameEnd => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				assert_eq!(id, *game.frames.id.values().last().unwrap());
				let old_len = *game.frames.item_offset.last();
				let new_len: i32 = game.frames.item.r#type.len().try_into().unwrap();
				game.frames.item_offset.try_push(new_len.checked_sub(old_len).unwrap()).unwrap();
				game.frames.end.read_push(r, game.start.slippi.version)?;
			},
			Item => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				assert_eq!(id, *game.frames.id.values().last().unwrap());
				game.frames.item.read_push(r, game.start.slippi.version)?;
			},
		};
	}

	Ok((size + 1, end)) // +1 byte for the event code
}

/// Options for parsing replays.
#[derive(Clone, Debug, Default)]
pub struct Opts {
	/// Skip all frame data when parsing a replay for speed
	/// (when you only need start/end/metadata).
	pub skip_frames: bool,
	pub debug_dir: Option<PathBuf>,
}

/// Parses a Slippi replay from `r`.
pub fn deserialize<R: Read>(
	mut r: &mut R,
	opts: Option<&Opts>,
) -> Result<Game> {
	// For speed, assume the `raw` element comes first and handle it manually.
	// The official JS parser does this too, so it should be reliable.
	expect_bytes(&mut r, &crate::SLIPPI_FILE_SIGNATURE)?;

	let raw_len = r.read_u32::<BE>()? as usize;
	info!("Raw length: {} bytes", raw_len);
	let (mut bytes_read, payload_sizes) = payload_sizes(&mut r)?;
	let skip_frames = opts.map(|o| o.skip_frames).unwrap_or(false);

	let debug_dir = opts.map(|o| o.debug_dir.as_ref()).unwrap_or(None);
	// track how many of each event we've seen so we know where to put the debug output
	let mut event_counts = HashMap::<u8, usize>::new();
	let mut start: Option<game::Start> = None;

	// `raw_len` will be 0 for an in-progress replay
	while (raw_len == 0 || bytes_read < raw_len) && start.is_none() {
		let (bytes, _start) = pre_start_event(
			r.by_ref(),
			&payload_sizes,
			&mut event_counts,
			debug_dir,
		)?;
		bytes_read += bytes;
		start = _start;
	}

	let mut game = {
		let start = start.unwrap();
		let ports: Vec<_> = start.players.iter().map(|p|
			frame::PortOccupancy {
				port: p.port,
				follower: p.character == character::External::ICE_CLIMBERS
			}
		).collect();
		let version = start.slippi.version;
		MutableGame {
			start: start,
			end: None,
			frames: frame::MutableFrame::with_capacity(1024, version, &ports),
			metadata: None,
			gecko_codes: None,
		}
	};

	if skip_frames {
		// Skip to GameEnd, which we assume is the last event in the stream!
		let end_offset: usize = payload_sizes[&(Event::GameEnd as u8)] as usize + 1;
		if raw_len == 0 || raw_len - bytes_read < end_offset {
			return Err(err!(
				"Cannot skip to game end. Replay in-progress or corrupted."
			));
		}
		let skip = raw_len - bytes_read - end_offset;
		info!("Jumping to GameEnd (skipping {} bytes)", skip);
		// In theory we should seek() if `r` is Seekable, but it's not much
		// faster and is very awkward to implement without specialization.
		io::copy(&mut r.by_ref().take(skip as u64), &mut io::sink())?;
		bytes_read += skip;
	}

	let mut end = None;
	let mut splitter_accumulator = Default::default();
	while (raw_len == 0 || bytes_read < raw_len) && end.is_none() {
		let (bytes, _end) = post_start_event(
			r.by_ref(),
			&payload_sizes,
			&mut splitter_accumulator,
			&mut event_counts,
			debug_dir,
			&mut game,
		)?;
		bytes_read += bytes;
		end = _end;
	}

	game.end = end;

	info!("frames: {}", game.frames.port[0].leader.pre.state.len());

	if raw_len != 0 && bytes_read != raw_len {
		return Err(err!(
			"failed to consume expected number of bytes: {}, {}",
			raw_len,
			bytes_read
		));
	}

	expect_bytes(
		&mut r,
		// `metadata` key & type ("U\x08metadata{")
		&[
			0x55, 0x08, 0x6d, 0x65, 0x74, 0x61, 0x64, 0x61, 0x74, 0x61, 0x7b,
		],
	)?;

	// Since we already read the opening "{" from the `metadata` value,
	// we know it's a map. `parse_map` will consume the corresponding "}".
	let metadata = ubjson::de::to_map(&mut r)?;
	info!("Raw metadata: {}", serde_json::to_string(&metadata)?);
	game.metadata = Some(metadata);

	expect_bytes(&mut r, &[0x7d])?; // top-level closing brace ("}")

	Ok(game.into())
}
