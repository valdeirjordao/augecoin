#![no_main]

use libfuzzer_sys::fuzz_target;
use augecoin_core::block::OperationBlock;

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize any byte sequence as an OperationBlock.
    // Must not panic — only return Ok or Err.
    let _ = OperationBlock::from_bytes(data);
});
