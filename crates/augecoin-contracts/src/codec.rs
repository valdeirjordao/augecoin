//! Minimal, deterministic binary codec used by the contract subsystem.
//!
//! We deliberately avoid `serde`/`bincode` here so that serialization is fully
//! under our control, big-endian, and reproducible across all validators. All
//! integers are encoded big-endian.

use crate::ContractError;

pub struct Writer {
    buf: Vec<u8>,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.buf.push(if v { 1 } else { 0 });
        self
    }

    pub fn fixed(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v);
        self
    }

    pub fn string(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes());
        self
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn u8(&mut self) -> Result<u8, ContractError> {
        if self.pos >= self.data.len() {
            return Err(ContractError::InvalidSerialization);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn u32(&mut self) -> Result<u32, ContractError> {
        if self.pos + 4 > self.data.len() {
            return Err(ContractError::InvalidSerialization);
        }
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.data[self.pos..self.pos + 4]);
        self.pos += 4;
        Ok(u32::from_be_bytes(b))
    }

    pub fn u64(&mut self) -> Result<u64, ContractError> {
        if self.pos + 8 > self.data.len() {
            return Err(ContractError::InvalidSerialization);
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_be_bytes(b))
    }

    pub fn bool(&mut self) -> Result<bool, ContractError> {
        Ok(self.u8()? != 0)
    }

    pub fn fixed<const N: usize>(&mut self) -> Result<[u8; N], ContractError> {
        if self.pos + N > self.data.len() {
            return Err(ContractError::InvalidSerialization);
        }
        let mut b = [0u8; N];
        b.copy_from_slice(&self.data[self.pos..self.pos + N]);
        self.pos += N;
        Ok(b)
    }

    pub fn bytes(&mut self) -> Result<Vec<u8>, ContractError> {
        let len = self.u32()? as usize;
        if self.pos + len > self.data.len() {
            return Err(ContractError::InvalidSerialization);
        }
        let v = self.data[self.pos..self.pos + len].to_vec();
        self.pos += len;
        Ok(v)
    }

    pub fn string(&mut self) -> Result<String, ContractError> {
        let b = self.bytes()?;
        String::from_utf8(b).map_err(|_| ContractError::InvalidSerialization)
    }

    /// Assert there is no trailing data left.
    pub fn expect_end(&self) -> Result<(), ContractError> {
        if self.remaining() != 0 {
            return Err(ContractError::InvalidSerialization);
        }
        Ok(())
    }
}
