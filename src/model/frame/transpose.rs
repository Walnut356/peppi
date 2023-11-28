// This file is auto-generated by `gen/scripts/regen`. Do not edit.

use crate::model::game::Port;

#[derive(PartialEq, Debug)]
pub struct Data {
	pub pre: Pre,
	pub post: Post,
}

#[derive(PartialEq, Debug)]
pub struct PortData {
	pub port: Port,
	pub leader: Data,
	pub follower: Option<Data>,
}

#[derive(PartialEq, Debug)]
pub struct Frame {
	pub id: i32,
	pub ports: Vec<PortData>,
	pub start: Option<Start>,
	pub end: Option<End>,
	pub items: Option<Vec<Item>>,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct End {
	pub latest_finalized_frame: Option<i32>,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Item {
	pub r#type: u16,
	pub state: u8,
	pub direction: f32,
	pub velocity: Velocity,
	pub position: Position,
	pub damage: u16,
	pub timer: f32,
	pub id: u32,
	pub misc: Option<ItemMisc>,
	pub owner: Option<i8>,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct ItemMisc(pub u8, pub u8, pub u8, pub u8);

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Position {
	pub x: f32,
	pub y: f32,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Post {
	pub character: u8,
	pub state: u16,
	pub position: Position,
	pub direction: f32,
	pub percent: f32,
	pub shield: f32,
	pub last_attack_landed: u8,
	pub combo_count: u8,
	pub last_hit_by: u8,
	pub stocks: u8,
	pub state_age: Option<f32>,
	pub state_flags: Option<StateFlags>,
	pub misc_as: Option<f32>,
	pub airborne: Option<u8>,
	pub ground: Option<u16>,
	pub jumps: Option<u8>,
	pub l_cancel: Option<u8>,
	pub hurtbox_state: Option<u8>,
	pub velocities: Option<Velocities>,
	pub hitlag: Option<f32>,
	pub animation_index: Option<u32>,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Pre {
	pub random_seed: u32,
	pub state: u16,
	pub position: Position,
	pub direction: f32,
	pub joystick: Position,
	pub cstick: Position,
	pub triggers: f32,
	pub buttons: u32,
	pub buttons_physical: u16,
	pub triggers_physical: TriggersPhysical,
	pub raw_analog_x: Option<i8>,
	pub percent: Option<f32>,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Start {
	pub random_seed: u32,
	pub scene_frame_counter: Option<u32>,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct StateFlags(pub u8, pub u8, pub u8, pub u8, pub u8);

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct TriggersPhysical {
	pub l: f32,
	pub r: f32,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Velocities {
	pub self_x_air: f32,
	pub self_y: f32,
	pub knockback_x: f32,
	pub knockback_y: f32,
	pub self_x_ground: f32,
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
pub struct Velocity {
	pub x: f32,
	pub y: f32,
}
