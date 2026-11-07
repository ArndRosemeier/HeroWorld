//! Messages between game clients and the server.
//!
//! Every packet starts with a 4-byte magic, the protocol version and a message
//! type. Snapshots carry the full state of the entities near a player, so a
//! lost packet costs nothing but a frame; deltas come later.

use crate::wire::{Reader, WireError, Writer};
use crate::PROTOCOL_VERSION;
use hw_core::NetId;

const MAGIC: [u8; 4] = *b"HWLD";

/// Biggest UDP payload we send (stays under typical MTU).
pub const MAX_PACKET: usize = 1200;

/// A player, numbered by the server from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u16);

/// What a player is pressing, sampled once per client tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    /// Wanted horizontal movement in the local frame (x east, z south), length ≤ 1.
    pub move_x: f32,
    pub move_z: f32,
    pub jump: bool,
}

/// One entity in a snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub id: NetId,
    /// Position in the city's local frame (f64: precise anywhere in the city).
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    /// The player who controls it, if it is a hero.
    pub owner: Option<PlayerId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello { name: String },
    Input { seq: u32, input: InputState },
    Bye,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome {
        player: PlayerId,
        hero: NetId,
        world_seed: u64,
        tick_hz: u16,
    },
    Reject {
        reason: String,
    },
    Snapshot {
        tick: u64,
        /// Last input sequence the server applied for this player.
        ack: u32,
        entities: Vec<EntityState>,
    },
}

/// Most entities that fit in one snapshot packet.
pub const MAX_SNAPSHOT_ENTITIES: usize = (MAX_PACKET - 32) / ENTITY_BYTES;
const ENTITY_BYTES: usize = 4 + 24 + 12 + 2;

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

impl ClientMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            ClientMsg::Hello { name } => {
                header(&mut w, 1);
                w.str(name);
            }
            ClientMsg::Input { seq, input } => {
                header(&mut w, 2);
                w.u32(*seq)
                    .f32(input.move_x)
                    .f32(input.move_z)
                    .bool(input.jump);
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
                let (x, z) = (r.f32()?, r.f32()?);
                let len = (x * x + z * z).sqrt();
                let (move_x, move_z) = if len > 1.0 {
                    (x / len, z / len)
                } else {
                    (x, z)
                };
                ClientMsg::Input {
                    seq,
                    input: InputState {
                        move_x,
                        move_z,
                        jump: r.bool()?,
                    },
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
                tick_hz,
            } => {
                header(&mut w, 1);
                w.u16(player.0).u32(hero.0).u64(*world_seed).u16(*tick_hz);
            }
            ServerMsg::Reject { reason } => {
                header(&mut w, 2);
                w.str(reason);
            }
            ServerMsg::Snapshot {
                tick,
                ack,
                entities,
            } => {
                header(&mut w, 3);
                let n = entities.len().min(MAX_SNAPSHOT_ENTITIES);
                w.u64(*tick).u32(*ack).u16(n as u16);
                for e in &entities[..n] {
                    w.u32(e.id.0);
                    for v in e.pos {
                        w.f64(v);
                    }
                    for v in e.vel {
                        w.f32(v);
                    }
                    w.u16(e.owner.map_or(0, |p| p.0));
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
                tick_hz: r.u16()?,
            },
            2 => ServerMsg::Reject { reason: r.str()? },
            3 => {
                let tick = r.u64()?;
                let ack = r.u32()?;
                let n = r.u16()? as usize;
                if n > MAX_SNAPSHOT_ENTITIES {
                    return Err(WireError);
                }
                let mut entities = Vec::with_capacity(n);
                for _ in 0..n {
                    let id = NetId(r.u32()?);
                    let pos = [r.f64()?, r.f64()?, r.f64()?];
                    let vel = [r.f32()?, r.f32()?, r.f32()?];
                    let owner = match r.u16()? {
                        0 => None,
                        p => Some(PlayerId(p)),
                    };
                    entities.push(EntityState {
                        id,
                        pos,
                        vel,
                        owner,
                    });
                }
                ServerMsg::Snapshot {
                    tick,
                    ack,
                    entities,
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

    #[test]
    fn client_messages_round_trip() {
        for m in [
            ClientMsg::Hello {
                name: "Captain".into(),
            },
            ClientMsg::Input {
                seq: 9,
                input: InputState {
                    move_x: 0.6,
                    move_z: -0.8,
                    jump: true,
                },
            },
            ClientMsg::Bye,
        ] {
            assert_eq!(ClientMsg::decode(&m.encode()).unwrap(), m);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let snap = ServerMsg::Snapshot {
            tick: 77,
            ack: 5,
            entities: vec![
                EntityState {
                    id: NetId(3),
                    pos: [1.0, 2.0, -3.5],
                    vel: [0.5, 0.0, 0.0],
                    owner: Some(PlayerId(1)),
                },
                EntityState {
                    id: NetId(4),
                    pos: [100.25, 0.0, 9.0],
                    vel: [0.0; 3],
                    owner: None,
                },
            ],
        };
        for m in [
            ServerMsg::Welcome {
                player: PlayerId(2),
                hero: NetId(8),
                world_seed: 42,
                tick_hz: 30,
            },
            ServerMsg::Reject {
                reason: "full".into(),
            },
            snap,
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
            input: InputState {
                move_x: 3.0,
                move_z: 4.0,
                jump: false,
            },
        };
        let ClientMsg::Input { input, .. } = ClientMsg::decode(&m.encode()).unwrap() else {
            panic!()
        };
        assert!(((input.move_x.powi(2) + input.move_z.powi(2)).sqrt() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_full_snapshot_fits_a_packet() {
        let entities = vec![
            EntityState {
                id: NetId(1),
                pos: [0.0; 3],
                vel: [0.0; 3],
                owner: None,
            };
            MAX_SNAPSHOT_ENTITIES + 10
        ];
        let bytes = ServerMsg::Snapshot {
            tick: 0,
            ack: 0,
            entities,
        }
        .encode();
        assert!(bytes.len() <= MAX_PACKET, "{} bytes", bytes.len());
    }
}
