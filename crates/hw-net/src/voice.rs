//! Team voice on the wire: Opus frames of the players' microphones.
//!
//! A client packs up to [`MAX_FRAMES`] frames of [`FRAME_MS`] ms each into one
//! datagram ([`crate::ClientMsg::Voice`]); the host does not decode them, it
//! only checks the sizes and rate and forwards them to every other player
//! ([`crate::ServerMsg::Voice`]) and to the person at the host. Voice is
//! unreliable on purpose: a lost packet is a short gap, never a delay.
//!
//! The game talks to this through a [`VoiceBox`]: a small shared mailbox with
//! a queue of packets to send and a queue of packets heard, so the audio
//! threads never touch the network and the network never waits for audio.
//! A bot that wants to speak puts ready Opus frames into the same box.

use crate::PlayerId;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Milliseconds of sound in one frame.
pub const FRAME_MS: u32 = 20;
/// Samples per frame at the 48 kHz Opus runs at.
pub const FRAME_SAMPLES: usize = 960;
/// Frames in one datagram at most (60 ms: a few packets a second more would
/// only add delay).
pub const MAX_FRAMES: usize = 4;
/// Bytes one frame may have (24 kbit/s speech is about 60).
pub const MAX_FRAME_BYTES: usize = 160;
/// Frames per second the host forwards for one player: twice real time, so
/// a burst after a hiccup passes, a flood does not.
pub const MAX_FRAMES_PER_SECOND: f64 = 100.0;
/// Packets a mailbox queue holds; older ones are dropped.
const QUEUE: usize = 128;

/// Some speech: consecutive frames from one player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoicePacket {
    /// Who spoke (the sender's own id when it is sent, set by the host).
    pub from: PlayerId,
    /// Counts up by one per packet, wraps: gaps show lost packets.
    pub seq: u16,
    pub frames: Vec<Vec<u8>>,
}

impl VoicePacket {
    /// Whether the host should accept this from a client.
    pub fn is_sane(&self) -> bool {
        !self.frames.is_empty()
            && self.frames.len() <= MAX_FRAMES
            && self.frames.iter().all(|f| f.len() <= MAX_FRAME_BYTES)
    }
}

#[derive(Default)]
struct Inner {
    out: VecDeque<Vec<Vec<u8>>>,
    heard: VecDeque<VoicePacket>,
    seq: u16,
}

/// The mailbox between the audio threads and the network. Clone it to share
/// it.
#[derive(Clone, Default)]
pub struct VoiceBox(Arc<Mutex<Inner>>);

impl VoiceBox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether both are the same mailbox (one session's).
    pub fn same(&self, other: &VoiceBox) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic elsewhere must not silence voice for good.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queues frames to send (one datagram's worth, at most [`MAX_FRAMES`]).
    pub fn send(&self, frames: Vec<Vec<u8>>) {
        if frames.is_empty() || frames.len() > MAX_FRAMES {
            return;
        }
        let mut i = self.lock();
        i.out.push_back(frames);
        while i.out.len() > QUEUE {
            i.out.pop_front();
        }
    }

    /// Packets to send since the last call, with their sequence numbers.
    pub fn take_outgoing(&self) -> Vec<(u16, Vec<Vec<u8>>)> {
        let mut i = self.lock();
        let packets: Vec<_> = i.out.drain(..).collect();
        packets
            .into_iter()
            .map(|f| {
                i.seq = i.seq.wrapping_add(1);
                (i.seq, f)
            })
            .collect()
    }

    /// Something heard (called by the connection or the host).
    pub fn hear(&self, p: VoicePacket) {
        let mut i = self.lock();
        i.heard.push_back(p);
        while i.heard.len() > QUEUE {
            i.heard.pop_front();
        }
    }

    /// Speech heard since the last call.
    pub fn take_heard(&self) -> Vec<VoicePacket> {
        self.lock().heard.drain(..).collect()
    }
}

/// Lets a player send [`MAX_FRAMES_PER_SECOND`] frames a second on average.
#[derive(Clone, Copy, Debug)]
pub struct FrameBudget {
    tokens: f64,
    at: f64,
}

impl Default for FrameBudget {
    fn default() -> Self {
        Self {
            tokens: MAX_FRAMES_PER_SECOND,
            at: 0.0,
        }
    }
}

impl FrameBudget {
    /// Whether `frames` more frames may pass at time `now` (seconds).
    pub fn allow(&mut self, now: f64, frames: usize) -> bool {
        let dt = (now - self.at).max(0.0);
        self.at = now;
        self.tokens = (self.tokens + dt * MAX_FRAMES_PER_SECOND).min(MAX_FRAMES_PER_SECOND);
        if self.tokens >= frames as f64 {
            self.tokens -= frames as f64;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_numbers_packets_and_stays_bounded() {
        let b = VoiceBox::new();
        for _ in 0..QUEUE * 2 {
            b.send(vec![vec![1, 2, 3]]);
        }
        b.send(Vec::new()); // nothing to say
        b.send(vec![vec![0]; MAX_FRAMES + 1]); // too long for a datagram
        let out = b.take_outgoing();
        assert_eq!(out.len(), QUEUE);
        assert_eq!(out[1].0, out[0].0.wrapping_add(1));
        assert!(b.take_outgoing().is_empty());
        for k in 0..QUEUE * 2 {
            b.hear(VoicePacket {
                from: PlayerId(1),
                seq: k as u16,
                frames: vec![vec![0]],
            });
        }
        assert_eq!(b.take_heard().len(), QUEUE);
    }

    #[test]
    fn a_flood_is_cut_off_and_speech_passes() {
        let mut b = FrameBudget::default();
        // Real time: 3 frames per 60 ms packet, for a minute.
        let mut t = 0.0;
        for _ in 0..1000 {
            assert!(b.allow(t, 3));
            t += 0.06;
        }
        // A flood of 100 packets at once.
        let passed = (0..100).filter(|_| b.allow(t, 4)).count();
        assert!(passed < 40, "{passed}");
    }

    #[test]
    fn oversize_packets_are_not_sane() {
        let ok = VoicePacket {
            from: PlayerId(1),
            seq: 0,
            frames: vec![vec![0; 60]; 3],
        };
        assert!(ok.is_sane());
        assert!(!VoicePacket {
            frames: vec![vec![0; MAX_FRAME_BYTES + 1]],
            ..ok.clone()
        }
        .is_sane());
        assert!(!VoicePacket {
            frames: Vec::new(),
            ..ok
        }
        .is_sane());
    }
}
