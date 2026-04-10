# ADR-005: Sparse File Preservation During Backup

**Date:** 2026-04-10

## Status

Proposed

## Context

Sparse files are files containing "holes" - regions that have never been written and don't consume disk space. The filesystem returns zeros when reading these regions, but no blocks are allocated. Common examples include VM disk images, database files, and Android system images.

The current streaming copy implementation reads all bytes (including holes as zeros) and writes them as real data. This causes sparse files to expand to their full apparent size in the backup:

- Source: `system.img` apparent=2.5GB, actual=979MB (sparse)
- Backup: `system.img` apparent=2.5GB, actual=2.5GB (expanded)

This wastes significant disk space and defeats one of the benefits of sparse files.

Related: https://github.com/timabell/disk-hog-backup/issues/67

## Decision

We will extend the streaming pipeline to preserve sparse files by transforming the data sent to the writer thread. The reader will detect zero-filled regions and send write operations rather than raw bytes.

### WriteOp Enum

Replace the writer channel type:

```rust
// Before: writer receives raw chunks
Receiver<Arc<Vec<u8>>>

// After: writer receives operations
enum WriteOp {
    Data(Arc<Vec<u8>>),  // Non-zero data to write
    Hole(usize),         // Bytes to seek past (creates hole)
}

Receiver<WriteOp>
```

### Zero Detection in Reader

The reader scans each chunk in 4KB blocks (matching typical filesystem block size) and emits appropriate `WriteOp` messages:

```rust
const SPARSE_BLOCK_SIZE: usize = 4096;

fn is_zero_block(block: &[u8]) -> bool {
    block.iter().all(|&b| b == 0)
}

// For each chunk, emit WriteOps for contiguous regions
for block in chunk.chunks(SPARSE_BLOCK_SIZE) {
    if is_zero_block(block) {
        // Accumulate hole size, emit when transitioning to data
    } else {
        // Accumulate data, emit when transitioning to hole
    }
}
```

Contiguous regions are coalesced before sending to minimize channel overhead:
- `[64KB zeros][128KB data][64KB zeros]` → `Hole(64KB), Data(128KB), Hole(64KB)`

### Simple Writer

The writer becomes a straightforward state machine:

```rust
for op in rx {
    match op {
        WriteOp::Data(bytes) => file.write_all(&bytes)?,
        WriteOp::Hole(n) => {
            file.seek(SeekFrom::Current(n as i64))?;
        }
    }
}
```

### Trailing Holes

If a file ends with zeros, seeking past them doesn't establish the file size. After the pipeline completes, the orchestrator calls `file.set_len(source_size)` to ensure correct file size. The orchestrator already has the source metadata, so this adds no complexity.

### Hasher Unchanged

The hasher channel continues to receive full chunks with all bytes (including zeros). This ensures MD5 computation remains correct for hardlink detection.

```
Reader → [Arc<Vec<u8>>] → Hasher (unchanged)
       ↘ [WriteOp]      → Writer (sparse-aware)
```

## Alternatives Considered

1. **Scan in Writer Thread**

   Writer receives raw chunks and scans for zeros before writing.

   **Downside:** Adds complex scanning logic to writer, violating single-responsibility. The reader already has the data in memory - scanning there is essentially free.

2. **SEEK_HOLE/SEEK_DATA System Calls**

   Use Linux-specific syscalls to query hole locations in the source file, then read only data regions.

   **Downside:** Platform-specific (Linux only, partial macOS support, different Windows API). More complex implementation for modest benefit over zero detection.

3. **Post-Process with fallocate Punch Hole**

   Write file normally, then use `fallocate(FALLOC_FL_PUNCH_HOLE)` to create holes.

   **Downside:** Requires writing all data first (wasted I/O), then reading metadata to find zeros, then punching holes. Double the work. Linux 3.14+ only.

4. **Separate Sparsifier Thread**

   Insert a transform thread between reader and writer.

   **Downside:** Extra thread coordination, more channels, increased complexity. The reader can do this work inline with negligible overhead.

## Rationale

The `WriteOp` approach:

- **Maintains pipeline architecture:** Data flows through channels, each thread has a single responsibility.
- **Scanning is free:** Reader already has chunks in memory. Checking for zeros is a memory-bound operation that adds negligible overhead compared to disk I/O.
- **Writer stays simple:** Match on enum, write or seek. No scanning logic.
- **Coalesced operations:** Contiguous regions become single messages, minimizing channel overhead.
- **4KB granularity:** Matches filesystem block size. Holes smaller than this don't save space anyway.
- **Portable:** Works on any filesystem that supports sparse files via seek. Gracefully degrades (no space savings) on filesystems that don't.

## Consequences

### Performance

- **Disk space:** Sparse files retain their space efficiency in backups.
- **Write I/O:** Reduced for sparse files (holes aren't written).
- **CPU:** Negligible overhead for zero detection (memory-bound, data already in cache).
- **Channel overhead:** Slightly more messages for files with many hole/data transitions, but coalescing minimizes this.

### Memory Accounting

The global memory counter tracks bytes in flight. `WriteOp::Hole(n)` consumes no memory (just a size), so accounting becomes slightly more accurate.

### Arc Sharing (Future Optimization)

A future optimization could wrap the existing `Arc<Vec<u8>>` in `WriteOp::Data` when a chunk is entirely non-zero, avoiding allocation. Currently, `chunk_to_write_ops` always allocates new `Vec`s for data regions. Most chunks in typical files are all-data, so this optimization could reduce allocation overhead.

### Statistics

New metrics tracked:
- `sparse_holes_count`: Number of hole regions detected
- `sparse_hole_bytes`: Total bytes saved by sparse holes (not written)

### Platform Considerations

- **Linux/macOS:** Seeking past regions creates holes automatically on supporting filesystems (ext4, APFS, etc.).
- **Windows NTFS:** Sparse files require explicit marking via `DeviceIoControl(FSCTL_SET_SPARSE)` before seeks create holes. Initial implementation may skip sparse optimization on Windows, with support added in a follow-up.

### Design Choice: Optimize Destination

We detect zeros in the data being read, not whether the source file is sparse. A dense file containing zeros will become sparse in the backup. This is intentional - we optimize for destination space, not for preserving source on-disk representation. The file content is byte-for-byte identical either way.

### Interaction with Existing Optimizations

- **mtime+size hardlink:** Unchanged. If both match, we skip the pipeline entirely and hardlink.
- **MD5 cancellation:** Unchanged. Cancel flag still works; remaining `WriteOp`s are drained to maintain correct memory accounting.
- **Hardlinks between files:** If two files have identical content (including zeros), they get the same MD5 and hardlink together. The on-disk representation depends on whichever was written first.

## Testing

End-to-end tests will:

1. Create a sparse file using seek (not writing zeros)
2. Run backup
3. Verify backup file is sparse: `st_blocks * 512 < st_size`
4. Verify content matches via MD5

Also test:
- File entirely of zeros (becomes entirely sparse)
- File with trailing zeros (trailing hole + correct size)
- File with no zeros (written normally, no regression)
- Mixed file with multiple hole/data regions
