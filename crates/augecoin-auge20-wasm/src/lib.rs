//! AUGE20 token as a real WASM contract (Fase 9).
//!
//! This module is compiled to WebAssembly (`wasm32-unknown-unknown`) and is
//! executed by the existing `augecoin-wasm-runtime` pipeline — the exact same
//! runtime used for arbitrary stored WASM code. It does NOT use Wasmtime,
//! Wasmer, EVM or Solidity, and it never touches RocksDB or the account SafeBox
//! directly. All persistence goes through the fixed `auge` host API.
//!
//! Only the three V1 ABI exports are emitted: `instantiate`, `execute`,
//! `query`. Every mutation is buffered by the host's `ContractStateOverlay` and
//! only committed atomically by the engine after a fully successful run.
//!
//! Storage layout (all keys are prefixed with the 32-byte `contract_id` so the
//! engine's per-contract snapshot captures them):
//!   * `contract_id || "T"` -> TokenConfig (decimals, mint, burn, max_supply,
//!     owner, name, symbol)
//!   * `contract_id || "S"` -> total supply (u64 big-endian)
//!   * `contract_id || "B" || addr(8)` -> balance (u64 big-endian)
//!
//! Errors are reported through a tiny protocol: on failure the contract writes
//! two bytes at the output pointer, `[0xFF, code]`, and returns `2`. The engine
//! maps `code` to the matching `ContractError`, preserving every economic rule.

#![no_std]

// --- memory ABI (must match augecoin-wasm-runtime) ---
const INPUT_PTR: i32 = 1024;
const OUTPUT_PTR: i32 = 4096;
const KEY_OFF: i32 = 64; // storage key scratch (<= 64 bytes); must be NON-zero
const VAL_OFF: i32 = 1024; // storage value / event scratch (<= 2048 bytes)

// --- error codes (must match engine's map_auge20_err) ---
const ERR_INVALID_AMOUNT: u8 = 1;
const ERR_INSUFFICIENT: u8 = 2;
const ERR_UNAUTHORIZED: u8 = 3;
const ERR_MINT_DISABLED: u8 = 4;
const ERR_BURN_DISABLED: u8 = 5;
const ERR_MAX_SUPPLY: u8 = 6;
const ERR_OVERFLOW: u8 = 7;
const ERR_SERIALIZATION: u8 = 8;

// --- host imports (the only allowed "auge" module) ---
#[link(wasm_import_module = "auge")]
extern "C" {
    fn get_sender(ptr: i32) -> i32;
    fn get_block_height() -> i64;
    #[allow(dead_code)]
    fn get_contract_id(ptr: i32) -> i32;
    fn storage_read(key_ptr: i32, key_len: i32, out_ptr: i32, out_max: i32) -> i32;
    fn storage_write(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
    fn storage_delete(key_ptr: i32, key_len: i32) -> i32;
    fn emit_event(ptr: i32, len: i32) -> i32;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ---------------------------------------------------------------------------
// Linear memory helpers
// ---------------------------------------------------------------------------

unsafe fn mem_read(ptr: i32, len: i32, out: &mut [u8]) {
    let n = len as usize;
    let s = core::slice::from_raw_parts(ptr as *const u8, n);
    out[..n].copy_from_slice(s);
}

unsafe fn mem_write(ptr: i32, src: &[u8]) {
    core::ptr::copy_nonoverlapping(src.as_ptr(), ptr as *mut u8, src.len());
}

// ---------------------------------------------------------------------------
// Deterministic codec (mirrors augecoin-contracts::codec)
// ---------------------------------------------------------------------------

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8]) -> Self {
        Reader { d, p: 0 }
    }
    fn u8(&mut self) -> Result<u8, ()> {
        if self.p >= self.d.len() {
            return Err(());
        }
        let v = self.d[self.p];
        self.p += 1;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, ()> {
        if self.p + 4 > self.d.len() {
            return Err(());
        }
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.d[self.p..self.p + 4]);
        self.p += 4;
        Ok(u32::from_be_bytes(b))
    }
    fn u64(&mut self) -> Result<u64, ()> {
        if self.p + 8 > self.d.len() {
            return Err(());
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.d[self.p..self.p + 8]);
        self.p += 8;
        Ok(u64::from_be_bytes(b))
    }
    fn bool(&mut self) -> Result<bool, ()> {
        Ok(self.u8()? != 0)
    }
    fn bytes(&mut self) -> Result<&'a [u8], ()> {
        let len = self.u32()? as usize;
        if self.p + len > self.d.len() {
            return Err(());
        }
        let v = &self.d[self.p..self.p + len];
        self.p += len;
        Ok(v)
    }
    fn end(&self) -> Result<(), ()> {
        if self.p != self.d.len() {
            Err(())
        } else {
            Ok(())
        }
    }
}

struct Writer {
    buf: [u8; 2048],
    len: usize,
}

impl Writer {
    fn new() -> Self {
        Writer {
            buf: [0u8; 2048],
            len: 0,
        }
    }
    fn u8(&mut self, v: u8) -> &mut Self {
        self.buf[self.len] = v;
        self.len += 1;
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.buf[self.len..self.len + 4].copy_from_slice(&v.to_be_bytes());
        self.len += 4;
        self
    }
    fn u64(&mut self, v: u64) -> &mut Self {
        self.buf[self.len..self.len + 8].copy_from_slice(&v.to_be_bytes());
        self.len += 8;
        self
    }
    fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(if v { 1 } else { 0 })
    }
    fn fixed(&mut self, v: &[u8]) -> &mut Self {
        self.buf[self.len..self.len + v.len()].copy_from_slice(v);
        self.len += v.len();
        self
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32);
        self.buf[self.len..self.len + v.len()].copy_from_slice(v);
        self.len += v.len();
        self
    }
    fn as_slice(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

// ---------------------------------------------------------------------------
// Small string (no allocator)
// ---------------------------------------------------------------------------

struct SmallStr {
    buf: [u8; 64],
    len: usize,
}

impl SmallStr {
    fn from_bytes(s: &[u8]) -> Result<SmallStr, ()> {
        if s.len() > 64 {
            return Err(());
        }
        let mut buf = [0u8; 64];
        buf[..s.len()].copy_from_slice(s);
        Ok(SmallStr {
            buf,
            len: s.len(),
        })
    }
    fn as_slice(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

// ---------------------------------------------------------------------------
// Contract state
// ---------------------------------------------------------------------------

struct TokenState {
    decimals: u8,
    mint_enabled: bool,
    burn_enabled: bool,
    max_supply: u64,
    owner: u64,
    name: SmallStr,
    symbol: SmallStr,
}

fn build_key(tag: u8, addr: u64, cid: &[u8; 32]) -> i32 {
    unsafe {
        mem_write(KEY_OFF, cid);
        let off = KEY_OFF + 32;
        mem_write(off, &[tag]);
        if tag == b'B' {
            mem_write(off + 1, &addr.to_be_bytes());
            32 + 1 + 8
        } else {
            32 + 1
        }
    }
}

fn storage_read_value(key_len: i32, out: &mut [u8; 2048]) -> i32 {
    let r = unsafe { storage_read(KEY_OFF, key_len, VAL_OFF, 2048) };
    if r > 0 {
        unsafe {
            mem_read(VAL_OFF, r, &mut out[..r as usize]);
        }
    }
    r
}

fn storage_write_value(key_len: i32, value: &[u8]) {
    unsafe {
        mem_write(VAL_OFF, value);
        storage_write(KEY_OFF, key_len, VAL_OFF, value.len() as i32);
    }
}

/// Read a `u64` value stored at `cid || tag || addr`. The key is rebuilt from
/// its components immediately before the host call so that no stale bytes left
/// at `KEY_OFF` (e.g. by a prior `get_sender`) can corrupt the lookup.
fn read_u64(tag: u8, addr: u64, cid: &[u8; 32]) -> u64 {
    let key_len = build_key(tag, addr, cid);
    let r = unsafe { storage_read(KEY_OFF, key_len, VAL_OFF, 2048) };
    if r != 8 {
        return 0;
    }
    let mut b = [0u8; 8];
    unsafe {
        mem_read(VAL_OFF, 8, &mut b);
    }
    u64::from_be_bytes(b)
}

/// Write a `u64` value stored at `cid || tag || addr`.
fn write_u64(tag: u8, addr: u64, cid: &[u8; 32], v: u64) {
    let key_len = build_key(tag, addr, cid);
    storage_write_value(key_len, &v.to_be_bytes());
}

fn read_token_state(cid: &[u8; 32]) -> Result<TokenState, u8> {
    let kl = build_key(b'T', 0, cid);
    let mut raw = [0u8; 2048];
    let r = storage_read_value(kl, &mut raw);
    if r < 0 {
        return Err(ERR_SERIALIZATION);
    }
    let mut r = Reader::new(&raw[..r as usize]);
    let decimals = r.u8().map_err(|_| ERR_SERIALIZATION)?;
    let mint_enabled = r.bool().map_err(|_| ERR_SERIALIZATION)?;
    let burn_enabled = r.bool().map_err(|_| ERR_SERIALIZATION)?;
    let max_supply = r.u64().map_err(|_| ERR_SERIALIZATION)?;
    let owner = r.u64().map_err(|_| ERR_SERIALIZATION)?;
    let name = SmallStr::from_bytes(r.bytes().map_err(|_| ERR_SERIALIZATION)?)
        .map_err(|_| ERR_SERIALIZATION)?;
    let symbol = SmallStr::from_bytes(r.bytes().map_err(|_| ERR_SERIALIZATION)?)
        .map_err(|_| ERR_SERIALIZATION)?;
    r.end().map_err(|_| ERR_SERIALIZATION)?;
    Ok(TokenState {
        decimals,
        mint_enabled,
        burn_enabled,
        max_supply,
        owner,
        name,
        symbol,
    })
}

fn write_token_state(cid: &[u8; 32], t: &TokenState) {
    let kl = build_key(b'T', 0, cid);
    let mut w = Writer::new();
    w.u8(t.decimals)
        .bool(t.mint_enabled)
        .bool(t.burn_enabled)
        .u64(t.max_supply)
        .u64(t.owner)
        .bytes(t.name.as_slice())
        .bytes(t.symbol.as_slice());
    storage_write_value(kl, w.as_slice());
}

// ---------------------------------------------------------------------------
// Event emission (ContractEvent::to_bytes format)
// ---------------------------------------------------------------------------

fn emit_transferred(cid: &[u8; 32], from: u64, to: u64, amount: u64) {
    let mut w = Writer::new();
    w.u8(2).fixed(cid).u64(from).u64(to).u64(amount);
    unsafe {
        mem_write(VAL_OFF, w.as_slice());
        emit_event(VAL_OFF, w.len as i32);
    }
}

fn emit_minted(cid: &[u8; 32], to: u64, amount: u64) {
    let mut w = Writer::new();
    w.u8(3).fixed(cid).u64(to).u64(amount);
    unsafe {
        mem_write(VAL_OFF, w.as_slice());
        emit_event(VAL_OFF, w.len as i32);
    }
}

fn emit_burned(cid: &[u8; 32], from: u64, amount: u64) {
    let mut w = Writer::new();
    w.u8(4).fixed(cid).u64(from).u64(amount);
    unsafe {
        mem_write(VAL_OFF, w.as_slice());
        emit_event(VAL_OFF, w.len as i32);
    }
}

// ---------------------------------------------------------------------------
// Output / error protocol
// ---------------------------------------------------------------------------

unsafe fn write_output(buf: &[u8]) -> i32 {
    mem_write(OUTPUT_PTR, buf);
    buf.len() as i32
}

fn fail(code: u8) -> i32 {
    unsafe { write_output(&[0xFF, code]) }
}

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn instantiate(ptr: i32, len: i32) -> i32 {
    let mut input = [0u8; 4096];
    unsafe {
        let n = len as usize;
        if n > input.len() {
            return fail(ERR_SERIALIZATION);
        }
        mem_read(ptr, len, &mut input[..n]);
    }
    let mut r = Reader::new(&input[..len as usize]);

    // TokenInit: decimals, mint, burn, initial_supply, max_supply, name, symbol
    let decimals = match r.u8() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    let mint_enabled = match r.bool() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    let burn_enabled = match r.bool() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    let initial_supply = match r.u64() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    let max_supply = match r.u64() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    let name = match SmallStr::from_bytes(r.bytes().unwrap_or(&[])) {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    let symbol = match SmallStr::from_bytes(r.bytes().unwrap_or(&[])) {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };
    if r.end().is_err() {
        return fail(ERR_SERIALIZATION);
    }

    if name.as_slice().is_empty() || symbol.as_slice().is_empty() {
        return fail(ERR_INVALID_AMOUNT);
    }
    if decimals > 18 {
        return fail(ERR_INVALID_AMOUNT);
    }
    if initial_supply > max_supply {
        return fail(ERR_MAX_SUPPLY);
    }

    let mut cid = [0u8; 32];
    unsafe {
        get_contract_id(KEY_OFF);
        mem_read(KEY_OFF, 32, &mut cid);
    }
    let mut sender = [0u8; 8];
    unsafe {
        get_sender(KEY_OFF);
        mem_read(KEY_OFF, 8, &mut sender);
    }
    let owner = u64::from_be_bytes(sender);

    // Write token config (with owner).
    let ts = TokenState {
        decimals,
        mint_enabled,
        burn_enabled,
        max_supply,
        owner,
        name,
        symbol,
    };
    write_token_state(&cid, &ts);

    // Write supply.
    write_u64(b'S', 0, &cid, initial_supply);

    // Write initial balance to owner.
    write_u64(b'B', owner, &cid, initial_supply);

    0
}

#[no_mangle]
pub extern "C" fn execute(ptr: i32, len: i32) -> i32 {
    let mut input = [0u8; 4096];
    unsafe {
        let n = len as usize;
        if n > input.len() {
            return fail(ERR_SERIALIZATION);
        }
        mem_read(ptr, len, &mut input[..n]);
    }
    let mut r = Reader::new(&input[..len as usize]);

    let tag = match r.u8() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };

    let mut cid = [0u8; 32];
    unsafe {
        get_contract_id(KEY_OFF);
        mem_read(KEY_OFF, 32, &mut cid);
    }

    let ts = match read_token_state(&cid) {
        Ok(v) => v,
        Err(c) => return fail(c),
    };
    let supply = read_u64(b'S', 0, &cid);

    match tag {
        0 => {
            // Transfer { to, amount }
            let to = match r.u64() {
                Ok(v) => v,
                Err(_) => return fail(ERR_SERIALIZATION),
            };
            let amount = match r.u64() {
                Ok(v) => v,
                Err(_) => return fail(ERR_SERIALIZATION),
            };
            if r.end().is_err() {
                return fail(ERR_SERIALIZATION);
            }
            if amount == 0 {
                return fail(ERR_INVALID_AMOUNT);
            }
            let mut sender = [0u8; 8];
            unsafe {
                get_sender(KEY_OFF);
                mem_read(KEY_OFF, 8, &mut sender);
            }
            let from = u64::from_be_bytes(sender);
            let from_bal = read_u64(b'B', from, &cid);
            if from_bal < amount {
                return fail(ERR_INSUFFICIENT);
            }
            let new_from = match from_bal.checked_sub(amount) {
                Some(v) => v,
                None => return fail(ERR_OVERFLOW),
            };
            write_u64(b'B', from, &cid, new_from);
            let to_bal = read_u64(b'B', to, &cid);
            let new_to = match to_bal.checked_add(amount) {
                Some(v) => v,
                None => return fail(ERR_OVERFLOW),
            };
            write_u64(b'B', to, &cid, new_to);
            emit_transferred(&cid, from, to, amount);
            0
        }
        1 => {
            // Mint { to, amount }
            let to = match r.u64() {
                Ok(v) => v,
                Err(_) => return fail(ERR_SERIALIZATION),
            };
            let amount = match r.u64() {
                Ok(v) => v,
                Err(_) => return fail(ERR_SERIALIZATION),
            };
            if r.end().is_err() {
                return fail(ERR_SERIALIZATION);
            }
            if amount == 0 {
                return fail(ERR_INVALID_AMOUNT);
            }
            let mut sender = [0u8; 8];
            unsafe {
                get_sender(KEY_OFF);
                mem_read(KEY_OFF, 8, &mut sender);
            }
            let sender_addr = u64::from_be_bytes(sender);
            if sender_addr != ts.owner {
                return fail(ERR_UNAUTHORIZED);
            }
            if !ts.mint_enabled {
                return fail(ERR_MINT_DISABLED);
            }
            let new_supply = match supply.checked_add(amount) {
                Some(v) => v,
                None => return fail(ERR_OVERFLOW),
            };
            if new_supply > ts.max_supply {
                return fail(ERR_MAX_SUPPLY);
            }
            write_u64(b'S', 0, &cid, new_supply);
            let to_bal = read_u64(b'B', to, &cid);
            let new_to = match to_bal.checked_add(amount) {
                Some(v) => v,
                None => return fail(ERR_OVERFLOW),
            };
            write_u64(b'B', to, &cid, new_to);
            emit_minted(&cid, to, amount);
            0
        }
        2 => {
            // Burn { amount }
            let amount = match r.u64() {
                Ok(v) => v,
                Err(_) => return fail(ERR_SERIALIZATION),
            };
            if r.end().is_err() {
                return fail(ERR_SERIALIZATION);
            }
            if amount == 0 {
                return fail(ERR_INVALID_AMOUNT);
            }
            if !ts.burn_enabled {
                return fail(ERR_BURN_DISABLED);
            }
            if amount == 0 {
                return fail(ERR_INVALID_AMOUNT);
            }
            let mut sender = [0u8; 8];
            unsafe {
                get_sender(KEY_OFF);
                mem_read(KEY_OFF, 8, &mut sender);
            }
            let from = u64::from_be_bytes(sender);
            let bal = read_u64(b'B', from, &cid);
            if bal < amount {
                return fail(ERR_INSUFFICIENT);
            }
            let new_bal = match bal.checked_sub(amount) {
                Some(v) => v,
                None => return fail(ERR_OVERFLOW),
            };
            let new_supply = match supply.checked_sub(amount) {
                Some(v) => v,
                None => return fail(ERR_OVERFLOW),
            };
            write_u64(b'S', 0, &cid, new_supply);
            write_u64(b'B', from, &cid, new_bal);
            emit_burned(&cid, from, amount);
            0
        }
        _ => fail(ERR_SERIALIZATION),
    }
}

#[no_mangle]
pub extern "C" fn query(ptr: i32, len: i32) -> i32 {
    let mut input = [0u8; 4096];
    unsafe {
        let n = len as usize;
        if n > input.len() {
            return fail(ERR_SERIALIZATION);
        }
        mem_read(ptr, len, &mut input[..n]);
    }
    let mut r = Reader::new(&input[..len as usize]);
    let tag = match r.u8() {
        Ok(v) => v,
        Err(_) => return fail(ERR_SERIALIZATION),
    };

    let mut cid = [0u8; 32];
    unsafe {
        get_contract_id(KEY_OFF);
        mem_read(KEY_OFF, 32, &mut cid);
    }

    match tag {
        0 => {
            // BalanceOf(addr)
            let addr = match r.u64() {
                Ok(v) => v,
                Err(_) => return fail(ERR_SERIALIZATION),
            };
            if r.end().is_err() {
                return fail(ERR_SERIALIZATION);
            }
            let bal = read_u64(b'B', addr, &cid);
            let mut w = Writer::new();
            w.u8(0).u64(bal);
            unsafe { write_output(w.as_slice()) }
        }
        1 => {
            // TotalSupply
            if r.end().is_err() {
                return fail(ERR_SERIALIZATION);
            }
            let supply = read_u64(b'S', 0, &cid);
            let mut w = Writer::new();
            w.u8(1).u64(supply);
            unsafe { write_output(w.as_slice()) }
        }
        2 => {
            // TokenInfo
            if r.end().is_err() {
                return fail(ERR_SERIALIZATION);
            }
            let ts = match read_token_state(&cid) {
                Ok(v) => v,
                Err(c) => return fail(c),
            };
            let supply = read_u64(b'S', 0, &cid);
            let mut w = Writer::new();
            w.u8(2)
                .bytes(ts.name.as_slice())
                .bytes(ts.symbol.as_slice())
                .u8(ts.decimals)
                .u64(ts.max_supply)
                .bool(ts.mint_enabled)
                .bool(ts.burn_enabled)
                .u64(ts.owner)
                .u64(supply);
            unsafe { write_output(w.as_slice()) }
        }
        _ => fail(ERR_SERIALIZATION),
    }
}
