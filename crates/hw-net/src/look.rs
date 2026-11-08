//! A player's looks on the wire: the hero's `CharacterConfig` as compact
//! JSON (only what differs from Actors' defaults), opaque to the network
//! and the server. It is too big for one packet, so it travels in pieces
//! over the reliable channel and is put together again on arrival.

/// Bytes per piece: small enough to ride next to the world in an update.
pub const LOOK_PIECE: usize = 320;
/// The most a look may be (a full face and body of sliders fits easily).
pub const MAX_LOOK: usize = 32 * 1024;

/// `look` cut into (part, parts, bytes) pieces; empty for a look that is
/// too big (it is then not sent: the others see a stand-in).
pub fn pieces(look: &[u8]) -> Vec<(u8, u8, Vec<u8>)> {
    if look.is_empty() || look.len() > MAX_LOOK {
        return Vec::new();
    }
    let chunks: Vec<&[u8]> = look.chunks(LOOK_PIECE).collect();
    let n = chunks.len() as u8;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| (i as u8, n, c.to_vec()))
        .collect()
}

/// Puts one player's pieces back together. A piece from a newer look (a
/// different count, or a part 0 after the last one) starts over.
#[derive(Clone, Debug, Default)]
pub struct LookParts {
    parts: Vec<Option<Vec<u8>>>,
}

impl LookParts {
    /// Takes a piece; returns the whole look once the last piece is in.
    pub fn add(&mut self, part: u8, parts: u8, bytes: Vec<u8>) -> Option<Vec<u8>> {
        if parts == 0 || part >= parts || bytes.len() > LOOK_PIECE {
            return None;
        }
        if self.parts.len() != parts as usize || part == 0 {
            self.parts = vec![None; parts as usize];
        }
        self.parts[part as usize] = Some(bytes);
        if self.parts.iter().all(|p| p.is_some()) {
            let whole: Vec<u8> = self.parts.drain(..).flatten().flatten().collect();
            return Some(whole);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_comes_back_whole() {
        let look: Vec<u8> = (0..2000u32).map(|i| (i * 7 % 251) as u8).collect();
        let p = pieces(&look);
        assert_eq!(p.len(), look.len().div_ceil(LOOK_PIECE));
        assert!(p.len() > 1);
        let mut parts = LookParts::default();
        let mut out = None;
        for (i, n, b) in p {
            out = parts.add(i, n, b);
        }
        assert_eq!(out.as_deref(), Some(&look[..]));
        assert!(pieces(&vec![1u8; MAX_LOOK + 1]).is_empty());
        // A new look interrupts an old one.
        let mut parts = LookParts::default();
        let a = pieces(&look);
        parts.add(a[0].0, a[0].1, a[0].2.clone());
        let b = pieces(b"{\"x\":1}");
        assert_eq!(
            parts.add(b[0].0, b[0].1, b[0].2.clone()).as_deref(),
            Some(&b"{\"x\":1}"[..])
        );
    }
}
