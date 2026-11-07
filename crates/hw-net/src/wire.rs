//! A tiny little-endian binary encoding for packets. Hand-rolled so the wire
//! format is explicit, stable and free of dependencies.

/// Writes values into a packet buffer.
#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }

    /// A string of at most 255 bytes (longer ones are cut at a char boundary).
    pub fn str(&mut self, s: &str) -> &mut Self {
        let mut end = s.len().min(255);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8);
        self.buf.extend_from_slice(&s.as_bytes()[..end]);
        self
    }
}

/// Reads values back; every read fails cleanly on a short or bad packet.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

/// A malformed packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError;

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let end = self.pos.checked_add(N).ok_or(WireError)?;
        let bytes = self.buf.get(self.pos..end).ok_or(WireError)?;
        self.pos = end;
        Ok(bytes.try_into().expect("length checked"))
    }

    pub fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    pub fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    pub fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    pub fn f32(&mut self) -> Result<f32, WireError> {
        let v = f32::from_le_bytes(self.take()?);
        v.is_finite().then_some(v).ok_or(WireError)
    }

    pub fn f64(&mut self) -> Result<f64, WireError> {
        let v = f64::from_le_bytes(self.take()?);
        v.is_finite().then_some(v).ok_or(WireError)
    }

    pub fn bool(&mut self) -> Result<bool, WireError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError),
        }
    }

    pub fn str(&mut self) -> Result<String, WireError> {
        let n = self.u8()? as usize;
        let end = self.pos.checked_add(n).ok_or(WireError)?;
        let bytes = self.buf.get(self.pos..end).ok_or(WireError)?;
        self.pos = end;
        String::from_utf8(bytes.to_vec()).map_err(|_| WireError)
    }

    pub fn is_done(&self) -> bool {
        self.pos == self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut w = Writer::new();
        w.u8(7)
            .u16(300)
            .u32(70_000)
            .u64(1 << 40)
            .f32(1.5)
            .f64(-2.25)
            .bool(true)
            .str("héro");
        let mut r = Reader::new(&w.buf);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u16().unwrap(), 300);
        assert_eq!(r.u32().unwrap(), 70_000);
        assert_eq!(r.u64().unwrap(), 1 << 40);
        assert_eq!(r.f32().unwrap(), 1.5);
        assert_eq!(r.f64().unwrap(), -2.25);
        assert!(r.bool().unwrap());
        assert_eq!(r.str().unwrap(), "héro");
        assert!(r.is_done());
    }

    #[test]
    fn short_and_bad_packets_fail() {
        assert_eq!(Reader::new(&[1, 2]).u32(), Err(WireError));
        assert_eq!(Reader::new(&[2]).bool(), Err(WireError));
        assert_eq!(Reader::new(&[5, b'a']).str(), Err(WireError));
        let nan = f32::NAN.to_le_bytes();
        assert_eq!(Reader::new(&nan).f32(), Err(WireError));
    }
}
