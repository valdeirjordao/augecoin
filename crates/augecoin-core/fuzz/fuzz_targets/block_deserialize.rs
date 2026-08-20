#![no_main]

use libfuzzer_sys::fuzz_target;
use augecoin_core::block::Block;

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize any byte sequence as a Block.
    // Must not panic — only return Ok or Err.
    let _ = Block::from_bytes(data);
});
