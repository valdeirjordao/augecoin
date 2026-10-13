//! Incremental Sparse Merkle Tree (SMT) backing the protocol-v6 state root.
//!
//! ## Why
//!
//! [`crate::state_root::compute_consensus_state_root`] hashes every record of
//! every consensus column family on every call — `O(total state)` per block.
//! This module provides a *compact, content-addressed binary Merkle trie* over
//! a 256-bit key space. The root depends only on the records that changed
//! (`O(k · depth)` for `k` touched keys) and the tree persists `O(n)` nodes for
//! `n` records. A naïve fixed-depth Sparse Merkle Tree would persist `O(n·256)`
//! nodes (~243 per leaf), which is unusable at chain scale; here branch nodes
//! exist only where two subtrees actually diverge.
//!
//! ## Cryptographic specification (protocol v6)
//!
//! * Hash: BLAKE3-512 (`augecoin_crypto::hash::blake3_512`), 64-byte output.
//! * Record key (leaf position): `key256 = truncate_256(BLAKE3-512(
//!   "AUGECOIN_SMT_KEY_V1" || len(cf_name)_u32_be || cf_name ||
//!   len(key)_u32_be || key))`. Including the column-family name stops records
//!   from being reinterpreted across namespaces (`accounts` vs
//!   `contract_balances`, ...).
//! * Leaf node bytes: `0x00 || key256 || H(value)`. Leaf hash = `H(bytes)`.
//! * Branch node bytes: `0x02 || height_u16_be || prefix || left || right`,
//!   where `prefix` is the branch's shared bit-path (`[0, height)`, packed
//!   big-endian, unused low bits zero), `height` is the bit index at which its
//!   two subtrees diverge, and `left`/`right` are the two child hashes. Branch
//!   hash = `H(bytes)`.
//! * Every persisted branch has exactly two non-empty children. A branch left
//!   with a single child is collapsed, so the tree is canonical for a given
//!   key/value map regardless of insertion order.
//! * Empty tree root: the sentinel `[0u8; 64]` ([`EMPTY_STATE_ROOT`]), matching
//!   the legacy flat root's empty value.
//!
//! Every operation is a pure function of `(cf_name, key, value)`; the root does
//! not depend on insertion order, wall-clock time or memory layout, so all
//! honest validators derive the same root.
//!
//! ## Persistence
//!
//! Nodes are content-addressed by their 64-byte hash and stored in the
//! `state_nodes` column family. An update rewrites only the nodes on the paths
//! of the changed keys and deletes the nodes it orphans, so `state_nodes` stays
//! bounded by `O(n)` and per-block work is `O(k · depth)`.

use augecoin_crypto::hash::blake3_512;
use std::collections::BTreeMap;

/// Sentinel root for a completely empty state, shared with the legacy flat
/// root so `height == 0` behaves identically under both algorithms.
pub const EMPTY_STATE_ROOT: [u8; 64] = [0u8; 64];

/// Width of the key space in bits.
pub const TREE_DEPTH: usize = 256;

const KEY_DOMAIN: &[u8] = b"AUGECOIN_SMT_KEY_V1";
const LEAF_TAG: u8 = 0x00;
const BRANCH_TAG: u8 = 0x02;

const LEAF_LEN: usize = 1 + 32 + 64;
const BRANCH_LEN: usize = 1 + 2 + 32 + 64 + 64;

/// Deterministic leaf position for a `(column family, key)` record.
pub fn record_key(cf_name: &[u8], key: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(KEY_DOMAIN.len() + 4 + cf_name.len() + 4 + key.len());
    buf.extend_from_slice(KEY_DOMAIN);
    buf.extend_from_slice(&(cf_name.len() as u32).to_be_bytes());
    buf.extend_from_slice(cf_name);
    buf.extend_from_slice(&(key.len() as u32).to_be_bytes());
    buf.extend_from_slice(key);
    let h = blake3_512(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&h[..32]);
    out
}

fn encode_leaf(key256: &[u8; 32], value_hash: &[u8; 64]) -> [u8; LEAF_LEN] {
    let mut buf = [0u8; LEAF_LEN];
    buf[0] = LEAF_TAG;
    buf[1..33].copy_from_slice(key256);
    buf[33..97].copy_from_slice(value_hash);
    buf
}

fn encode_branch(
    height: u16,
    prefix: &[u8; 32],
    left: &[u8; 64],
    right: &[u8; 64],
) -> [u8; BRANCH_LEN] {
    let mut buf = [0u8; BRANCH_LEN];
    buf[0] = BRANCH_TAG;
    buf[1..3].copy_from_slice(&height.to_be_bytes());
    buf[3..35].copy_from_slice(prefix);
    buf[35..99].copy_from_slice(left);
    buf[99..163].copy_from_slice(right);
    buf
}

fn leaf_hash_raw(key256: &[u8; 32], value_hash: &[u8; 64]) -> [u8; 64] {
    blake3_512(&encode_leaf(key256, value_hash))
}

/// Leaf node hash for an existing record.
pub fn hash_leaf(key256: &[u8; 32], value: &[u8]) -> [u8; 64] {
    leaf_hash_raw(key256, &blake3_512(value))
}

/// Branch node hash (exposed for proof verification).
pub fn hash_branch(height: u16, prefix: &[u8; 32], left: &[u8; 64], right: &[u8; 64]) -> [u8; 64] {
    blake3_512(&encode_branch(height, prefix, left, right))
}

fn parse_leaf(bytes: &[u8]) -> Option<([u8; 32], [u8; 64])> {
    if bytes.len() != LEAF_LEN || bytes[0] != LEAF_TAG {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[1..33]);
    let mut vh = [0u8; 64];
    vh.copy_from_slice(&bytes[33..97]);
    Some((key, vh))
}

struct BranchNode {
    height: u16,
    prefix: [u8; 32],
    left: [u8; 64],
    right: [u8; 64],
}

fn parse_branch(bytes: &[u8]) -> Option<BranchNode> {
    if bytes.len() != BRANCH_LEN || bytes[0] != BRANCH_TAG {
        return None;
    }
    let height = u16::from_be_bytes([bytes[1], bytes[2]]);
    let mut prefix = [0u8; 32];
    prefix.copy_from_slice(&bytes[3..35]);
    let mut left = [0u8; 64];
    left.copy_from_slice(&bytes[35..99]);
    let mut right = [0u8; 64];
    right.copy_from_slice(&bytes[99..163]);
    Some(BranchNode {
        height,
        prefix,
        left,
        right,
    })
}

fn bit(key: &[u8; 32], index: usize) -> u8 {
    (key[index / 8] >> (7 - (index % 8))) & 1
}

fn set_bit(packed: &mut [u8; 32], index: usize) {
    packed[index / 8] |= 1 << (7 - (index % 8));
}

fn clear_bit(packed: &mut [u8; 32], index: usize) {
    packed[index / 8] &= !(1 << (7 - (index % 8)));
}

/// Copy bits `[from, to)` of `src` into `dst` at the same positions.
fn copy_bits(dst: &mut [u8; 32], src: &[u8; 32], from: usize, to: usize) {
    for index in from..to {
        if bit(src, index) == 1 {
            set_bit(dst, index);
        } else {
            clear_bit(dst, index);
        }
    }
}

/// First bit index at which two distinct keys differ. `None` when equal.
pub fn first_diff_bit(a: &[u8; 32], b: &[u8; 32]) -> Option<usize> {
    for index in 0..TREE_DEPTH {
        if bit(a, index) != bit(b, index) {
            return Some(index);
        }
    }
    None
}

/// First bit index in `[0, height)` where `key` differs from `prefix`.
fn first_prefix_mismatch(key: &[u8; 32], prefix: &[u8; 32], height: usize) -> Option<usize> {
    (0..height).find(|&index| bit(key, index) != bit(prefix, index))
}

/// A single record mutation: `value == None` removes the record.
#[derive(Clone, Debug)]
pub struct TreeChange {
    pub key256: [u8; 32],
    pub value: Option<Vec<u8>>,
}

impl TreeChange {
    pub fn put(cf_name: &[u8], key: &[u8], value: &[u8]) -> Self {
        TreeChange {
            key256: record_key(cf_name, key),
            value: Some(value.to_vec()),
        }
    }

    pub fn delete(cf_name: &[u8], key: &[u8]) -> Self {
        TreeChange {
            key256: record_key(cf_name, key),
            value: None,
        }
    }
}

/// Storage abstraction for content-addressed tree nodes.
pub trait NodeBackend {
    /// Serialized node stored under `hash`, or `None` when absent.
    fn get(&self, hash: &[u8; 64]) -> Option<Vec<u8>>;
    /// Persist the serialized node under `hash`.
    fn put(&mut self, hash: [u8; 64], bytes: Vec<u8>);
    /// Erase the node stored under `hash`.
    fn delete(&mut self, hash: &[u8; 64]);
}

/// An in-memory node backend. Used by tests and rebuild audits.
#[derive(Default)]
pub struct MemNodeBackend {
    pub nodes: BTreeMap<[u8; 64], Vec<u8>>,
    pub reads: u64,
    pub writes: u64,
    pub deletes: u64,
}

impl NodeBackend for MemNodeBackend {
    fn get(&self, hash: &[u8; 64]) -> Option<Vec<u8>> {
        self.nodes.get(hash).cloned()
    }
    fn put(&mut self, hash: [u8; 64], bytes: Vec<u8>) {
        self.writes += 1;
        self.nodes.insert(hash, bytes);
    }
    fn delete(&mut self, hash: &[u8; 64]) {
        self.deletes += 1;
        self.nodes.remove(hash);
    }
}

/// Counters describing the work a single [`update`] performed.
#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateStats {
    pub nodes_read: u64,
    pub nodes_written: u64,
    pub nodes_deleted: u64,
    pub leaves_written: u64,
    pub leaves_removed: u64,
}

struct Ctx<'a, B: NodeBackend> {
    backend: &'a mut B,
    stats: UpdateStats,
}

impl<B: NodeBackend> Ctx<'_, B> {
    fn load(&mut self, hash: &[u8; 64]) -> Option<Vec<u8>> {
        self.stats.nodes_read += 1;
        self.backend.get(hash)
    }

    fn store_leaf(&mut self, key256: &[u8; 32], value_hash: &[u8; 64]) -> [u8; 64] {
        let bytes = encode_leaf(key256, value_hash);
        let hash = blake3_512(&bytes);
        self.backend.put(hash, bytes.to_vec());
        self.stats.nodes_written += 1;
        self.stats.leaves_written += 1;
        hash
    }

    fn store_leaf_value(&mut self, key256: &[u8; 32], value: &[u8]) -> [u8; 64] {
        let vh = blake3_512(value);
        self.store_leaf(key256, &vh)
    }

    fn store_branch(&mut self, node: &BranchNode) -> [u8; 64] {
        let bytes = encode_branch(node.height, &node.prefix, &node.left, &node.right);
        let hash = blake3_512(&bytes);
        self.backend.put(hash, bytes.to_vec());
        self.stats.nodes_written += 1;
        hash
    }

    /// Delete `old` unless it is the same node we just wrote.
    fn retire(&mut self, old: &[u8; 64], new: &[u8; 64]) {
        if old != new {
            self.backend.delete(old);
            self.stats.nodes_deleted += 1;
        }
    }

    /// Build a branch at `height` with children determined by bit `height` of
    /// `left_key`/`right_key`. `prefix` must be the shared bits `[0, height)`.
    fn make_branch(
        &mut self,
        height: usize,
        prefix: [u8; 32],
        left_key: &[u8; 32],
        left_hash: [u8; 64],
        right_key: &[u8; 32],
        right_hash: [u8; 64],
    ) -> [u8; 64] {
        let left_bits = bit(left_key, height);
        let right_bits = bit(right_key, height);
        debug_assert_ne!(left_bits, right_bits, "branch children must diverge");
        let node = if left_bits == 0 {
            BranchNode {
                height: height as u16,
                prefix,
                left: left_hash,
                right: right_hash,
            }
        } else {
            BranchNode {
                height: height as u16,
                prefix,
                left: right_hash,
                right: left_hash,
            }
        };
        self.store_branch(&node)
    }

    fn insert(&mut self, root: Option<[u8; 64]>, key: &[u8; 32], value: &[u8]) -> [u8; 64] {
        let Some(old) = root else {
            return self.store_leaf_value(key, value);
        };
        let bytes = self.load(&old).expect("tree node must exist");
        if let Some((k2, vh2)) = parse_leaf(&bytes) {
            if &k2 == key {
                let new = self.store_leaf_value(key, value);
                self.retire(&old, &new);
                return new;
            }
            let d = first_diff_bit(&k2, key).expect("distinct keys differ");
            let mut prefix = [0u8; 32];
            copy_bits(&mut prefix, &k2, 0, d);
            // The existing leaf is reused verbatim as one child, so it must not
            // be retired here (it is still referenced by the new branch).
            let left_hash = self.store_leaf(&k2, &vh2);
            let right_hash = self.store_leaf_value(key, value);
            let new = self.make_branch(d, prefix, &k2, left_hash, key, right_hash);
            let _ = old;
            return new;
        }
        let node = parse_branch(&bytes).expect("valid node encoding");
        let h = node.height as usize;
        // Does the key share this branch's prefix?
        if let Some(d) = first_prefix_mismatch(key, &node.prefix, h) {
            // The key diverges above the branch: insert a new branch at `d`
            // with the existing subtree on one side and the new leaf on the
            // other. The existing node is reused as a child (its hash is kept).
            let mut prefix = [0u8; 32];
            copy_bits(&mut prefix, &node.prefix, 0, d);
            let leaf_hash = self.store_leaf_value(key, value);
            let subtree_bit = bit(&node.prefix, d);
            let new = if subtree_bit == 0 {
                self.make_branch(d, prefix, &node.prefix, old, key, leaf_hash)
            } else {
                self.make_branch(d, prefix, key, leaf_hash, &node.prefix, old)
            };
            // `old` is still referenced as a child: do not retire it.
            return new;
        }
        // Key matches the prefix; descend into the child selected by bit `h`.
        let b = bit(key, h);
        let child = if b == 0 { node.left } else { node.right };
        let new_child = self.insert(Some(child), key, value);
        let new_node = if b == 0 {
            BranchNode {
                height: node.height,
                prefix: node.prefix,
                left: new_child,
                right: node.right,
            }
        } else {
            BranchNode {
                height: node.height,
                prefix: node.prefix,
                left: node.left,
                right: new_child,
            }
        };
        let new = self.store_branch(&new_node);
        self.retire(&old, &new);
        new
    }

    fn remove(&mut self, root: Option<[u8; 64]>, key: &[u8; 32]) -> Option<[u8; 64]> {
        let old = root?;
        let bytes = self.load(&old).expect("tree node must exist");
        if let Some((k2, vh2)) = parse_leaf(&bytes) {
            if &k2 == key {
                self.backend.delete(&old);
                self.stats.nodes_deleted += 1;
                self.stats.leaves_removed += 1;
                return None;
            }
            // Different key: unchanged. Keep the node.
            let _ = vh2;
            return Some(old);
        }
        let node = parse_branch(&bytes).expect("valid node encoding");
        let h = node.height as usize;
        if first_prefix_mismatch(key, &node.prefix, h).is_some() {
            return Some(old);
        }
        let b = bit(key, h);
        let child = if b == 0 { node.left } else { node.right };
        let new_child = self.remove(Some(child), key);
        match new_child {
            Some(nh) => {
                let new_node = if b == 0 {
                    BranchNode {
                        height: node.height,
                        prefix: node.prefix,
                        left: nh,
                        right: node.right,
                    }
                } else {
                    BranchNode {
                        height: node.height,
                        prefix: node.prefix,
                        left: node.left,
                        right: nh,
                    }
                };
                let new = self.store_branch(&new_node);
                self.retire(&old, &new);
                Some(new)
            }
            None => {
                // One side emptied: collapse to the surviving child.
                let survivor = if b == 0 { node.right } else { node.left };
                self.backend.delete(&old);
                self.stats.nodes_deleted += 1;
                Some(survivor)
            }
        }
    }
}

/// Apply a set of record changes to the tree and return the new root.
///
/// Repeated changes to the same key collapse with last-write-wins; an empty
/// change set is a no-op returning the current root.
pub fn update<B: NodeBackend>(backend: &mut B, changes: Vec<TreeChange>) -> [u8; 64] {
    let mut collapsed: BTreeMap<[u8; 32], Option<Vec<u8>>> = BTreeMap::new();
    for change in changes {
        collapsed.insert(change.key256, change.value);
    }
    update_collapsed(backend, collapsed)
}

/// Like [`update`] but accepts an already-collapsed change map.
pub fn update_collapsed<B: NodeBackend>(
    backend: &mut B,
    collapsed: BTreeMap<[u8; 32], Option<Vec<u8>>>,
) -> [u8; 64] {
    update_inner(backend, collapsed, None).0
}

/// Insert or replace a single record, returning the new root. Used by streaming
/// rebuilds where records are applied one at a time to bound memory.
pub fn put_record<B: NodeBackend>(
    backend: &mut B,
    root: Option<[u8; 64]>,
    key256: [u8; 32],
    value: &[u8],
) -> [u8; 64] {
    let mut ctx = Ctx {
        backend,
        stats: UpdateStats::default(),
    };
    let root = root.filter(|r| *r != EMPTY_STATE_ROOT);
    ctx.insert(root, &key256, value)
}

/// Apply changes and also return per-operation counters.
pub fn update_with_stats<B: NodeBackend>(
    backend: &mut B,
    changes: Vec<TreeChange>,
) -> ([u8; 64], UpdateStats) {
    let mut collapsed: BTreeMap<[u8; 32], Option<Vec<u8>>> = BTreeMap::new();
    for change in changes {
        collapsed.insert(change.key256, change.value);
    }
    update_inner(backend, collapsed, None)
}

/// Apply an already-collapsed change map with an explicit starting root.
pub fn update_from_root<B: NodeBackend>(
    backend: &mut B,
    root: Option<[u8; 64]>,
    changes: Vec<TreeChange>,
) -> [u8; 64] {
    let mut collapsed: BTreeMap<[u8; 32], Option<Vec<u8>>> = BTreeMap::new();
    for change in changes {
        collapsed.insert(change.key256, change.value);
    }
    update_inner(backend, collapsed, root).0
}

/// Apply an already-collapsed change map with an explicit starting root,
/// returning per-operation counters.
pub fn update_from_root_with_stats<B: NodeBackend>(
    backend: &mut B,
    root: Option<[u8; 64]>,
    changes: Vec<TreeChange>,
) -> ([u8; 64], UpdateStats) {
    let mut collapsed: BTreeMap<[u8; 32], Option<Vec<u8>>> = BTreeMap::new();
    for change in changes {
        collapsed.insert(change.key256, change.value);
    }
    update_inner(backend, collapsed, root)
}

fn update_inner<B: NodeBackend>(
    backend: &mut B,
    collapsed: BTreeMap<[u8; 32], Option<Vec<u8>>>,
    root: Option<[u8; 64]>,
) -> ([u8; 64], UpdateStats) {
    let mut ctx = Ctx {
        backend,
        stats: UpdateStats::default(),
    };
    let mut root = root.filter(|r| *r != EMPTY_STATE_ROOT);
    for (key, value) in collapsed {
        root = match value {
            Some(v) => Some(ctx.insert(root, &key, &v)),
            None => ctx.remove(root, &key),
        };
    }
    let stats = ctx.stats;
    (root.unwrap_or(EMPTY_STATE_ROOT), stats)
}

/// Compute the tree root for a full set of records without persisting.
pub fn dry_run_root(changes: Vec<TreeChange>) -> [u8; 64] {
    let mut backend = MemNodeBackend::default();
    update(&mut backend, changes)
}

/// A single proof step: sibling subtree hash at a branch `height`/`prefix`.
#[derive(Clone, Debug)]
pub struct ProofStep {
    pub height: u16,
    pub prefix: [u8; 32],
    pub sibling: [u8; 64],
    pub right: bool,
}

/// Where a proof walk terminated.
#[derive(Clone, Debug)]
pub enum ProofTerminal {
    /// The tree is empty.
    Empty,
    /// A leaf was found at `key256`'s position (may be a different key for an
    /// absence proof).
    Leaf {
        key256: [u8; 32],
        value_hash: [u8; 64],
    },
    /// The queried key diverged from a branch prefix above that branch, which
    /// is a proof of absence.
    Branch {
        height: u16,
        prefix: [u8; 32],
        left: [u8; 64],
        right: [u8; 64],
    },
}

/// A Merkle proof for a single key: inclusion or absence.
#[derive(Clone, Debug)]
pub struct MerkleProof {
    pub key256: [u8; 32],
    pub terminal: ProofTerminal,
    pub steps: Vec<ProofStep>,
}

impl MerkleProof {
    pub fn present(&self) -> bool {
        matches!(&self.terminal, ProofTerminal::Leaf { key256, .. } if key256 == &self.key256)
    }
}

/// Generate an inclusion or absence proof for `key256` under `root`.
pub fn prove<B: NodeBackend>(backend: &B, root: &[u8; 64], key256: &[u8; 32]) -> MerkleProof {
    let mut steps = Vec::new();
    if *root == EMPTY_STATE_ROOT {
        return MerkleProof {
            key256: *key256,
            terminal: ProofTerminal::Empty,
            steps,
        };
    }
    let mut current = Some(*root);
    while let Some(hash) = current {
        let Some(bytes) = backend.get(&hash) else {
            return MerkleProof {
                key256: *key256,
                terminal: ProofTerminal::Empty,
                steps,
            };
        };
        if let Some((k2, vh2)) = parse_leaf(&bytes) {
            return MerkleProof {
                key256: *key256,
                terminal: ProofTerminal::Leaf {
                    key256: k2,
                    value_hash: vh2,
                },
                steps,
            };
        }
        let node = parse_branch(&bytes).expect("valid node encoding");
        let h = node.height as usize;
        if first_prefix_mismatch(key256, &node.prefix, h).is_some() {
            return MerkleProof {
                key256: *key256,
                terminal: ProofTerminal::Branch {
                    height: node.height,
                    prefix: node.prefix,
                    left: node.left,
                    right: node.right,
                },
                steps,
            };
        }
        let right_taken = bit(key256, h) == 1;
        steps.push(ProofStep {
            height: node.height,
            prefix: node.prefix,
            sibling: if right_taken { node.left } else { node.right },
            right: right_taken,
        });
        current = Some(if right_taken { node.right } else { node.left });
    }
    MerkleProof {
        key256: *key256,
        terminal: ProofTerminal::Empty,
        steps,
    }
}

/// Verify a proof against `expected_root`.
pub fn verify_proof(
    expected_root: &[u8; 64],
    key256: &[u8; 32],
    value: Option<&[u8]>,
    proof: &MerkleProof,
) -> bool {
    if &proof.key256 != key256 {
        return false;
    }
    // Fold the proof steps, checking heights decrease and prefixes match the
    // queried key, so the path is unambiguous.
    let mut last_height: Option<usize> = None;
    let mut fold = |mut current: [u8; 64]| -> Option<[u8; 64]> {
        for step in proof.steps.iter().rev() {
            let h = step.height as usize;
            if let Some(prev) = last_height {
                if h >= prev {
                    return None;
                }
            }
            last_height = Some(h);
            if (bit(key256, h) == 1) != step.right {
                return None;
            }
            if first_prefix_mismatch(key256, &step.prefix, h).is_some() {
                return None;
            }
            current = if step.right {
                hash_branch(step.height, &step.prefix, &step.sibling, &current)
            } else {
                hash_branch(step.height, &step.prefix, &current, &step.sibling)
            };
        }
        Some(current)
    };

    match &proof.terminal {
        ProofTerminal::Empty => {
            value.is_none() && *expected_root == EMPTY_STATE_ROOT && proof.steps.is_empty()
        }
        ProofTerminal::Leaf {
            key256: k,
            value_hash,
        } => {
            match value {
                Some(v) => {
                    if k != key256 || blake3_512(v) != *value_hash {
                        return false;
                    }
                }
                None => {
                    if k == key256 {
                        return false;
                    }
                    // The found leaf must sit on the recorded path.
                    for step in &proof.steps {
                        if (bit(k, step.height as usize) == 1) != step.right {
                            return false;
                        }
                    }
                }
            }
            match fold(leaf_hash_raw(k, value_hash)) {
                Some(root) => root == *expected_root,
                None => false,
            }
        }
        ProofTerminal::Branch {
            height,
            prefix,
            left,
            right,
        } => {
            // Absence only: the key must diverge from this branch's prefix.
            if value.is_some() {
                return false;
            }
            if first_prefix_mismatch(key256, prefix, *height as usize).is_none() {
                return false;
            }
            match fold(hash_branch(*height, prefix, left, right)) {
                Some(root) => root == *expected_root,
                None => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(cf: &str, key: &str, value: &str) -> TreeChange {
        TreeChange::put(cf.as_bytes(), key.as_bytes(), value.as_bytes())
    }

    fn root_of(changes: Vec<TreeChange>) -> ([u8; 64], MemNodeBackend) {
        let mut backend = MemNodeBackend::default();
        let root = update(&mut backend, changes);
        (root, backend)
    }

    #[test]
    fn empty_tree_root_is_sentinel() {
        let mut backend = MemNodeBackend::default();
        assert_eq!(update(&mut backend, vec![]), EMPTY_STATE_ROOT);
        assert!(backend.nodes.is_empty());
    }

    #[test]
    fn single_leaf_root_is_stable_and_non_empty() {
        let (root, backend) = root_of(vec![mk("accounts", "1", "alice")]);
        assert_ne!(root, EMPTY_STATE_ROOT);
        assert_eq!(backend.nodes.len(), 1);
    }

    #[test]
    fn insertion_order_does_not_matter() {
        let changes = vec![
            mk("accounts", "1", "a"),
            mk("accounts", "2", "b"),
            mk("validator_set", "height", "7"),
            mk("contracts", "c", "d"),
        ];
        let (r1, _) = root_of(changes.clone());
        let mut rev = changes;
        rev.reverse();
        let (r2, _) = root_of(rev);
        assert_eq!(r1, r2);
    }

    #[test]
    fn all_insertion_orders_agree_regression() {
        // Regression for the branch-placement bug: a key diverging above an
        // existing branch must yield the same tree regardless of order.
        let base = vec![
            mk("accounts", "alpha", "1"),
            mk("accounts", "beta", "2"),
            mk("accounts", "gamma", "3"),
            mk("accounts", "delta", "4"),
        ];
        let expected = root_of(base.clone()).0;
        let mut permutations = vec![
            vec![3, 2, 1, 0],
            vec![1, 3, 0, 2],
            vec![2, 0, 3, 1],
            vec![0, 1, 2, 3],
        ];
        for perm in permutations.drain(..) {
            let reordered: Vec<TreeChange> = perm.iter().map(|&i| base[i].clone()).collect();
            assert_eq!(root_of(reordered).0, expected, "order {perm:?}");
        }
    }

    #[test]
    fn incremental_matches_full_rebuild() {
        let records: Vec<TreeChange> = (0..512)
            .map(|i| mk("accounts", &i.to_string(), &format!("v{i}")))
            .collect();

        let mut incremental = MemNodeBackend::default();
        let mut root = EMPTY_STATE_ROOT;
        for change in &records {
            root = update_from_root(&mut incremental, Some(root), vec![change.clone()]);
        }

        let mut fresh = MemNodeBackend::default();
        let rebuilt = update(&mut fresh, records);
        assert_eq!(root, rebuilt);
    }

    #[test]
    fn node_count_is_linear_not_depth_times_leaf() {
        let changes: Vec<TreeChange> = (0..1000)
            .map(|i| mk("accounts", &i.to_string(), "v"))
            .collect();
        let (_, backend) = root_of(changes);
        assert!(
            backend.nodes.len() < 4000,
            "expected linear node count, got {}",
            backend.nodes.len()
        );
        assert!(backend.nodes.len() >= 1999, "expected ~2n-1 nodes");
    }

    #[test]
    fn update_then_delete_restores_root() {
        let a = mk("accounts", "1", "a");
        let b = mk("accounts", "2", "b");
        let (root_a, mut backend) = root_of(vec![a.clone()]);
        let root_ab = update_from_root(&mut backend, Some(root_a), vec![b.clone()]);
        assert_ne!(root_a, root_ab);
        let after_delete = update_from_root(
            &mut backend,
            Some(root_ab),
            vec![TreeChange::delete(b"accounts", b"2")],
        );
        assert_eq!(after_delete, root_a);
        assert_eq!(backend.nodes.len(), 1, "collapsed tree keeps one node");
    }

    #[test]
    fn many_delete_reinsert_cycles_stay_canonical() {
        let mut backend = MemNodeBackend::default();
        let mut root = update(
            &mut backend,
            (0..100)
                .map(|i| mk("accounts", &i.to_string(), "x"))
                .collect(),
        );
        let expected = root;
        for _ in 0..5 {
            let deletes: Vec<TreeChange> = (0..100)
                .filter(|i| i % 2 == 0)
                .map(|i| TreeChange::delete(b"accounts", i.to_string().as_bytes()))
                .collect();
            root = update_from_root(&mut backend, Some(root), deletes);
            let reinserts: Vec<TreeChange> = (0..100)
                .filter(|i| i % 2 == 0)
                .map(|i| mk("accounts", &i.to_string(), "x"))
                .collect();
            root = update_from_root(&mut backend, Some(root), reinserts);
            assert_eq!(root, expected);
        }
    }

    #[test]
    fn repeated_key_last_write_wins() {
        let (root1, _) = root_of(vec![
            mk("accounts", "1", "first"),
            mk("accounts", "1", "second"),
        ]);
        let (root2, _) = root_of(vec![mk("accounts", "1", "second")]);
        assert_eq!(root1, root2);
    }

    #[test]
    fn different_values_change_root() {
        let (r1, _) = root_of(vec![mk("accounts", "1", "a")]);
        let (r2, _) = root_of(vec![mk("accounts", "1", "b")]);
        assert_ne!(r1, r2);
    }

    #[test]
    fn column_family_separates_records() {
        let (r1, _) = root_of(vec![mk("accounts", "1", "x")]);
        let (r2, _) = root_of(vec![mk("contract_balances", "1", "x")]);
        assert_ne!(r1, r2);
    }

    #[test]
    fn deletion_of_all_records_returns_to_sentinel() {
        let (root, mut backend) = root_of(vec![mk("accounts", "1", "a"), mk("accounts", "2", "b")]);
        let root = update_from_root(
            &mut backend,
            Some(root),
            vec![
                TreeChange::delete(b"accounts", b"1"),
                TreeChange::delete(b"accounts", b"2"),
            ],
        );
        assert_eq!(root, EMPTY_STATE_ROOT);
        assert!(backend.nodes.is_empty(), "empty tree must persist no nodes");
    }
    #[test]
    fn inclusion_proof_roundtrip() {
        let changes = vec![
            TreeChange::put(b"accounts", b"42", b"payload"),
            mk("accounts", "43", "other"),
            mk("accounts", "44", "third"),
        ];
        let (root, backend) = root_of(changes);
        let key = record_key(b"accounts", b"42");
        let proof = prove(&backend, &root, &key);
        assert!(proof.present());
        assert!(verify_proof(&root, &key, Some(b"payload"), &proof));
        assert!(!verify_proof(&root, &key, Some(b"tampered"), &proof));
    }

    #[test]
    fn absence_proof_roundtrip() {
        let (root, backend) = root_of(vec![mk("accounts", "1", "a"), mk("accounts", "2", "b")]);
        let missing = record_key(b"accounts", b"999");
        let proof = prove(&backend, &root, &missing);
        assert!(!proof.present());
        assert!(verify_proof(&root, &missing, None, &proof));
        assert!(!verify_proof(&root, &missing, Some(b"x"), &proof));
    }

    #[test]
    fn deterministic_across_independent_instances() {
        let records: Vec<TreeChange> = (0..500)
            .map(|i| mk("accounts", &i.to_string(), &format!("v{i}")))
            .collect();
        let mut a = MemNodeBackend::default();
        let mut b = MemNodeBackend::default();
        assert_eq!(update(&mut a, records.clone()), update(&mut b, records));
    }

    /// Simple deterministic PRNG so the randomized test is reproducible.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 11
        }
    }

    /// Differential fuzz: thousands of random insert/delete operations applied
    /// incrementally must always agree with a full rebuild of the same logical
    /// map, and every inclusion/absence proof must verify. This exercises the
    /// collapse/split logic under adversarial orderings.
    #[test]
    fn differential_random_ops_match_rebuild_and_proofs() {
        let mut rng = Lcg(0xC0FFEE);
        let mut backend = MemNodeBackend::default();
        let mut reference: BTreeMap<[u8; 32], Vec<u8>> = BTreeMap::new();
        let mut root = EMPTY_STATE_ROOT;

        for step in 0..4000u32 {
            let idx = rng.next() % 200;
            let key256 = record_key(b"accounts", &idx.to_be_bytes());
            if rng.next() % 3 == 0 {
                // delete
                reference.remove(&key256);
                root = update_from_root(
                    &mut backend,
                    Some(root),
                    vec![TreeChange::delete(b"accounts", &idx.to_be_bytes())],
                );
            } else {
                let value = format!("v{}", rng.next() % 1_000_000).into_bytes();
                reference.insert(key256, value.clone());
                root = update_from_root(
                    &mut backend,
                    Some(root),
                    vec![TreeChange::put(b"accounts", &idx.to_be_bytes(), &value)],
                );
            }

            // Every ~250 ops, cross-check against a full rebuild.
            if step % 250 == 0 {
                let changes: Vec<TreeChange> = reference
                    .iter()
                    .map(|(k, v)| TreeChange {
                        key256: *k,
                        value: Some(v.clone()),
                    })
                    .collect();
                let mut fresh = MemNodeBackend::default();
                let rebuilt = update(&mut fresh, changes);
                assert_eq!(root, rebuilt, "divergence at step {step}");
            }
        }

        // Final full comparison and proof checks.
        let changes: Vec<TreeChange> = reference
            .iter()
            .map(|(k, v)| TreeChange {
                key256: *k,
                value: Some(v.clone()),
            })
            .collect();
        let mut fresh = MemNodeBackend::default();
        assert_eq!(root, update(&mut fresh, changes));

        for lane in 0..300u64 {
            let key256 = record_key(b"accounts", &(lane % 200).to_be_bytes());
            let proof = prove(&backend, &root, &key256);
            match reference.get(&key256) {
                Some(v) => {
                    assert!(proof.present());
                    assert!(verify_proof(&root, &key256, Some(v), &proof));
                }
                None => {
                    assert!(!proof.present());
                    assert!(verify_proof(&root, &key256, None, &proof));
                }
            }
        }
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// Compute the canonical root for a fixed record set; the pinned value below
    /// guards against accidental changes to the root definition.
    fn golden_root() -> [u8; 64] {
        let changes = vec![
            mk("accounts", "1", "a"),
            mk("accounts", "2", "b"),
            mk("accounts", "3", "c"),
            mk("validator_set", "height", "7"),
        ];
        dry_run_root(changes)
    }

    #[test]
    fn golden_root_is_pinned() {
        // Generated once from this implementation. A change here means the
        // consensus root definition changed and MUST bump the protocol version.
        let root = golden_root();
        assert_eq!(to_hex(&root), "877b3f4f3007e27fb4c7aa3a86c44bdf853a36997d2a984a47f9bcc1eced72a02595c33ba82008bebe63ce0a3b7b28d633161014534c5a4cf22ccb44e3b75f66");
        assert_ne!(root, EMPTY_STATE_ROOT);
    }
}
