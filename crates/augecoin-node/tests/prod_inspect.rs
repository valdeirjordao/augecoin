//! Read-only inspection of node data directories (no mutation beyond creating
//! any missing column families the current build expects).
//!
//!   AUGECOIN_INSPECT_DIRS=/opt/augecoin/data/node-1,... \
//!     cargo test -p augecoin-node --test prod_inspect -- --ignored --nocapture

use augecoin_storage::Storage;

#[test]
#[ignore]
fn inspect_prod_dirs() {
    let dirs = std::env::var("AUGECOIN_INSPECT_DIRS").expect("AUGECOIN_INSPECT_DIRS");
    for dir in dirs.split(',') {
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        match Storage::open(dir) {
            Ok(s) => {
                let height = s.get_height().unwrap_or(u64::MAX);
                let latest = s.get_block(height).ok().flatten();
                let proto = latest.as_ref().map(|b| b.header.protocol_version);
                let vset_len = s
                    .get_validator_set_bytes()
                    .ok()
                    .flatten()
                    .map(|v| v.len());
                let flat = s.consensus_state_root().ok();
                let v6 = s.persisted_state_tree_root().ok().flatten();
                println!(
                    "dir={dir}\n  height={height} latest_protocol={:?} vset_bytes={:?}\n  flat_root={}\n  v6_meta={}",
                    proto,
                    vset_len,
                    flat.map(hex::encode).unwrap_or_else(|| "<err>".into()),
                    v6.map(hex::encode).unwrap_or_else(|| "<none>".into()),
                );
            }
            Err(e) => println!("dir={dir} OPEN_ERROR={e}"),
        }
    }
}