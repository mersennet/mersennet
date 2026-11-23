//! Offline compaction of a sled state directory.
//!
//! sled 0.34 frees a blob file (any record over ~128 KiB, which is most leaves
//! of the `blocks` tree) only through bookkeeping it holds in memory until the
//! segment that replaced it is sealed. A restart drops that bookkeeping, and
//! the old blob file then stays on disk with nothing pointing at it, for good.
//! Months of restarts left the archive nodes (and the snapshot every node is
//! restored from) at ~130 GB, about half of it such files; pruning removes
//! live records and cannot touch them.
//!
//! The fix is to copy what is live into a fresh directory: every tree, entry
//! by entry. With `keep_blocks` the copy also drops the history a pruned node
//! would delete (blocks, their hash/tx index entries and state roots below the
//! floor) and records the floor, so a validator goes straight to its pruned
//! size without the hours-long first prune pass.
//!
//! The node must be stopped (sled's file lock refuses a second opener). The
//! source is never modified: the caller swaps directories after `compact_state`
//! returns, and keeps the old one until the node has started on the new one.

use anyhow::{Context, Result, bail};
use std::ops::Bound;
use std::path::Path;

use crate::state::{decode_be_u64, decode_tx_location};

/// Files and directories sled itself owns inside a state directory; anything
/// else there (the shielded-pool redb file, for one) is copied as-is.
fn is_sled_file(name: &str) -> bool {
    name == "conf" || name == "db" || name == "blobs" || name.starts_with("snap.")
}

#[derive(Debug, Default, Clone)]
pub struct TreeReport {
    pub name: String,
    pub copied: u64,
    pub skipped: u64,
}

#[derive(Debug, Default, Clone)]
pub struct CompactReport {
    pub latest_height: Option<u64>,
    /// The block floor the copy carries (None: full history kept).
    pub block_floor: Option<u64>,
    pub trees: Vec<TreeReport>,
    pub other_files: Vec<String>,
}

const BATCH_ENTRIES: usize = 4096;
const BATCH_BYTES: usize = 32 << 20;
/// Entries / bytes read per iterator before a fresh one (see the copy loop).
const CHUNK_ENTRIES: usize = 50_000;
const CHUNK_BYTES: usize = 64 << 20;
const REOPEN_BYTES: usize = 256 << 20;

/// The process's peak resident memory (VmHWM), for the progress lines.
fn peak_rss() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM")).map(|l| l[6..].trim().to_string()))
        .unwrap_or_else(|| "?".into())
}

/// Copy the live contents of the sled store at `src` into a new store at
/// `dst` (which must not exist). With `keep_blocks = Some(n)`, blocks below
/// `latest_height - n` (and never below an existing floor) are left out.
/// Every tree is verified after the copy: entry count and CRC32 of the new
/// tree must match what was read from the old one.
pub fn compact_state(
    src: &Path,
    dst: &Path,
    keep_blocks: Option<u64>,
    mut progress: impl FnMut(&str),
) -> Result<CompactReport> {
    if dst.exists() {
        bail!("{} already exists; remove it first", dst.display());
    }
    if !src.join("db").is_file() || !src.join("conf").is_file() {
        bail!("{} is not a sled state directory (no db/conf)", src.display());
    }
    let cache = 64u64 << 20;
    // sled 0.34 keeps what it has read reachable whatever its cache budget
    // (~0.7 bytes of memory per byte read, measured), so the old store is
    // closed and opened again every REOPEN_BYTES (MERSENNET_COMPACT_REOPEN_MB
    // overrides it): on a 2.4 GB test store the peak fell from 3.8 to 1.3 GB.
    let open_old = || {
        sled::Config::new()
            .path(src)
            .cache_capacity(cache)
            .open()
            .with_context(|| format!("opening {} (is the node still running?)", src.display()))
    };
    let mut old = open_old()?;
    let (latest, old_floor) = {
        let meta = old.open_tree("height_meta")?;
        if meta.get("commit_in_progress")?.is_some() {
            // Copied as-is; the node finishes or repairs the commit at startup
            // exactly as it would have on the old directory.
            progress("note: an interrupted-commit marker is set; it is copied unchanged");
        }
        (
            meta.get("latest_height")?.and_then(|v| decode_be_u64(&v)),
            meta.get("block_floor")?.and_then(|v| decode_be_u64(&v)),
        )
    };
    let floor = match (keep_blocks, latest) {
        (Some(keep), Some(h)) => {
            let keep = keep.max(crate::engine::MIN_PRUNE_KEEP_BLOCKS);
            Some(h.saturating_sub(keep).max(old_floor.unwrap_or(0)))
        }
        (Some(_), None) => bail!("no latest_height in {}: refusing to prune an empty or foreign store", src.display()),
        (None, _) => old_floor,
    };
    let prune_below = if keep_blocks.is_some() { floor.unwrap_or(0) } else { 0 };

    let reopen_bytes = std::env::var("MERSENNET_COMPACT_REOPEN_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|mb| mb << 20)
        .unwrap_or(REOPEN_BYTES);
    let new = sled::Config::new()
        .path(dst)
        .cache_capacity(cache)
        .create_new(true)
        .open()
        .with_context(|| format!("creating {}", dst.display()))?;

    let mut report = CompactReport { latest_height: latest, block_floor: floor, ..Default::default() };
    let mut read_since_open = 0usize;
    for name in old.tree_names() {
        let label = String::from_utf8_lossy(&name).into_owned();
        let to = new.open_tree(&name)?;
        let mut hasher = crc32fast::Hasher::new();
        let mut tr = TreeReport { name: label.clone(), ..Default::default() };
        let mut batch = sled::Batch::default();
        let (mut n, mut bytes) = (0usize, 0usize);
        // Read in chunks, each with a fresh iterator: a single iterator over a
        // 100 GB tree kept everything it had visited reachable and the process
        // grew past 12 GB (OOM on the release test). Between chunks the new
        // store is flushed so its dirty pages can go too.
        let mut lower: Bound<Vec<u8>> = if label == "blocks" && prune_below > 0 {
            Bound::Included(prune_below.to_be_bytes().to_vec())
        } else {
            Bound::Unbounded
        };
        let mut last_note = std::time::Instant::now();
        loop {
            let (mut chunk_entries, mut chunk_bytes) = (0usize, 0usize);
            let mut last_key: Option<Vec<u8>> = None;
            let mut exhausted = true;
            let from = old.open_tree(&name)?;
            for item in from.range::<Vec<u8>, _>((lower.clone(), Bound::Unbounded)) {
                let (k, v) = item?;
                chunk_entries += 1;
                chunk_bytes += k.len() + v.len();
                last_key = Some(k.to_vec());
                if prune_below > 0 && !keep_entry(&label, &k, &v, prune_below) {
                    tr.skipped += 1;
                } else {
                    hasher.update(&k);
                    hasher.update(&v);
                    bytes += k.len() + v.len();
                    batch.insert(k, v);
                    n += 1;
                    tr.copied += 1;
                    if n >= BATCH_ENTRIES || bytes >= BATCH_BYTES {
                        to.apply_batch(std::mem::take(&mut batch))?;
                        n = 0;
                        bytes = 0;
                    }
                }
                if chunk_entries >= CHUNK_ENTRIES || chunk_bytes >= CHUNK_BYTES {
                    exhausted = false;
                    break;
                }
            }
            drop(from);
            to.apply_batch(std::mem::take(&mut batch))?;
            n = 0;
            bytes = 0;
            new.flush()?;
            read_since_open += chunk_bytes;
            if read_since_open >= reopen_bytes {
                // Close first: sled's file lock refuses a second opener.
                drop(old);
                old = open_old()?;
                read_since_open = 0;
            }
            if last_note.elapsed().as_secs() >= 30 {
                progress(&format!("  {label}: {} entries copied (memory peak {})", tr.copied, peak_rss()));
                last_note = std::time::Instant::now();
            }
            match last_key {
                Some(k) if !exhausted => lower = Bound::Excluded(k),
                _ => break,
            }
        }
        to.apply_batch(batch)?;
        if label == "height_meta" && prune_below > 0 {
            to.insert("block_floor", &prune_below.to_be_bytes())?;
            hasher.update(b"block_floor");
            hasher.update(&prune_below.to_be_bytes());
            if old_floor.is_none() {
                tr.copied += 1;
            }
        }
        new.flush()?;
        // Read the new tree back: what sled stores must be what we read.
        let mut check = crc32fast::Hasher::new();
        let mut count = 0u64;
        let mut lower: Bound<Vec<u8>> = Bound::Unbounded;
        loop {
            let (mut chunk, mut last_key, mut exhausted) = (0usize, None, true);
            for item in to.range::<Vec<u8>, _>((lower.clone(), Bound::Unbounded)) {
                let (k, v) = item?;
                check.update(&k);
                check.update(&v);
                count += 1;
                chunk += k.len() + v.len();
                last_key = Some(k.to_vec());
                if chunk >= CHUNK_BYTES {
                    exhausted = false;
                    break;
                }
            }
            match last_key {
                Some(k) if !exhausted => lower = Bound::Excluded(k),
                _ => break,
            }
        }
        let want = hasher.finalize();
        if label == "height_meta" && prune_below > 0 {
            // block_floor was inserted at its sorted position, not appended:
            // compare the entry count here and the bytes key by key below.
            if count != tr.copied {
                bail!("height_meta: {count} entries in the copy, {} expected", tr.copied);
            }
            for item in old.open_tree(&name)?.iter() {
                let (k, v) = item?;
                if &k[..] == b"block_floor" {
                    continue;
                }
                if to.get(&k)?.as_deref() != Some(&v[..]) {
                    bail!("height_meta: entry {:?} differs in the copy", String::from_utf8_lossy(&k));
                }
            }
        } else if count != tr.copied || check.finalize() != want {
            bail!("{label}: the copy does not match ({count} entries read back, {} written)", tr.copied);
        }
        progress(&format!(
            "  {label}: {} entries{} (memory peak {})",
            tr.copied,
            if tr.skipped > 0 { format!(", {} below the floor left out", tr.skipped) } else { String::new() },
            peak_rss()
        ));
        report.trees.push(tr);
    }
    new.flush()?;
    drop(new);
    drop(old);

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_sled_file(&name) {
            continue;
        }
        copy_recursive(&entry.path(), &dst.join(&name))?;
        report.other_files.push(name);
    }
    Ok(report)
}

/// For a pruned copy: does this entry belong to history at or above `floor`?
/// Entries whose layout is not recognised are kept.
fn keep_entry(tree: &str, k: &[u8], v: &[u8], floor: u64) -> bool {
    match tree {
        // `blocks` is read from the floor on; nothing below reaches here.
        "pruning" => decode_be_u64(k).is_none_or(|h| h >= floor),
        "history_index" => match k.first() {
            Some(b'b') if k.len() == 33 => decode_be_u64(v).is_none_or(|h| h >= floor),
            Some(b't') if k.len() == 33 => decode_tx_location(v).is_none_or(|(h, _)| h >= floor),
            _ => true,
        },
        _ => true,
    }
}

fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
    let md = std::fs::symlink_metadata(from)?;
    if md.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_recursive(&e.path(), &to.join(e.file_name()))?;
        }
    } else if md.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
    } else {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{history_block_key, history_tx_key, history_tx_value};

    fn fill(dir: &Path, blocks: u64) {
        let db = sled::Config::new().path(dir).open().unwrap();
        let b = db.open_tree("blocks").unwrap();
        let idx = db.open_tree("history_index").unwrap();
        let roots = db.open_tree("pruning").unwrap();
        let meta = db.open_tree("height_meta").unwrap();
        let acc = db.open_tree("accounts").unwrap();
        for h in 0..blocks {
            b.insert(h.to_be_bytes(), vec![(h % 251) as u8; 200_000]).unwrap(); // blob-sized leaves
            let mut hash = [0u8; 32];
            hash[..8].copy_from_slice(&h.to_be_bytes());
            idx.insert(history_block_key(hash.into()), &h.to_be_bytes()).unwrap();
            hash[8] = 1;
            idx.insert(history_tx_key(hash.into()), &history_tx_value(h, 0)).unwrap();
            roots.insert(h.to_be_bytes(), &[7u8; 32]).unwrap();
        }
        for i in 0..500u64 {
            acc.insert(i.to_be_bytes(), vec![i as u8; 100]).unwrap();
        }
        db.insert(b"default-key", b"v").unwrap();
        meta.insert("latest_height", &(blocks - 1).to_be_bytes()).unwrap();
        meta.insert("consensus_set", b"set").unwrap();
        db.flush().unwrap();
    }

    #[test]
    fn full_copy_keeps_every_entry() {
        let t = tempfile::tempdir().unwrap();
        let (src, dst) = (t.path().join("state"), t.path().join("state.compact"));
        fill(&src, 120);
        std::fs::write(src.join("mersennet_shielded.redb"), b"redb").unwrap();
        let r = compact_state(&src, &dst, None, |_| {}).unwrap();
        assert_eq!(r.block_floor, None);
        assert_eq!(r.other_files, vec!["mersennet_shielded.redb".to_string()]);
        let (a, b) = (sled::open(&src).unwrap(), sled::open(&dst).unwrap());
        for name in a.tree_names() {
            assert_eq!(
                a.open_tree(&name).unwrap().checksum().unwrap(),
                b.open_tree(&name).unwrap().checksum().unwrap(),
                "{}",
                String::from_utf8_lossy(&name)
            );
        }
        assert_eq!(b.get(b"default-key").unwrap().as_deref(), Some(&b"v"[..]));
        assert_eq!(std::fs::read(dst.join("mersennet_shielded.redb")).unwrap(), b"redb");
    }

    #[test]
    fn pruned_copy_drops_history_below_the_floor() {
        let t = tempfile::tempdir().unwrap();
        let (src, dst) = (t.path().join("state"), t.path().join("state.compact"));
        let blocks = crate::engine::MIN_PRUNE_KEEP_BLOCKS / 100 + 50; // small: keep is raised to the minimum
        fill(&src, blocks);
        // A keep below the minimum is raised to it, so nothing is dropped here...
        let r = compact_state(&src, &dst, Some(10), |_| {}).unwrap();
        assert_eq!(r.block_floor, Some(0));
        std::fs::remove_dir_all(&dst).unwrap();
        // ...and with an existing floor the copy starts there.
        {
            let db = sled::open(&src).unwrap();
            db.open_tree("height_meta").unwrap().insert("block_floor", &30u64.to_be_bytes()).unwrap();
            db.flush().unwrap();
        }
        let r = compact_state(&src, &dst, Some(10), |_| {}).unwrap();
        assert_eq!(r.block_floor, Some(30));
        let db = sled::open(&dst).unwrap();
        let b = db.open_tree("blocks").unwrap();
        assert_eq!(b.len() as u64, blocks - 30);
        assert!(b.get(29u64.to_be_bytes()).unwrap().is_none());
        assert!(b.get(30u64.to_be_bytes()).unwrap().is_some());
        let idx = db.open_tree("history_index").unwrap();
        assert_eq!(idx.len() as u64, 2 * (blocks - 30));
        assert_eq!(db.open_tree("pruning").unwrap().len() as u64, blocks - 30);
        let meta = db.open_tree("height_meta").unwrap();
        assert_eq!(meta.get("block_floor").unwrap().and_then(|v| decode_be_u64(&v)), Some(30));
        assert_eq!(meta.get("consensus_set").unwrap().as_deref(), Some(&b"set"[..]));
        assert_eq!(db.open_tree("accounts").unwrap().len(), 500);
    }

    #[test]
    fn refuses_an_existing_destination_and_a_running_node() {
        let t = tempfile::tempdir().unwrap();
        let (src, dst) = (t.path().join("state"), t.path().join("state.compact"));
        fill(&src, 20);
        std::fs::create_dir(&dst).unwrap();
        assert!(compact_state(&src, &dst, None, |_| {}).is_err());
        std::fs::remove_dir(&dst).unwrap();
        let _running = sled::open(&src).unwrap(); // holds sled's lock like a live node
        assert!(compact_state(&src, &dst, None, |_| {}).is_err());
    }
}
