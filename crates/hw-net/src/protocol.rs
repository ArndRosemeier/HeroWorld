//! Messages between game clients and the server.
//!
//! Every packet starts with a 4-byte magic, the protocol version and a message
//! type. The server sends one [`ServerMsg::Update`] per tick and player: the
//! entities near that player that changed since the player last confirmed
//! them, the ones that went away, and reliable messages (joins, chat, the end
//! of a session) that are repeated until the other side confirms them.
//! Clients send their newest inputs (a few of the previous ones too, so one
//! lost packet loses no key press) and say which updates arrived.

use crate::wire::{Reader, WireError, Writer};
use crate::PROTOCOL_VERSION;
use hw_core::NetId;

const MAGIC: [u8; 4] = *b"HWLD";

/// Biggest UDP payload we send (stays under typical MTU).
pub const MAX_PACKET: usize = 1200;

/// Inputs repeated in every input packet (the newest last).
pub const INPUT_REDUNDANCY: usize = 4;

/// A player, numbered by the server from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u16);

/// What a player is pressing, sampled once per client tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    /// Wanted horizontal movement in the local frame (x east, z south), length ≤ 1.
    pub move_x: f32,
    pub move_z: f32,
    /// Up or down while flying; (move_x, move_y, move_z) has length ≤ 1.
    pub move_y: f32,
    pub jump: bool,
    /// Go down (in a lift or in flight).
    pub descend: bool,
    /// Flying (the client keeps the toggle and sends it every tick, so a lost
    /// packet cannot lose a key press).
    pub fly: bool,
    /// Faster flight.
    pub boost: bool,
}

/// What an entity is, so the client knows how to draw it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntityKind {
    /// A player's hero.
    #[default]
    Hero,
    /// A loose crate: a physics body anyone can shove around.
    Crate,
}

impl EntityKind {
    fn code(self) -> u8 {
        match self {
            EntityKind::Hero => 0,
            EntityKind::Crate => 1,
        }
    }

    fn from_code(c: u8) -> Result<Self, WireError> {
        match c {
            0 => Ok(EntityKind::Hero),
            1 => Ok(EntityKind::Crate),
            _ => Err(WireError),
        }
    }
}

/// No rotation.
pub const NO_ROTATION: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// One entity as the server sends it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub id: NetId,
    pub kind: EntityKind,
    /// Position in the city's local frame (f64: precise anywhere in the city).
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    /// Orientation as a quaternion (x, y, z, w); heroes stay upright.
    pub rot: [f32; 4],
    /// Standing on something (needed to replay movement exactly).
    pub grounded: bool,
    /// The player who controls it, if it is a hero.
    pub owner: Option<PlayerId>,
}

impl Default for EntityState {
    fn default() -> Self {
        Self {
            id: NetId(0),
            kind: EntityKind::Hero,
            pos: [0.0; 3],
            vel: [0.0; 3],
            rot: NO_ROTATION,
            grounded: false,
            owner: None,
        }
    }
}

const F_GROUNDED: u8 = 1;
const F_OWNER: u8 = 2;
const F_VEL: u8 = 4;
const F_ROT: u8 = 8;

impl EntityState {
    fn flags(&self) -> u8 {
        let mut f = 0;
        if self.grounded {
            f |= F_GROUNDED;
        }
        if self.owner.is_some() {
            f |= F_OWNER;
        }
        if self.vel != [0.0; 3] {
            f |= F_VEL;
        }
        if self.rot != NO_ROTATION {
            f |= F_ROT;
        }
        f
    }

    /// Bytes this entity takes in an update.
    pub fn wire_size(&self) -> usize {
        let f = self.flags();
        4 + 1
            + 1
            + 24
            + if f & F_VEL != 0 { 12 } else { 0 }
            + if f & F_ROT != 0 { 16 } else { 0 }
            + if f & F_OWNER != 0 { 2 } else { 0 }
    }

    fn write(&self, w: &mut Writer) {
        let f = self.flags();
        w.u32(self.id.0).u8(self.kind.code()).u8(f);
        for v in self.pos {
            w.f64(v);
        }
        if f & F_VEL != 0 {
            for v in self.vel {
                w.f32(v);
            }
        }
        if f & F_ROT != 0 {
            for v in self.rot {
                w.f32(v);
            }
        }
        if let Some(p) = self.owner {
            w.u16(p.0);
        }
    }

    fn read(r: &mut Reader) -> Result<Self, WireError> {
        let id = NetId(r.u32()?);
        let kind = EntityKind::from_code(r.u8()?)?;
        let f = r.u8()?;
        if f >> 4 != 0 {
            return Err(WireError);
        }
        let pos = [r.f64()?, r.f64()?, r.f64()?];
        let vel = if f & F_VEL != 0 {
            [r.f32()?, r.f32()?, r.f32()?]
        } else {
            [0.0; 3]
        };
        let rot = if f & F_ROT != 0 {
            [r.f32()?, r.f32()?, r.f32()?, r.f32()?]
        } else {
            NO_ROTATION
        };
        let owner = if f & F_OWNER != 0 {
            match r.u16()? {
                0 => return Err(WireError),
                p => Some(PlayerId(p)),
            }
        } else {
            None
        };
        Ok(Self {
            id,
            kind,
            pos,
            vel,
            rot,
            grounded: f & F_GROUNDED != 0,
            owner,
        })
    }
}

/// Reliable news from the server: repeated until the client confirms it,
/// delivered in order, exactly once.
#[derive(Clone, Debug, PartialEq)]
pub enum ToClient {
    Joined {
        player: PlayerId,
        name: String,
    },
    Left {
        player: PlayerId,
        name: String,
    },
    Chat {
        from: PlayerId,
        text: String,
    },
    /// The host is closing the world; the client should leave.
    SessionEnd {
        reason: String,
    },
}

/// Reliable messages from a client.
#[derive(Clone, Debug, PartialEq)]
pub enum ToServer {
    Chat { text: String },
}

impl ToClient {
    pub fn wire_size(&self) -> usize {
        let s = |t: &str| 1 + t.len().min(255);
        1 + match self {
            ToClient::Joined { name, .. } | ToClient::Left { name, .. } => 2 + s(name),
            ToClient::Chat { text, .. } => 2 + s(text),
            ToClient::SessionEnd { reason } => s(reason),
        }
    }

    fn write(&self, w: &mut Writer) {
        match self {
            ToClient::Joined { player, name } => {
                w.u8(1).u16(player.0).str(name);
            }
            ToClient::Left { player, name } => {
                w.u8(2).u16(player.0).str(name);
            }
            ToClient::Chat { from, text } => {
                w.u8(3).u16(from.0).str(text);
            }
            ToClient::SessionEnd { reason } => {
                w.u8(4).str(reason);
            }
        }
    }

    fn read(r: &mut Reader) -> Result<Self, WireError> {
        Ok(match r.u8()? {
            1 => ToClient::Joined {
                player: PlayerId(r.u16()?),
                name: r.str()?,
            },
            2 => ToClient::Left {
                player: PlayerId(r.u16()?),
                name: r.str()?,
            },
            3 => ToClient::Chat {
                from: PlayerId(r.u16()?),
                text: r.str()?,
            },
            4 => ToClient::SessionEnd { reason: r.str()? },
            _ => return Err(WireError),
        })
    }
}

impl ToServer {
    pub fn wire_size(&self) -> usize {
        match self {
            ToServer::Chat { text } => 2 + text.len().min(255),
        }
    }

    fn write(&self, w: &mut Writer) {
        match self {
            ToServer::Chat { text } => {
                w.u8(1).str(text);
            }
        }
    }

    fn read(r: &mut Reader) -> Result<Self, WireError> {
        Ok(match r.u8()? {
            1 => ToServer::Chat { text: r.str()? },
            _ => return Err(WireError),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello {
        name: String,
    },
    Input {
        /// Sequence number of the newest input (the last in `inputs`).
        seq: u32,
        /// The newest inputs, oldest first, at most [`INPUT_REDUNDANCY`].
        inputs: Vec<InputState>,
        /// The newest update tick received, and which of the 32 ticks before
        /// it arrived too (bit 0 = `ack_tick - 1`).
        ack_tick: u64,
        ack_bits: u32,
        /// Every reliable message from the server below this ID has arrived.
        rel_ack: u32,
        /// Reliable messages not yet confirmed by the server.
        reliable: Vec<(u32, ToServer)>,
    },
    Bye,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome {
        player: PlayerId,
        hero: NetId,
        world_seed: u64,
        /// People in the town: with the seed, it makes the world.
        town_population: f64,
        tick_hz: u16,
    },
    Reject {
        reason: String,
    },
    Update {
        tick: u64,
        /// Last input sequence the server applied for this player.
        ack: u32,
        /// Every reliable message from this client below this ID has arrived.
        rel_ack: u32,
        reliable: Vec<(u32, ToClient)>,
        /// Entities that changed (whole state each, so any update can be
        /// applied on its own).
        entities: Vec<EntityState>,
        /// Entities the client should forget (gone, or out of its range).
        removed: Vec<NetId>,
    },
}

/// Bytes an update takes before its entities, reliable messages and removals.
pub const UPDATE_HEADER_BYTES: usize = 4 + 4 + 1 + 8 + 4 + 4 + 1 + 2 + 2;
/// Bytes per removed entity.
pub const REMOVED_BYTES: usize = 4;

fn header(w: &mut Writer, kind: u8) {
    w.buf.extend_from_slice(&MAGIC);
    w.u32(PROTOCOL_VERSION).u8(kind);
}

/// Checks magic and version; returns the message type.
fn read_header(r: &mut Reader) -> Result<u8, WireError> {
    let magic = [r.u8()?, r.u8()?, r.u8()?, r.u8()?];
    if magic != MAGIC || r.u32()? != PROTOCOL_VERSION {
        return Err(WireError);
    }
    r.u8()
}

fn write_input(w: &mut Writer, input: &InputState) {
    w.f32(input.move_x)
        .f32(input.move_z)
        .f32(input.move_y)
        .u8(input.jump as u8
            | (input.descend as u8) << 1
            | (input.fly as u8) << 2
            | (input.boost as u8) << 3);
}

fn read_input(r: &mut Reader) -> Result<InputState, WireError> {
    let (x, z, y) = (r.f32()?, r.f32()?, r.f32()?);
    let len = (x * x + y * y + z * z).sqrt();
    let k = if len > 1.0 { 1.0 / len } else { 1.0 };
    let bits = r.u8()?;
    if bits >> 4 != 0 {
        return Err(WireError);
    }
    Ok(InputState {
        move_x: x * k,
        move_y: y * k,
        move_z: z * k,
        jump: bits & 1 != 0,
        descend: bits & 2 != 0,
        fly: bits & 4 != 0,
        boost: bits & 8 != 0,
    })
}

impl ClientMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            ClientMsg::Hello { name } => {
                header(&mut w, 1);
                w.str(name);
            }
            ClientMsg::Input {
                seq,
                inputs,
                ack_tick,
                ack_bits,
                rel_ack,
                reliable,
            } => {
                header(&mut w, 2);
                let inputs = &inputs[inputs.len().saturating_sub(INPUT_REDUNDANCY)..];
                w.u32(*seq).u8(inputs.len() as u8);
                for i in inputs {
                    write_input(&mut w, i);
                }
                w.u64(*ack_tick).u32(*ack_bits).u32(*rel_ack);
                // Reliable messages that would overflow the packet wait.
                let mut fit = Vec::new();
                let mut room = MAX_PACKET.saturating_sub(w.buf.len() + 1);
                for (id, m) in reliable.iter().take(255) {
                    let n = 4 + m.wire_size();
                    if n > room {
                        break;
                    }
                    room -= n;
                    fit.push((id, m));
                }
                w.u8(fit.len() as u8);
                for (id, m) in fit {
                    w.u32(*id);
                    m.write(&mut w);
                }
            }
            ClientMsg::Bye => header(&mut w, 3),
        }
        w.buf
    }

    pub fn decode(buf: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(buf);
        let msg = match read_header(&mut r)? {
            1 => ClientMsg::Hello { name: r.str()? },
            2 => {
                let seq = r.u32()?;
                let n = r.u8()? as usize;
                if n == 0 || n > INPUT_REDUNDANCY {
                    return Err(WireError);
                }
                let mut inputs = Vec::with_capacity(n);
                for _ in 0..n {
                    inputs.push(read_input(&mut r)?);
                }
                let ack_tick = r.u64()?;
                let ack_bits = r.u32()?;
                let rel_ack = r.u32()?;
                let k = r.u8()? as usize;
                let mut reliable = Vec::with_capacity(k);
                for _ in 0..k {
                    let id = r.u32()?;
                    reliable.push((id, ToServer::read(&mut r)?));
                }
                ClientMsg::Input {
                    seq,
                    inputs,
                    ack_tick,
                    ack_bits,
                    rel_ack,
                    reliable,
                }
            }
            3 => ClientMsg::Bye,
            _ => return Err(WireError),
        };
        r.is_done().then_some(msg).ok_or(WireError)
    }
}

impl ServerMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            ServerMsg::Welcome {
                player,
                hero,
                world_seed,
                town_population,
                tick_hz,
            } => {
                header(&mut w, 1);
                w.u16(player.0)
                    .u32(hero.0)
                    .u64(*world_seed)
                    .f64(*town_population)
                    .u16(*tick_hz);
            }
            ServerMsg::Reject { reason } => {
                header(&mut w, 2);
                w.str(reason);
            }
            ServerMsg::Update {
                tick,
                ack,
                rel_ack,
                reliable,
                entities,
                removed,
            } => {
                // The sender sizes updates with `wire_size`; anything past
                // the packet limit is cut here as a last resort.
                header(&mut w, 3);
                w.u64(*tick).u32(*ack).u32(*rel_ack);
                let mut room = MAX_PACKET - UPDATE_HEADER_BYTES;
                let mut rel = Vec::new();
                for (id, m) in reliable.iter().take(255) {
                    let n = 4 + m.wire_size();
                    if n > room {
                        break;
                    }
                    room -= n;
                    rel.push((id, m));
                }
                w.u8(rel.len() as u8);
                for (id, m) in rel {
                    w.u32(*id);
                    m.write(&mut w);
                }
                let gone = removed
                    .len()
                    .min(room / REMOVED_BYTES)
                    .min(u16::MAX as usize);
                room -= gone * REMOVED_BYTES;
                w.u16(gone as u16);
                for id in &removed[..gone] {
                    w.u32(id.0);
                }
                let mut n = 0;
                for e in entities.iter().take(u16::MAX as usize) {
                    let s = e.wire_size();
                    if s > room {
                        break;
                    }
                    room -= s;
                    n += 1;
                }
                w.u16(n as u16);
                for e in &entities[..n] {
                    e.write(&mut w);
                }
            }
        }
        w.buf
    }

    pub fn decode(buf: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(buf);
        let msg = match read_header(&mut r)? {
            1 => ServerMsg::Welcome {
                player: PlayerId(r.u16()?),
                hero: NetId(r.u32()?),
                world_seed: r.u64()?,
                town_population: r.f64()?,
                tick_hz: r.u16()?,
            },
            2 => ServerMsg::Reject { reason: r.str()? },
            3 => {
                let tick = r.u64()?;
                let ack = r.u32()?;
                let rel_ack = r.u32()?;
                let k = r.u8()? as usize;
                let mut reliable = Vec::with_capacity(k);
                for _ in 0..k {
                    let id = r.u32()?;
                    reliable.push((id, ToClient::read(&mut r)?));
                }
                let g = r.u16()? as usize;
                if g * REMOVED_BYTES > MAX_PACKET {
                    return Err(WireError);
                }
                let mut removed = Vec::with_capacity(g);
                for _ in 0..g {
                    removed.push(NetId(r.u32()?));
                }
                let n = r.u16()? as usize;
                if n > MAX_PACKET / 30 {
                    return Err(WireError);
                }
                let mut entities = Vec::with_capacity(n);
                for _ in 0..n {
                    entities.push(EntityState::read(&mut r)?);
                }
                ServerMsg::Update {
                    tick,
                    ack,
                    rel_ack,
                    reliable,
                    entities,
                    removed,
                }
            }
            _ => return Err(WireError),
        };
        r.is_done().then_some(msg).ok_or(WireError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(id: u32) -> EntityState {
        EntityState {
            id: NetId(id),
            pos: [1.0, 2.0, -3.5],
            vel: [0.5, 0.0, 0.0],
            grounded: true,
            owner: Some(PlayerId(1)),
            ..Default::default()
        }
    }

    #[test]
    fn client_messages_round_trip() {
        for m in [
            ClientMsg::Hello {
                name: "Captain".into(),
            },
            ClientMsg::Input {
                seq: 9,
                inputs: vec![
                    InputState::default(),
                    InputState {
                        move_x: 0.6,
                        move_z: -0.8,
                        move_y: 0.0,
                        jump: true,
                        descend: false,
                        fly: true,
                        boost: false,
                    },
                ],
                ack_tick: 1234,
                ack_bits: 0xF0F0_0001,
                rel_ack: 3,
                reliable: vec![(
                    3,
                    ToServer::Chat {
                        text: "hello".into(),
                    },
                )],
            },
            ClientMsg::Bye,
        ] {
            assert_eq!(ClientMsg::decode(&m.encode()).unwrap(), m);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let update = ServerMsg::Update {
            tick: 77,
            ack: 5,
            rel_ack: 2,
            reliable: vec![
                (
                    0,
                    ToClient::Joined {
                        player: PlayerId(2),
                        name: "Bob".into(),
                    },
                ),
                (
                    1,
                    ToClient::SessionEnd {
                        reason: "host left".into(),
                    },
                ),
            ],
            entities: vec![
                hero(3),
                EntityState {
                    id: NetId(4),
                    kind: EntityKind::Crate,
                    pos: [100.25, 0.0, 9.0],
                    rot: [0.0, 0.6, 0.0, 0.8],
                    ..Default::default()
                },
            ],
            removed: vec![NetId(9), NetId(11)],
        };
        for m in [
            ServerMsg::Welcome {
                player: PlayerId(2),
                hero: NetId(8),
                world_seed: 42,
                town_population: 1234.5,
                tick_hz: 30,
            },
            ServerMsg::Reject {
                reason: "full".into(),
            },
            update,
        ] {
            let bytes = m.encode();
            assert!(bytes.len() <= MAX_PACKET);
            assert_eq!(ServerMsg::decode(&bytes).unwrap(), m);
        }
    }

    #[test]
    fn rejects_foreign_and_overlong_input() {
        assert!(ClientMsg::decode(b"nonsense").is_err());
        let mut bytes = ClientMsg::Bye.encode();
        bytes.push(0);
        assert!(ClientMsg::decode(&bytes).is_err());
        // Over-long movement is clamped to length 1.
        let m = ClientMsg::Input {
            seq: 1,
            inputs: vec![InputState {
                move_x: 3.0,
                move_z: 4.0,
                move_y: 0.0,
                jump: false,
                descend: true,
                fly: false,
                boost: true,
            }],
            ack_tick: 0,
            ack_bits: 0,
            rel_ack: 0,
            reliable: Vec::new(),
        };
        let ClientMsg::Input { inputs, .. } = ClientMsg::decode(&m.encode()).unwrap() else {
            panic!()
        };
        let i = inputs[0];
        assert!(((i.move_x.powi(2) + i.move_z.powi(2)).sqrt() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sizes_are_what_the_encoder_writes() {
        let crate_box = EntityState {
            kind: EntityKind::Crate,
            rot: [0.0, 0.0, 0.6, 0.8],
            ..hero(2)
        };
        for e in [hero(1), crate_box, EntityState::default()] {
            let one = ServerMsg::Update {
                tick: 0,
                ack: 0,
                rel_ack: 0,
                reliable: Vec::new(),
                entities: vec![e],
                removed: Vec::new(),
            };
            assert_eq!(one.encode().len(), UPDATE_HEADER_BYTES + e.wire_size());
        }
        let m = ToClient::Chat {
            from: PlayerId(1),
            text: "hi there".into(),
        };
        let one = ServerMsg::Update {
            tick: 0,
            ack: 0,
            rel_ack: 0,
            reliable: vec![(7, m.clone())],
            entities: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(one.encode().len(), UPDATE_HEADER_BYTES + 4 + m.wire_size());
    }

    #[test]
    fn an_overfull_update_is_cut_to_a_packet() {
        let bytes = ServerMsg::Update {
            tick: 0,
            ack: 0,
            rel_ack: 0,
            reliable: Vec::new(),
            entities: vec![hero(1); 100],
            removed: vec![NetId(1); 50],
        }
        .encode();
        assert!(bytes.len() <= MAX_PACKET, "{} bytes", bytes.len());
        assert!(ServerMsg::decode(&bytes).is_ok());
    }
}
