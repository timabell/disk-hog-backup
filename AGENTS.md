# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Build, Test, and Lint Commands

```bash
cargo build              # Build the project
cargo test               # Run all tests
cargo test <test_name>   # Run a single test
./lint.sh                # Run all linting (fmt, clippy, license check, yamllint)
cargo run -- --source <dir> --destination <dir>  # Run the backup tool
```

## Architecture Overview

Disk Hog Backup is a CLI backup tool written in Rust that creates backups as normal folders with hardlinks for unchanged files. It uses MD5 checksums for file integrity verification.

### Module Structure

- `src/main.rs` - CLI entry point using clap for argument parsing
- `src/backup.rs` - High-level backup orchestration
- `src/dhcopy/` - Core copying logic
  - `copy_folder.rs` - Recursive folder traversal and auto-delete logic
  - `streaming_copy.rs` - Multi-threaded file copy pipeline
  - `ignore_patterns.rs` - `.dhbignore` file parsing
- `src/backup_sets/` - Backup set management
  - `backup_set.rs` - Finding and managing backup sets
  - `set_manager.rs` - Weighted-random deletion for auto-delete
  - `md5_store.rs` - MD5 hash file storage and lookup
  - `backup_stats.rs` - Statistics collection and reporting
  - `set_namer.rs` - Timestamp-based naming (dhb-set-YYYYMMDD-HHMMSS)
- `src/disk_space.rs` - Disk space queries using sysinfo crate

### Threading Model

File operations use a 4-thread pipeline per file (see `doc/threading.md`):
1. **Main thread** - Orchestrates pipeline, compares MD5 hashes, decides on hardlinks
2. **Reader thread** - Reads 256KB chunks from source
3. **Hasher thread** - Computes MD5 incrementally
4. **Writer thread** - Writes chunks to destination

Data flows through crossbeam bounded channels. If MD5 matches the previous backup, the writer is cancelled and a hardlink is created instead.

### Key Optimization

Before full file comparison, the code checks mtime+size. If both match the previous backup, it skips MD5 computation entirely and creates a hardlink immediately.

## Testing Approach

Tests are outside-in (end-to-end) using `assert_cmd` to run the binary. Tests use `tempfile` for temporary directories. See `tests/end_to_end_tests.rs`.

For background on this testing philosophy, see: https://0x5.uk/2024/03/27/why-do-automated-tests-matter/

## Commit Message Prefixes

From `.github/cliff.toml`, these prefixes trigger automatic releases:
- `security:` - Security fixes
- `feat:` - New features
- `fix:` - Bug fixes
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks
- `refactor:` - Code refactoring
- `doc:` - Documentation changes

## Release Process

Releases are automatic on pushes to main when commits contain release-worthy prefixes (above).

- **Version bumping**: Defaults to patch. Add `bump: minor` or `bump: major` as a commit footer to control.
- **Preview release notes**: `./release-notes.sh --preview`
- **View latest release notes**: `./release-notes.sh`

## Design Documents

- `doc/adr/` - Architecture Decision Records
- `doc/threading.md` - Threading pipeline diagrams
