//! Independent SMT-root audit over stopped testnet data directories.
//!
//! Reads one or more node data dirs, then for each reports:
//!   * the committed height and persisted `state_tree_meta` root;
//!   * an **independent full rebuild** root (recomputed from the state CFs);
//!   * the legacy flat root.
//!
//! All nodes at the same height must report identical roots. Ignored by default
//! so it never runs in the normal suite; run explicitly after stopping a
//! testnet:
//!
//!   AUGECOIN_AUDIT_DIRS=/tmp/opencode/t4/data/node-0,/tmp/.../node-1 \
//!     cargo test -p augecoin-node --test testnet_root_audit -- --ignored --nocapture

use augecoin_storage::Storage;

#[test]
#[ignore]
fn audit_testnet_roots() {
    let dirs = std::env::var("AUGECOIN_AUDIT_DIRS").expect("AUGECOIN_AUDIT_DIRS must be set");
    let activation: u64 = std::env::var("AUGECOIN_AUDIT_ACTIVATION")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut roots = Vec::new();
    for dir in dirs.split(',') {
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        let storage = Storage::open(dir).expect("open node data dir");
        storage.set_state_tree_activation(activation);
        let height = storage.get_height().unwrap();
        let persisted = storage.persisted_state_tree_root().unwrap();
        let rebuilt = storage.rebuild_state_tree().unwrap();
        let flat = storage.consensus_state_root().unwrap();
        println!(
            "dir={dir}\n  height={height}\n  persisted_v6={}\n  rebuilt_v6  ={}\n  flat_legacy ={}",
            hex::encode(persisted.unwrap_or([0u8; 64])),
            hex::encode(rebuilt),
            hex::encode(flat)
        );
        roots.push((height, rebuilt));
    }

    // All nodes at the same height must agree on the SMT root.
    if let Some((first_height, first_root)) = roots.first().copied() {
        for (height, root) in &roots[1..] {
            if *height == first_height {
                assert_eq!(root, &first_root, "SMT roots diverged at height {height}");
            }
        }
    }
    println!(
        "independent rebuild roots agree across {} nodes",
        roots.len()
    );
}
