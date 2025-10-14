# ADR-003: Use sysinfo Crate for Disk Space Information

**Date:** 2025-01-14

## Status

Proposed

## Context

We need to report disk space usage when backups start and complete (GitHub issue #63). This requires querying filesystem information to get total, used, and available disk space for the target backup location.

Initial implementation used direct system calls:
- **Unix/Linux**: `statvfs(2)` via `libc` crate with unsafe FFI
- **Windows**: `GetDiskFreeSpaceEx` via `winapi` crate with unsafe FFI

While this approach works and gives us full control, it has several drawbacks:
- Requires unsafe code blocks with FFI to C libraries
- Platform-specific implementations need separate testing and maintenance
- Limited to disk space only - no path for future system monitoring features
- Manual safety invariants must be carefully documented and maintained

We need to decide whether to keep the hand-rolled implementation or use an existing crate that abstracts these system calls.

## Decision

We will replace the hand-rolled unsafe implementation with the **sysinfo** crate for disk space information.

## Alternatives Considered

1. **Keep hand-rolled implementation**
   - **Pros**: Zero dependencies, full control, minimal code
   - **Cons**: Requires unsafe code, platform-specific, limited to disk space only

2. **fs2 crate**
   - **Pros**: Focused on filesystem utilities, lightweight
   - **Cons**: Less actively maintained, less comprehensive documentation, limited to filesystem operations

3. **sys-info crate**
   - **Pros**: Provides disk space info
   - **Cons**: Last updated October 2021, appears unmaintained, superseded by sysinfo

4. **sysinfo crate** (chosen)
   - **Pros**:
     - Very actively maintained (minimum rustc 1.88, current with latest Rust)
     - Comprehensive cross-platform support (Linux, macOS, Windows, BSD, illumos, Solaris)
     - Well-documented with extensive examples
     - Provides safe Rust API - no unsafe code needed
     - Opens door for future monitoring features (CPU, memory, process info)
     - Popular and widely used in the Rust ecosystem
   - **Cons**:
     - Larger dependency (includes CPU, memory, network monitoring)
     - Slightly heavier than fs2 (though only used features are compiled)

## Rationale

The sysinfo crate is the best choice because:

1. **Better cross-platform support**: Actively maintained with support for all major platforms including less common ones (BSD, illumos, Solaris)

2. **Eliminates unsafe code**: All platform-specific FFI and unsafe operations are handled by a well-tested library

3. **Future extensibility**: We may want to add performance monitoring in the future:
   - CPU usage during backup operations
   - Memory consumption tracking
   - I/O bandwidth monitoring
   - Process-level statistics

   Having sysinfo already in place makes these additions trivial.

4. **Maintenance burden**: Rather than maintaining platform-specific unsafe code, we delegate this to a crate with many contributors and users who will catch platform-specific issues

5. **Safety**: The sysinfo crate has been battle-tested across many projects, reducing the risk of platform-specific bugs or safety issues

## Consequences

### Positive

- **No unsafe code in our codebase** for system information queries
- **Reduced maintenance burden** - platform-specific code is handled by sysinfo
- **Future-proof** - easy to add CPU/memory/process monitoring features
- **Better testing** - sysinfo is tested across many platforms and scenarios
- **Cross-platform confidence** - less likely to hit platform-specific edge cases

### Negative

- **Larger dependency tree** - sysinfo includes functionality we don't currently use
- **Slightly larger binary** - though minimal given Rust's dead code elimination
- **External dependency** - we rely on sysinfo maintainers (though it's well-maintained)

### Neutral

- Need to map paths to disks (find which disk mount point contains our backup path)
- API differences from hand-rolled implementation (migration effort)

## Implementation Notes

### API Overview

The sysinfo crate provides:
- `Disks::new_with_refreshed_list()` to enumerate all disk mounts
- `Disk::available_space()` - available space in bytes
- `Disk::total_space()` - total space in bytes
- `Disk::mount_point()` - mount point path

### Finding Disk Space for Arbitrary Paths

The challenge is mapping an arbitrary backup path to the correct disk. This is non-trivial because:
- Systems can have nested mount points
- Paths may contain symlinks
- Paths may be relative
- The target path may not exist yet

**Algorithm:**

1. Canonicalize the path to resolve symlinks and make it absolute
2. Get all disk mount points
3. Filter disks whose mount point is a prefix of the canonical path
4. Select the disk with the **longest matching mount point**

**Example of nested mounts:**
```
Mounts:
  /                      (root filesystem)
  /home                  (separate partition)
  /home/tim/external     (external drive)

Target: /home/tim/backups/my-backup
  ✓ Matches: /
  ✓ Matches: /home          <- Longest match (correct!)
  ✗ Does not match: /home/tim/external
```

**Implementation sketch:**

```rust
pub fn get_disk_space(path: &Path) -> io::Result<DiskSpace> {
    let mut disks = Disks::new_with_refreshed_list();

    // Canonicalize to handle symlinks/relative paths
    let canonical_path = path.canonicalize()?;

    // Find disk with longest matching mount point
    let disk = disks
        .iter()
        .filter(|disk| canonical_path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::NotFound,
            format!("No disk found for path: {:?}", path)
        ))?;

    Ok(DiskSpace::new(
        disk.total_space(),
        disk.available_space()
    ))
}
```

### Edge Cases to Handle

1. **Symlinks**: Use `path.canonicalize()` to resolve to the real path
2. **Relative paths**: `canonicalize()` also makes paths absolute
3. **Non-existent paths**: If target doesn't exist yet, we may need to check parent directories
4. **Windows drive letters**: Works naturally - `C:\` is the mount point
5. **Network mounts**: May not appear in disk list (could return error or use parent mount)
6. **Bind mounts (Linux)**: Multiple paths pointing to same filesystem - canonicalize helps
7. **Empty mount point list**: Should not happen on working systems, but handle gracefully

## References

- GitHub issue #63: Report on disk space when backup completed
- sysinfo crate: https://crates.io/crates/sysinfo
- sysinfo documentation: https://docs.rs/sysinfo/
