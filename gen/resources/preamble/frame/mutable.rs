#![allow(unused_parens)]
#![allow(unused_variables)]
#![allow(dead_code)]

use arrow2::{
	array::{MutableArray, MutablePrimitiveArray},
	bitmap::MutableBitmap,
	offset::Offsets,
};

use byteorder::ReadBytesExt;
use std::io::Result;

use crate::{
	model::{
		frame::{transpose, PortOccupancy},
		game::Port,
		slippi::Version,
	},
};

type BE = byteorder::BigEndian;

pub struct Data {
	pub pre: Pre,
	pub post: Post,
	pub validity: Option<MutableBitmap>,
}

impl Data {
	pub fn with_capacity(capacity: usize, version: Version) -> Self {
		Self {
			pre: Pre::with_capacity(capacity, version),
			post: Post::with_capacity(capacity, version),
			validity: None,
		}
	}

	pub fn len(&self) -> usize {
		self.pre.len()
	}

	pub fn push_null(&mut self, version: Version) {
		let len = self.len();
		self.validity
			.get_or_insert_with(|| MutableBitmap::from_len_set(len))
			.push(false);
		self.pre.push_null(version);
		self.post.push_null(version);
	}

	pub fn transpose_one(&self, i: usize, version: Version) -> transpose::Data {
		transpose::Data {
			pre: self.pre.transpose_one(i, version),
			post: self.post.transpose_one(i, version),
		}
	}
}

pub struct PortData {
	pub port: Port,
	pub leader: Data,
	pub follower: Option<Data>,
}

impl PortData {
	pub fn with_capacity(capacity: usize, version: Version, port: PortOccupancy) -> Self {
		Self {
			port: port.port,
			leader: Data::with_capacity(capacity, version),
			follower: match port.follower {
				true => Some(Data::with_capacity(capacity, version)),
				_ => None,
			},
		}
	}

	pub fn len(&self) -> usize {
		self.leader.len()
	}

	pub fn transpose_one(&self, i: usize, version: Version) -> transpose::PortData {
		transpose::PortData {
			port: self.port,
			leader: self.leader.transpose_one(i, version),
			follower: self.follower.as_ref().map(|f| f.transpose_one(i, version)),
		}
	}
}

pub struct Frame {
	pub id: MutablePrimitiveArray<i32>,
	pub ports: Vec<PortData>,
	pub start: Option<Start>,
	pub end: Option<End>,
	pub item_offset: Option<Offsets<i32>>,
	pub item: Option<Item>,
}

impl Frame {
	pub fn with_capacity(capacity: usize, version: Version, ports: &[PortOccupancy]) -> Self {
		Self {
			id: MutablePrimitiveArray::<i32>::with_capacity(capacity),
			ports: ports
				.iter()
				.map(|p| PortData::with_capacity(capacity, version, *p))
				.collect(),
			start: version.gte(2, 2).then(|| Start::with_capacity(capacity, version)),
			end: version.gte(3, 0).then(|| End::with_capacity(capacity, version)),
			item_offset: version.gte(3, 0).then(|| Offsets::<i32>::with_capacity(capacity)),
			item: version.gte(3, 0).then(|| Item::with_capacity(0, version)),
		}
	}

	pub fn len(&self) -> usize {
		self.id.len()
	}

	pub fn transpose_one(&self, i: usize, version: Version) -> transpose::Frame {
		transpose::Frame {
			id: self.id.values()[i],
			ports: self.ports.iter().map(|p| p.transpose_one(i, version)).collect(),
			start: version.gte(2, 2).then(|| self.start.as_ref().unwrap().transpose_one(i, version)),
			end: version.gte(3, 0).then(|| self.end.as_ref().unwrap().transpose_one(i, version)),
			items: version.gte(3, 0).then(|| {
				let (start, end) = self.item_offset.as_ref().unwrap().start_end(i);
				(start..end)
					.map(|i| self.item.as_ref().unwrap().transpose_one(i, version))
					.collect()
			}),
		}
	}
}
