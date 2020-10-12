use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::Result;

use chrono::{DateTime, Utc};
use log::warn;
use serde::Serialize;
use serde::ser::SerializeMap;

use super::character;
use super::game::{FIRST_FRAME_INDEX, Port};
use super::ubjson::Object;

#[derive(Debug, PartialEq, Serialize)]
pub struct Metadata {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub date: Option<DateTime<Utc>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub duration: Option<usize>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub platform: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub console: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub players: Option<Vec<Player>>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Netplay {
	pub code: String,
	pub name: String,
}

// needed for JMESPath
pub fn serialize_characters<S>(characters: &Option<HashMap<character::Internal, usize>>, serializer: S) -> std::result::Result<S::Ok, S::Error> where S: serde::Serializer {
	match characters {
		Some(characters) => {
			let mut map = serializer.serialize_map(Some(characters.len()))?;
			for (k, v) in characters {
				map.serialize_entry(&format!("{:?}", k), v)?;
			}
			map.end()
		},
		_ => serializer.serialize_none(),
	}
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Player {
	pub port: Port,
	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(serialize_with = "serialize_characters")]
	pub characters: Option<HashMap<character::Internal, usize>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub netplay: Option<Netplay>,
}

fn date(json: &HashMap<String, Object>) -> Result<Option<DateTime<Utc>>> {
	let date_too_short = "2000-01-01T00:00:00".parse::<DateTime<Utc>>();
	match json.get("startAt") {
		None => Ok(None),
		Some(Object::Str(start_at)) => match start_at.parse::<DateTime<Utc>>() {
			Ok(start_at) => Ok(Some(start_at)),
			e if e == date_too_short =>
				format!("{}Z", start_at).parse::<DateTime<Utc>>()
					.map(Some)
					.map_err(|e| err!("metadata.startAt: parse error: {:?}, {:?}", e, start_at)),
			Err(e) => Err(err!("metadata.startAt: parse error: {:?}, {:?}", e, start_at)),
		},
		start_at => Err(err!("metadata.startAt: expected str, but got: {:?}", start_at)),
	}
}

fn duration(json: &HashMap<String, Object>) -> Result<Option<usize>> {
	match json.get("lastFrame") {
		None => Ok(None),
		Some(Object::Int(last_frame)) => match usize::try_from(*last_frame - FIRST_FRAME_INDEX as i64 + 1) {
			Ok(duration) => Ok(Some(duration)),
			Err(e) => Err(err!("metadata.lastFrame: value out of range: {:?}, {:?}", last_frame, e)),
		},
		last_frame => Err(err!("metadata.lastFrame: expected int, but got: {:?}", last_frame)),
	}
}

fn platform(json: &HashMap<String, Object>) -> Result<Option<String>> {
	match json.get("playedOn") {
		None => Ok(None),
		Some(Object::Str(played_on)) => Ok(Some(played_on.clone())),
		played_on => Err(err!("metadata.playedOn: expected str, but got: {:?}", played_on)),
	}
}

fn parse_characters(characters: &HashMap<String, Object>) -> Result<HashMap<character::Internal, usize>> {
	characters.iter().map(|(k, v)| {
		let k = k.parse::<u8>().map_err(|e| err!("metadata.players.N.characters: invalid character: {:?}, {:?}", k, e))?;
		match v {
			Object::Int(v) => Ok((
				character::Internal(k),
				usize::try_from(*v).map_err(|e| err!("metadata.players.N.characters.{}: invalid duration: {:?}, {:?}", k, v, e))?,
			)),
			v => Err(err!("metadata.players.N.characters.{}: expected int, but got: {:?}", k, v).into()),
		}
	}).collect()
}

fn metadata_player(port: Port, player: &HashMap<String, Object>) -> Result<Player> {
	Ok(Player {
		port: port,
		characters: match player.get("characters") {
			Some(Object::Map(characters)) => match parse_characters(&characters) {
				Ok(characters) => Some(characters),
				Err(e) => Err(err!("metadata.players.N.characters: parse error: {:?}, {:?}", e, characters))?,
			},
			characters => Err(err!("metadata.players.N.characters: expected map, but got: {:?}", characters))?,
		},
		netplay: match player.get("names") {
			None => None,
			Some(Object::Map(names)) => match names.get("code") {
				None => None,
				Some(Object::Str(code)) => match names.get("netplay") {
					None => { warn!("ignoring netplay name without code"); None },
					Some(Object::Str(name)) => Some(Netplay {
						code: code.clone(),
						name: name.clone(),
					}),
					name => Err(err!("metadata.players.N.names.netplay: expected str, but got: {:?}", name))?,
				},
				code => Err(err!("metadata.players.N.names.code: expected str, but got: {:?}", code))?,
			},
			names => Err(err!("metadata.players.N.names: expected map, but got: {:?}", names))?,
		},
	})
}

fn players(json: &HashMap<String, Object>) -> Result<Option<Vec<Player>>> {
	match json.get("players") {
		None => Ok(None),
		Some(Object::Map(players)) => {
			let mut result = Vec::<Player>::new();
			let mut players: Vec<_> = players.iter().collect();
			players.sort_by_key(|(k, _)| k.parse::<usize>().unwrap_or(0));
			for (port, player) in players {
				match port.parse::<u8>() {
					Ok(port) => match Port::try_from(port) {
						Ok(port) => match player {
							Object::Map(player) => result.push(metadata_player(port, player)?),
							player => Err(err!("metadata.players.{:?}: expected map, but got: {:?}", port, player))?,
						},
						Err(e) => Err(err!("metadata.players: invalid port: {}, {:?}", port, e))?,
					},
					Err(e) => Err(err!("metadata.players: invalid port: {}, {:?}", port, e))?,
				};
			}
			match result.len() {
				0 => Ok(None),
				_ => Ok(Some(result)),
			}
		},
		players => Err(err!("metadata.players: expected map, but got: {:?}", players))?,
	}
}

fn console(json: &HashMap<String, Object>) -> Result<Option<String>> {
	match json.get("consoleNick") {
		None => Ok(None),
		Some(Object::Str(console_nick)) => Ok(Some(console_nick.clone())),
		console_nick => Err(err!("metadata.consoleNick: expected str, but got: {:?}", console_nick)),
	}
}

pub fn parse(json: &HashMap<String, Object>) -> Result<Metadata> {
	Ok(Metadata {
		date: date(json)?,
		duration: duration(json)?,
		platform: platform(json)?,
		players: players(json)?,
		console: console(json)?,
	})
}
