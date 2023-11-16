use std::{
	collections::HashMap,
	error,
	fs::{self, File},
	io::{self, Read, Result, Write},
	path::PathBuf,
};

use arrow2::array::MutableArray;
use byteorder::ReadBytesExt;
use log::{debug, info};

type BE = byteorder::BigEndian;

use crate::{
	model::{
		frame::{self, mutable::Frame as MutableFrame, transpose},
		game::{self, immutable::Game, Netplay, Player, PlayerType, Port, NUM_PORTS},
		shift_jis::MeleeString,
		slippi,
	},
	ubjson,
};

pub(crate) const PAYLOADS_EVENT_CODE: u8 = 0x35;
const MAX_PLAYERS: usize = 6;
const ICE_CLIMBERS: u8 = 14;

#[derive(Clone, Debug)]
pub struct Debug {
	pub dir: PathBuf,
}

/// Options for parsing replays.
#[derive(Clone, Debug, Default)]
pub struct Opts {
	/// Skip all frame data when parsing a replay for speed
	/// (when you only need start/end/metadata).
	pub skip_frames: bool,
	pub debug: Option<Debug>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum Event {
	GameStart = 0x36,
	FramePre = 0x37,
	FramePost = 0x38,
	GameEnd = 0x39,
	FrameStart = 0x3A,
	Item = 0x3B,
	FrameEnd = 0x3C,
	GeckoCodes = 0x3D,
}

#[derive(Debug, Default)]
struct SplitAccumulator {
	raw: Vec<u8>,
	actual_size: u32,
}

pub struct PartialGame {
	pub start: game::Start,
	pub end: Option<game::End>,
	pub frames: MutableFrame,
	pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
	pub gecko_codes: Option<game::GeckoCodes>,
}

pub struct ParseState {
	payload_sizes: HashMap<u8, u16>,
	bytes_read: usize,
	event_counts: HashMap<u8, usize>,
	split_accumulator: SplitAccumulator,
	port_indexes: [usize; 4],
	game: PartialGame,
}

impl game::Game for ParseState {
	fn start(&self) -> &game::Start {
		&self.game.start
	}

	fn end(&self) -> &Option<game::End> {
		&self.game.end
	}

	fn metadata(&self) -> &Option<serde_json::Map<String, serde_json::Value>> {
		&self.game.metadata
	}

	fn gecko_codes(&self) -> &Option<game::GeckoCodes> {
		&self.game.gecko_codes
	}

	fn len(&self) -> usize {
		self.game.frames.id.len()
	}

	fn frame(&self, idx: usize) -> transpose::Frame {
		self.game.frames.transpose_one(idx, self.game.start.slippi.version)
	}
}

impl ParseState {
	pub fn frames(&self) -> &MutableFrame {
		&self.game.frames
	}

	pub fn bytes_read(&self) -> usize {
		self.bytes_read
	}

	fn last_id(&self) -> Option<i32> {
		self.game.frames.id.values().last().map(|id| *id)
	}

	fn frame_open(&mut self, id: i32) {
		let len = self.game.frames.id.len();
		self.game.frames.id.push(Some(id));
		//FIXME: do this for leaders too? (multiplayer)
		for p in &mut self.game.frames.port {
			if let Some(f) = &mut p.follower {
				while f.pre.random_seed.len() < len {
					f.push_none(self.game.start.slippi.version);
				}
			}
		}
	}

	fn frame_close(&mut self) {
		let len = self.game.frames.id.len();
		//FIXME: do this for leaders too? (multiplayer)
		for p in &mut self.game.frames.port {
			if let Some(f) = &mut p.follower {
				while f.pre.random_seed.len() < len {
					f.push_none(self.game.start.slippi.version);
				}
			}
		}
	}
}

impl From<PartialGame> for Game {
	fn from(game: PartialGame) -> Self {
		Game {
			start: game.start,
			end: game.end,
			frames: game.frames.into(),
			metadata: game.metadata,
			gecko_codes: game.gecko_codes,
		}
	}
}

impl From<ParseState> for Game {
	fn from(state: ParseState) -> Self {
		Self::from(state.game)
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

fn invalid_data<E: Into<Box<dyn error::Error + Send + Sync>>>(err: E) -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, err)
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

	let character = r.read_u8()?;
	let r#type = PlayerType::try_from(r.read_u8()?).ok();
	let stocks = r.read_u8()?;
	let costume = r.read_u8()?;
	r.read_exact(&mut unmapped[0..3])?;
	let team_shade = r.read_u8()?;
	let handicap = r.read_u8()?;
	let team_color = r.read_u8()?;
	let team = {
		match is_teams {
			true => Some(game::Team {
				color: team_color,
				shade: team_shade,
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
			Some(PlayerType::Cpu) => Some(cpu_level),
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
					x => Some(game::DashBack::try_from(x).map_err(invalid_data)?),
				},
				shield_drop: match r.read_u32::<BE>()? {
					0 => None,
					x => Some(game::ShieldDrop::try_from(x).map_err(invalid_data)?),
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
					result.map(String::from).map_err(invalid_data)
				})
				.transpose()?;
			Result::Ok(Netplay {
				name: MeleeString::try_from(name.as_slice())?,
				code: MeleeString::try_from(code.as_slice())?,
				suid,
			})
		})
		.transpose()?;

	Ok(r#type.map(|r#type| Player {
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
	}))
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
	let bytes = game::Bytes(r.to_vec());
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
	let stage = r.read_u16::<BE>()?;
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
		let nu = n as usize;
		if let Some(p) = player(
			Port::try_from(n).map_err(invalid_data)?,
			&players_v0[nu],
			is_teams,
			players_v1_0[nu],
			players_v1_3[nu],
			players_v3_9.0[nu],
			players_v3_9.1[nu],
			players_v3_11[nu],
		)? {
			players.push(p);
		}
	}

	let language = if_more(r, |r| {
		Ok(game::Language::try_from(r.read_u8()?).map_err(invalid_data)?)
	})?;

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
		bytes,
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
	let bytes = game::Bytes(r.to_vec());
	Ok(game::End {
		method: game::EndMethod::try_from(r.read_u8()?).map_err(invalid_data)?,
		bytes: bytes,
		// v2.0
		lras_initiator: if_more(r, |r| {
			Ok(match r.read_u8()? {
				255 => None,
				x => Some(Port::try_from(x).map_err(invalid_data)?),
			})
		})?,
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

fn debug_write_event(
	buf: &[u8],
	code: u8,
	state: Option<&ParseState>,
	debug: &Debug,
) -> Result<()> {
	// write the event's raw data to "{debug.dir}/{code}/{count}",
	// where `count` is how many of that event we've seen already
	let code_dir = debug.dir.join(format!("{}", code));
	fs::create_dir_all(&code_dir)?;
	let count = state.and_then(|s| s.event_counts.get(&code)).unwrap_or(&0);
	let mut f = File::create(code_dir.join(format!("{}", count)))?;
	f.write_all(buf)?;
	Ok(())
}

/// Parses an Event Payloads event from `r`, which must come first in the raw
/// stream and tells us the sizes for all other events to follow.
///
/// Returns the number of bytes read, and a map of event codes to payload sizes.
/// This map uses raw event codes as keys (as opposed to `Event` enum values)
/// for forwards compatibility, to allow skipping unknown events.
fn parse_payloads<R: Read>(mut r: R, opts: Option<&Opts>) -> Result<(usize, HashMap<u8, u16>)> {
	let code = r.read_u8()?;
	if code != PAYLOADS_EVENT_CODE {
		return Err(err!("expected event payloads, but got: {:#02x}", code));
	}

	// Size in bytes of the subsequent list of payload-size kv pairs.
	// Each pair is 3 bytes, so this size should be divisible by 3.
	// However the value includes this size byte itself, so it's off-by-one.
	let size = r.read_u8()?;
	if size % 3 != 1 {
		return Err(err!("invalid payload size: {}", size));
	}

	let mut buf = vec![0; (size - 1) as usize];
	r.read_exact(&mut buf)?;
	let buf = &mut &buf[..];

	if let Some(ref d) = opts.as_ref().and_then(|o| o.debug.as_ref()) {
		debug_write_event(&buf, code, None, d)?;
	}

	let mut sizes = HashMap::new();
	for _ in (0..size - 1).step_by(3) {
		let code = buf.read_u8()?;
		let size = buf.read_u16::<BE>()?;
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

/// Parses a Game Start event from `r`, which must come immediately after the
/// Event Payloads.
///
/// Returns the number of bytes read, and a parsed `game::Start` event
/// (or Err if the event wasn't a Game Start).
fn parse_game_start<R: Read>(
	mut r: R,
	payload_sizes: &HashMap<u8, u16>,
	opts: Option<&Opts>,
) -> Result<(usize, game::Start)> {
	let code = r.read_u8()?;
	debug!("Event: {:#x}", code);

	let size = *payload_sizes
		.get(&code)
		.ok_or_else(|| err!("unknown event: {:#02x}", code))? as usize;
	let mut buf = vec![0; size];
	r.read_exact(&mut buf)?;

	if let Some(ref d) = opts.as_ref().and_then(|o| o.debug.as_ref()) {
		debug_write_event(&buf, code, None, d)?;
	}

	match Event::try_from(code) {
		// +1 byte for the event code
		Ok(Event::GameStart) => Ok((size + 1, game_start(&mut &*buf)?)),
		_ => Err(err!("Invalid event before start: {:#02x}", code)),
	}
}

pub fn parse_header<R: Read>(mut r: R) -> Result<u32> {
	// For speed, assume the `raw` element comes first and handle it manually.
	// The official JS parser does this too, so it should be reliable.
	expect_bytes(&mut r, &crate::SLIPPI_FILE_SIGNATURE)?;
	// `raw` content size in bytes
	r.read_u32::<BE>()
}

pub fn parse_start<R: Read>(mut r: R, opts: Option<&Opts>) -> Result<ParseState> {
	let (bytes1, payload_sizes) = parse_payloads(&mut r, opts)?;
	let (bytes2, start) = parse_game_start(&mut r, &payload_sizes, opts)?;

	let ports: Vec<_> = start
		.players
		.iter()
		.map(|p| frame::PortOccupancy {
			port: p.port,
			follower: p.character == ICE_CLIMBERS,
		})
		.collect();
	let version = start.slippi.version;
	let game = PartialGame {
		start: start.clone(),
		end: None,
		frames: MutableFrame::with_capacity(1024, version, &ports),
		metadata: None,
		gecko_codes: None,
	};

	let port_indexes = {
		let mut result = [0, 0, 0, 0];
		for (i, p) in ports.into_iter().enumerate() {
			result[p.port as usize] = i;
		}
		result
	};

	Ok(ParseState {
		payload_sizes: payload_sizes,
		bytes_read: bytes1 + bytes2,
		event_counts: HashMap::new(), // FIXME: +1 for payloads & start
		split_accumulator: Default::default(),
		game: game,
		port_indexes: port_indexes,
	})
}

/// Parses a single event from `r`.
///
/// Returns the number of bytes read, and a `game::End` if the event was a
/// Game End (which signals the end of the event stream).
pub fn parse_event<R: Read>(mut r: R, state: &mut ParseState, opts: Option<&Opts>) -> Result<u8> {
	let mut code = r.read_u8()?;
	debug!("Event: {:#x}", code);

	let size = *state
		.payload_sizes
		.get(&code)
		.ok_or_else(|| err!("unknown event: {:#02x}", code))? as usize;
	let mut buf = vec![0; size];
	r.read_exact(&mut buf)?;

	if code == 0x10 {
		// message splitter
		if let Some(wrapped_event) = handle_splitter_event(&buf, &mut state.split_accumulator)? {
			code = wrapped_event;
			buf.clear();
			buf.append(&mut state.split_accumulator.raw);
		}
	};

	if let Some(ref d) = opts.as_ref().and_then(|o| o.debug.as_ref()) {
		debug_write_event(&buf, code, Some(state), d)?;
	}

	let event = Event::try_from(code).ok();
	if let Some(event) = event {
		use Event::*;
		match event {
			GeckoCodes => {
				state.game.gecko_codes = Some(game::GeckoCodes {
					bytes: buf.to_vec(),
					actual_size: state.split_accumulator.actual_size,
				})
			}
			GameStart => return Err(err!("Duplicate start event")),
			GameEnd => state.game.end = Some(game_end(&mut &*buf)?),
			FrameStart => {
				// no FrameEnd events before v3.0, so simulate it
				if !state.game.start.slippi.version.gte(3, 0) {
					state.frame_close();
				}
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				state.frame_open(id);
				state
					.game
					.frames
					.start
					.read_push(r, state.game.start.slippi.version)?;
			}
			FramePre => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				let port = r.read_u8()?;
				let is_follower = r.read_u8()? != 0;
				if state.game.start.slippi.version.gte(2, 2) {
					assert_eq!(id, state.last_id().unwrap());
				} else {
					// no Frame Start events before v2.2, but also no rollbacks
					let last_id = state.last_id().unwrap_or(frame::FIRST_INDEX - 1);
					if last_id + 1 == id {
						state.frame_open(id);
					} else {
						assert_eq!(id, last_id);
					}
				}
				match is_follower {
					true => state.game.frames.port[state.port_indexes[port as usize]]
						.follower
						.as_mut()
						.unwrap()
						.pre
						.read_push(r, state.game.start.slippi.version)?,
					_ => state.game.frames.port[state.port_indexes[port as usize]]
						.leader
						.pre
						.read_push(r, state.game.start.slippi.version)?,
				};
			}
			FramePost => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				let port = r.read_u8()?;
				let is_follower = r.read_u8()? != 0;
				assert_eq!(id, state.last_id().unwrap());
				match is_follower {
					true => state.game.frames.port[state.port_indexes[port as usize]]
						.follower
						.as_mut()
						.unwrap()
						.post
						.read_push(r, state.game.start.slippi.version)?,
					_ => state.game.frames.port[state.port_indexes[port as usize]]
						.leader
						.post
						.read_push(r, state.game.start.slippi.version)?,
				};
			}
			FrameEnd => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				assert_eq!(id, state.last_id().unwrap());
				let old_len = *state.game.frames.item_offset.last();
				let new_len: i32 = state.game.frames.item.r#type.len().try_into().unwrap();
				state
					.game
					.frames
					.item_offset
					.try_push(new_len.checked_sub(old_len).unwrap())
					.unwrap();
				state
					.game
					.frames
					.end
					.read_push(r, state.game.start.slippi.version)?;
				state.frame_close();
			}
			Item => {
				let r = &mut &*buf;
				let id = r.read_i32::<BE>()?;
				assert_eq!(id, state.last_id().unwrap());
				state
					.game
					.frames
					.item
					.read_push(r, state.game.start.slippi.version)?;
			}
		};
	}

	state.bytes_read += size + 1; // +1 byte for the event code
	Ok(code)
}

/// Parses a Slippi replay from `r`.
pub fn deserialize<R: Read>(mut r: &mut R, opts: Option<&Opts>) -> Result<Game> {
	let raw_len = parse_header(&mut r)? as usize;
	info!("Raw length: {} bytes", raw_len);

	let mut state = parse_start(&mut r, opts)?;

	if opts.map(|o| o.skip_frames).unwrap_or(false) {
		// Skip to GameEnd, which we assume is the last event in the stream!
		let end_offset: usize = state.payload_sizes[&(Event::GameEnd as u8)] as usize + 1;
		if raw_len == 0 || raw_len - state.bytes_read < end_offset {
			return Err(err!(
				"Cannot skip to game end. Replay in-progress or corrupted."
			));
		}
		let skip = raw_len - state.bytes_read - end_offset;
		info!("Jumping to GameEnd (skipping {} bytes)", skip);
		// In theory we should seek() if `r` is Seekable, but it's not much
		// faster and is very awkward to implement without specialization.
		io::copy(&mut r.by_ref().take(skip as u64), &mut io::sink())?;
		state.bytes_read += skip;
	}

	// `raw_len` will be 0 for an in-progress replay
	while raw_len == 0 || state.bytes_read < raw_len {
		if parse_event(r.by_ref(), &mut state, opts)? == Event::GameEnd as u8 {
			break;
		}
	}

	// FrameEnd doesn't exist until v3.0, so we simulate it in FrameStart/FramePre.
	// But that means there can be a "dangling" frame that we need to close here.
	if !state.game.start.slippi.version.gte(3, 0) {
		state.frame_close();
	}

	info!("frames: {}", state.game.frames.id.len());

	if raw_len != 0 && state.bytes_read != raw_len {
		return Err(err!(
			"failed to consume expected number of bytes: {}, {}",
			raw_len,
			state.bytes_read
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
	state.game.metadata = Some(metadata);

	expect_bytes(&mut r, &[0x7d])?; // top-level closing brace ("}")

	Ok(state.game.into())
}