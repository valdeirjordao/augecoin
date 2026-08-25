#![no_main]

use libfuzzer_sys::fuzz_target;
use augecoin_core::transaction::Transaction;

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize any byte sequence as a Transaction.
    // Must not panic — only return Ok or Err.
    let _ = Transaction::from_bytes(data);
});
