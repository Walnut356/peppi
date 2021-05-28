use std::{mem, sync::Arc};

use num_traits::identities::Zero;

use arrow::{
	array,
	array::{Array, StructArray},
	buffer,
	datatypes::{
		self,
		DataType,
		BooleanType,
		Int8Type,
		UInt8Type,
		Int16Type,
		UInt16Type,
		Int32Type,
		UInt32Type,
		Int64Type,
		UInt64Type,
		Float32Type,
	},
	error,
};

use super::{
	frame,
	game,
	primitives::{Direction, Port}
};

pub struct Buffer {
	pub buffer: buffer::MutableBuffer,
	pub validity_buffer: Option<array::BooleanBufferBuilder>,
	pub data_type: datatypes::DataType,
	pub name: String,
}

impl Buffer {
	pub fn item_size(&self) -> usize {
		use datatypes::DataType::*;
		match self.data_type {
			Boolean => 1,
			Int8 | UInt8 => 1,
			Int16 | UInt16 => 2,
			Int32 | UInt32 => 4,
			Int64 | UInt64 => 8,
			Float32 => 4,
			_ => unimplemented!(),
		}
	}

	pub fn into_array(self) -> Arc<dyn array::Array> {
		use array::*;
		let mut builder = array::ArrayData::builder(self.data_type.clone())
			.len(self.buffer.len() / self.item_size())
			.add_buffer(match self.data_type {
				Boolean => unsafe {
					buffer::MutableBuffer::from_trusted_len_iter_bool(
						self.buffer.into_iter().map(|x| *x != 0))
					.into()
				},
				_ => self.buffer.into(),
			});
		if let Some(mut validities) = self.validity_buffer {
			builder = builder.null_bit_buffer(validities.finish());
		}

		let data = builder.build();
		use datatypes::DataType::*;
		match self.data_type {
			Boolean => Arc::new(BooleanArray::from(data)),
			Int8 => Arc::new(Int8Array::from(data)),
			UInt8 => Arc::new(UInt8Array::from(data)),
			Int16 => Arc::new(Int16Array::from(data)),
			UInt16 => Arc::new(UInt16Array::from(data)),
			Int32 => Arc::new(Int32Array::from(data)),
			UInt32 => Arc::new(UInt32Array::from(data)),
			Int64 => Arc::new(Int64Array::from(data)),
			UInt64 => Arc::new(UInt64Array::from(data)),
			Float32 => Arc::new(Float32Array::from(data)),
			_ => unimplemented!(),
		}
	}
}

pub trait ArrowPrimitive: Copy + Sized {
	type ArrowNativeType: datatypes::ArrowNativeType + Zero;
	type ArrowType;
	const ARROW_DATA_TYPE: DataType;
	fn into_arrow_native(self) -> Self::ArrowNativeType;
}

macro_rules! primitives {
	( $($t1:ty : $t2:ty),* $(,)? ) => { $(
		impl ArrowPrimitive for $t1 {
			type ArrowNativeType = $t1;
			type ArrowType = $t2;
			const ARROW_DATA_TYPE: DataType = <$t2 as datatypes::ArrowPrimitiveType>::DATA_TYPE;
			fn into_arrow_native(self) -> Self::ArrowNativeType {
				self
			}
		}
	)* }
}

primitives!(
	i8: Int8Type,
	u8: UInt8Type,
	i16: Int16Type,
	u16: UInt16Type,
	i32: Int32Type,
	u32: UInt32Type,
	i64: Int64Type,
	u64: UInt64Type,
	f32: Float32Type,
);

impl ArrowPrimitive for bool {
	type ArrowNativeType = u8;
	type ArrowType = BooleanType;
	const ARROW_DATA_TYPE: DataType = DataType::Boolean;
	fn into_arrow_native(self) -> Self::ArrowNativeType {
		match self {
			true => 1,
			_ => 0,
		}
	}
}

impl ArrowPrimitive for Port {
	type ArrowNativeType = u8;
	type ArrowType = UInt8Type;
	const ARROW_DATA_TYPE: DataType = DataType::UInt8;
	fn into_arrow_native(self) -> Self::ArrowNativeType { self as u8 }
}

impl ArrowPrimitive for Direction {
	type ArrowNativeType = u8;
	type ArrowType = BooleanType;
	const ARROW_DATA_TYPE: DataType = DataType::Boolean;
	fn into_arrow_native(self) -> Self::ArrowNativeType { self as u8 }
}

pub trait Arrow {
	fn arrow_buffers(&self, name: &str, len: usize) -> Vec<Buffer>;
	fn arrow_append(&self, buffers: &mut Vec<Buffer>, index: usize) -> usize;
}

impl<T> Arrow for T where T: ArrowPrimitive {
	fn arrow_buffers(&self, name: &str, len: usize) -> Vec<Buffer> {
		vec![Buffer {
			buffer: buffer::MutableBuffer::new(len * mem::size_of::<T::ArrowNativeType>()),
			validity_buffer: None,
			data_type: T::ARROW_DATA_TYPE,
			name: name.to_string(),
		}]
	}

	fn arrow_append(&self, buffers: &mut Vec<Buffer>, index: usize) -> usize {
		buffers[index].buffer.push(self.into_arrow_native());
		1
	}
}

impl<T> Arrow for Option<T> where T: ArrowPrimitive {
	fn arrow_buffers(&self, name: &str, len: usize) -> Vec<Buffer> {
		vec![Buffer {
			buffer: buffer::MutableBuffer::new(len * mem::size_of::<T::ArrowNativeType>()),
			validity_buffer: Some(array::BooleanBufferBuilder::new(len)),
			data_type: T::ARROW_DATA_TYPE,
			name: name.to_string(),
		}]
	}

	fn arrow_append(&self, buffers: &mut Vec<Buffer>, index: usize) -> usize {
		let valid = match self {
			Some(v) => {
				buffers[index].buffer.push(v.into_arrow_native());
				true
			},
			_ => {
				buffers[index].buffer.push(T::ArrowNativeType::zero());
				false
			}
		};
		buffers[index].validity_buffer.as_mut().unwrap().append(valid);
		1
	}
}

impl<T, const N: usize> Arrow for [T; N] where T: Arrow {
	fn arrow_buffers(&self, name: &str, len: usize) -> Vec<Buffer> {
		let mut buffers = Vec::new();
		for i in 0 .. N {
			buffers.extend(self[i].arrow_buffers(
				format!("{}.{}", name, i).as_str(),
				len));
		}
		buffers
	}

	fn arrow_append(&self, buffers: &mut Vec<Buffer>, index: usize) -> usize {
		let mut offset = 0;
		for i in 0 .. N {
			offset += self[i].arrow_append(buffers, index + offset);
		}
		offset
	}
}

impl Arrow for frame::PortData {
	fn arrow_buffers(&self, name: &str, len: usize) -> Vec<Buffer> {
		let mut buffers = self.leader.arrow_buffers(
			format!("{}.{}", name, "leader").as_str(),
			len
		);
		if let Some(f) = &self.follower {
			buffers.extend(f.arrow_buffers(
				format!("{}.{}", name, "follower").as_str(),
				len));
		}
		buffers
	}

	fn arrow_append(&self, buffers: &mut Vec<Buffer>, index: usize) -> usize {
		let mut offset = 0;
		offset += self.leader.arrow_append(buffers, index + offset);
		if let Some(f) = &self.follower {
			offset += f.arrow_append(buffers, index + offset);
		}
		offset
	}
}

impl<const N: usize> Arrow for frame::Frame<N> {
	fn arrow_buffers(&self, name: &str, len: usize) -> Vec<Buffer> {
		let mut buffers = self.ports.arrow_buffers(
			format!("{}.{}", name, "ports").as_str(),
			len
		);
		if let Some(start) = self.start {
			buffers.extend(start.arrow_buffers(
				format!("{}.{}", name, "start").as_str(),
				len
			));
		}
		if let Some(end) = self.end {
			buffers.extend(end.arrow_buffers(
				format!("{}.{}", name, "end").as_str(),
				len
			))
		}
		buffers
	}

	fn arrow_append(&self, buffers: &mut Vec<Buffer>, index: usize) -> usize {
		let mut offset = 0;
		offset += self.ports.arrow_append(buffers, index + offset);
		if let Some(start) = &self.start {
			offset += start.arrow_append(buffers, index + offset);
		}
		if let Some(end) = &self.end {
			offset += end.arrow_append(buffers, index + offset);
		}
		offset
	}
}

fn pop_to(stack: &mut Vec<(String, Vec<(datatypes::Field, array::ArrayRef)>)>, len: usize) {
	while stack.len() > len {
		let (name, fields) = stack.pop().unwrap();
		let z = Arc::new(StructArray::from(fields));
		let last = stack.last_mut().unwrap();
		last.1.push((
			datatypes::Field::new(
				&name,
				z.data().data_type().clone(),
				false,
			),
			z,
		));
	}
}

fn struct_array(buffers: Vec<Buffer>) -> error::Result<StructArray> {
	let mut stack: Vec<(String, Vec<(datatypes::Field, array::ArrayRef)>)> = vec![];
	for b in buffers {
		let path: Vec<_> = b.name.split('.').collect();
		let common_prefix = stack.iter().zip(path.iter()).take_while(|(a, b)| &&a.0 == b).count();
		pop_to(&mut stack, common_prefix);
		while stack.len() + 1 < path.len() {
			stack.push((path[stack.len()].to_string(), vec![]));
		}
		stack.last_mut().unwrap().1.push((
			datatypes::Field::new(
				path.last().unwrap(),
				b.data_type.clone(),
				b.validity_buffer.is_some(),
			),
			b.into_array(),
		));
	}

	pop_to(&mut stack, 1);
	let (_, root) = stack.pop().unwrap();
	Ok(StructArray::from(root))
}

fn _frames<const N: usize>(src: &Vec<frame::Frame<N>>) -> error::Result<StructArray> {
	let mut buffers = src[0].arrow_buffers("", src.len());
	for frame in src {
		frame.arrow_append(&mut buffers, 0);
	}
	struct_array(buffers)
}

pub fn frames(game: &game::Game) -> error::Result<StructArray> {
	use game::Frames::*;
	match &game.frames {
		P1(f) => _frames(f),
		P2(f) => _frames(f),
		P3(f) => _frames(f),
		P4(f) => _frames(f),
	}
}

fn _items<const N: usize>(src: &Vec<frame::Frame<N>>) -> error::Result<Option<StructArray>> {
	let mut len = 0;
	let mut first_item = None;
	for frame in src {
		if let Some(items) = frame.items.as_ref() {
			len += items.len();
			if first_item.is_none() {
				if let Some(item) = items.iter().nth(0) {
					first_item = Some(item)
				}
			}
		}
	}

	if let Some(first_item) = first_item {
		let mut buffers = vec![
			Buffer {
				buffer: buffer::MutableBuffer::new(len * std::mem::size_of::<i32>()),
				validity_buffer: None,
				data_type: datatypes::DataType::UInt32,
				name: ".frame_index".to_string(),
			}
		];
		buffers.extend(first_item.arrow_buffers("", src.len()));
		for (idx, frame) in src.iter().enumerate() {
			for item in frame.items.as_ref().unwrap() {
				(idx as i32 + game::FIRST_FRAME_INDEX).arrow_append(&mut buffers, 0);
				item.arrow_append(&mut buffers, 1);
			}
		}
		Ok(Some(struct_array(buffers)?))
	} else {
		Ok(None)
	}
}

pub fn items(game: &game::Game) -> error::Result<Option<StructArray>> {
	use game::Frames::*;
	match &game.frames {
		P1(f) => _items(f),
		P2(f) => _items(f),
		P3(f) => _items(f),
		P4(f) => _items(f),
	}
}