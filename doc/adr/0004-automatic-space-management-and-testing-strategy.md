# ADR-004: Automatic Space Management and Testing Strategy

**Date:** 2025-01-06

## Status

Proposed

## Context

When backing up to external drives or limited storage, backups can fail when the disk runs out of space. The current implementation will encounter IO errors when attempting to write files to a full disk, similar to BackInTime's behavior. This requires manual intervention to free up space and retry the backup.

GitHub issue #11 requests automatic removal of old backups when disk space runs low, allowing backups to complete successfully by proactively managing available space.

### Key Requirements from Issue #11

1. **Proactive space detection**: Detect insufficient space before attempting to write, rather than handling IO errors after the fact
2. **Delete complete backup sets only**: Never leave partial/incomplete backups
3. **Weighted deletion strategy**: Consider backup age and importance (initially implementing oldest-first)
4. **Preserve hard-link targets**: Deleting the last previous backup set would eliminate hard-link targets, forcing full re-copies
5. **Opt-in behavior**: Use `--auto-delete` flag to prevent surprise deletions

### Testing Challenge

The core challenge is that this feature's behavior fundamentally depends on actual disk space availability. Traditional end-to-end tests would need to:
- Fill an entire disk to trigger low-space conditions
- Work with realistic backup sizes (potentially GBs)
- Handle unpredictable disk states on test machines

This makes the feature difficult to test in a deterministic, fast, and reliable way.

## Decision

We will implement automatic space management using a **layered testing approach** that separates pure logic from system interaction, enabling comprehensive testing without requiring actual disk space manipulation.

### Architecture Decision

The implementation will use **dependency injection** and **trait abstraction** to separate disk space checking from backup logic:

```rust
// Abstraction for testability
trait SpaceChecker {
    fn get_available_space(&self, path: &Path) -> io::Result<u64>;
    fn get_total_space(&self, path: &Path) -> io::Result<u64>;
}

// Production implementation using sysinfo
struct RealSpaceChecker;

// Test implementation returning controlled values
struct MockSpaceChecker {
    available: u64,
    total: u64,
}
```

### Testing Strategy: Three-Layer Approach

#### Layer 1: Pure Logic Unit Tests (Fast, Deterministic)

Test backup set management logic without any disk operations:

**Module**: `backup_sets/set_manager.rs`
- `list_backup_sets()` - enumerate existing sets with metadata
- `calculate_set_size()` - determine size of a backup set
- `select_sets_to_delete(sets, space_needed, space_available)` - decide what to delete
- `delete_backup_set()` - remove a specific set

**Tests**:
```rust
#[test]
fn test_select_oldest_sets_for_deletion() {
    let sets = vec![
        BackupSet { name: "set-1", size: 1000, created: time1 },
        BackupSet { name: "set-2", size: 2000, created: time2 },
        BackupSet { name: "set-3", size: 1500, created: time3 },
    ];
    let result = select_sets_to_delete(&sets, 2500, 1000);
    assert_eq!(result, vec!["set-1", "set-2"]); // oldest first, preserve at least 1
}

#[test]
fn test_preserve_at_least_one_set() {
    let sets = vec![
        BackupSet { name: "set-1", size: 5000, created: time1 },
    ];
    let result = select_sets_to_delete(&sets, 10000, 1000);
    assert_eq!(result, vec![]); // Never delete the last set
}
```

#### Layer 2: Integration Tests with Small Files (Moderate Speed, Deterministic)

Test the orchestration with real files but small sizes:

```rust
#[test]
fn test_auto_delete_with_mock_space_checker() {
    let source = create_tmp_folder("source")?;
    let dest = create_tmp_folder("dest")?;

    // Create 3 existing backup sets (small but trackable - 1KB each)
    create_test_backup_set(&dest, "dhb-set-20240101-000000", 1024)?;
    create_test_backup_set(&dest, "dhb-set-20240102-000000", 2048)?;
    create_test_backup_set(&dest, "dhb-set-20240103-000000", 1024)?;

    // Create source files that we'll back up (4KB)
    create_test_file(&source, "file.dat", 4096)?;

    // Inject mock space checker that reports only 3KB free
    let mock_checker = MockSpaceChecker {
        available: 3 * 1024,
        total: 100 * 1024,
    };

    // Run backup with auto-delete enabled
    run_backup_with_checker(&source, &dest, true, mock_checker)?;

    // Verify: oldest set deleted, others preserved
    assert!(!Path::new(&dest).join("dhb-set-20240101-000000").exists());
    assert!(Path::new(&dest).join("dhb-set-20240102-000000").exists());
    assert!(Path::new(&dest).join("dhb-set-20240103-000000").exists());
}
```

**Key insight**: We test with KB-sized files, not GB-sized files. The logic is the same regardless of scale.

#### Layer 3: End-to-End Tests (Slower, Real System)

Test with real space checking for integration confidence:

```rust
#[test]
fn test_auto_delete_integration() {
    // Use real disk space but small test files
    // This verifies the full integration works with actual sysinfo
    let source = create_tmp_folder("source")?;
    let dest = create_tmp_folder("dest")?;

    // Create multiple existing sets
    for i in 0..5 {
        create_test_backup_set(&dest, &format!("dhb-set-2024010{}-000000", i), 512)?;
    }

    // Backup with auto-delete
    disk_hog_backup_cmd()
        .arg("--source").arg(&source)
        .arg("--destination").arg(&dest)
        .arg("--auto-delete")
        .assert()
        .success();

    // Basic sanity check: some sets may be deleted, at least 1 remains
    let remaining = count_backup_sets(&dest)?;
    assert!(remaining >= 1);
}
```

### Deletion Strategy

**Initial implementation**: Oldest-first deletion
- Sort backup sets by creation time (ascending)
- Delete oldest sets until sufficient space is available
- Always preserve at least 1 previous backup set for hard-linking
- Delete complete sets only (atomic operation)

**Future enhancement**: Weighted random distribution (mentioned in issue)
- Consider backup age, size, and importance
- Could implement GFS (Grandfather-Father-Son) retention
- Defer to future ADR when implementing

## Alternatives Considered

### 1. Fill Disk in Tests

**Approach**: Actually fill a test disk partition to trigger low space

**Pros**:
- Tests real behavior
- No mocking needed

**Cons**:
- Extremely slow (writing GBs of data)
- Requires special test environment setup
- Can't run in parallel
- Flaky on shared CI systems
- Dangerous if test disk is misconfigured

**Verdict**: Rejected - too slow and brittle for TDD workflow

### 2. No Abstraction - Test Only at E2E Level

**Approach**: Skip unit tests, only test full backup command

**Pros**:
- Simpler implementation (no traits)
- Tests "real" behavior

**Cons**:
- Slow feedback loop (full backup needed per test)
- Hard to test edge cases (what if space runs out mid-file?)
- Difficult to test error conditions
- Violates outside-in testing preference

**Verdict**: Rejected - doesn't support TDD approach

### 3. Feature Flag to Disable Space Checking

**Approach**: Add `#[cfg(test)]` to skip real space checks in tests

**Pros**:
- Simple to implement
- Fast tests

**Cons**:
- Test code path differs from production code path
- Can't test actual space checking logic
- May hide bugs in integration

**Verdict**: Partially adopted - use for some E2E tests, but primary approach is dependency injection

### 4. Trait Abstraction + Dependency Injection (Chosen)

**Approach**: Abstract space checking behind trait, inject implementation

**Pros**:
- Test pure logic quickly without disk operations
- Test integration with controlled space values
- Production code uses real implementation
- Can test error cases (e.g., space check fails)
- Enables TDD workflow with fast feedback

**Cons**:
- Slightly more complex implementation
- Need to thread trait through call stack
- Small runtime cost for trait dispatch (negligible)

**Verdict**: Chosen - best balance of testability and realism

## Rationale

### Why Three-Layer Testing?

1. **Layer 1 (Pure Logic)**: Fast feedback during development
   - Tests run in milliseconds
   - Perfect for TDD red-green-refactor cycles
   - Covers all edge cases and error conditions
   - No flakiness or system dependencies

2. **Layer 2 (Integration)**: Realistic orchestration without system dependence
   - Tests run in hundreds of milliseconds
   - Verifies components work together correctly
   - Uses controlled space values to test specific scenarios
   - Deterministic and repeatable

3. **Layer 3 (E2E)**: Confidence in real-world behavior
   - Tests run in seconds
   - Verifies actual system integration
   - Catches issues with real disk space reporting
   - Fewer tests needed (sanity checks only)

### Why Dependency Injection?

Dependency injection enables us to:
- Write tests first (TDD) without implementing real disk operations
- Test error conditions that are hard to reproduce (e.g., space check fails)
- Run tests in parallel without conflicts
- Achieve deterministic test behavior
- Maintain fast test suite (critical for TDD)

### Alignment with Codebase Design Principles

From README.md:
> Code Design:
> - Outside-in-tests

This ADR follows outside-in testing:
1. Start with high-level behavior (backup with auto-delete)
2. Design API from consumer perspective
3. Test at multiple levels of abstraction
4. Pure logic separated from IO operations

## Consequences

### Positive

- **Fast TDD workflow**: Unit tests run in milliseconds, enabling red-green-refactor cycles
- **Comprehensive coverage**: Can test edge cases that are hard to reproduce with real disk operations
- **Deterministic tests**: No flakiness from actual disk space variations
- **Parallel test execution**: No conflicts from shared disk resources
- **Clear separation of concerns**: Pure logic vs. system interaction
- **Future-proof**: Easy to add more sophisticated deletion strategies

### Negative

- **Abstraction overhead**: Need to define trait and thread it through call stack
- **Multiple implementations**: Must maintain both real and mock implementations
- **Slight complexity increase**: More code than direct implementation
- **Trait dispatch cost**: Minimal runtime overhead (likely optimized away)

### Neutral

- **Learning curve**: Developers must understand dependency injection pattern
- **Test maintenance**: Three layers means more test code to maintain
- **Mock synchronization**: Must ensure mock behavior matches real behavior

## Implementation Plan

### Phase 1: Pure Logic Module (TDD with Unit Tests)

1. Create `backup_sets/set_manager.rs`
2. Implement with tests:
   - `list_backup_sets()` - enumerate sets with metadata
   - `calculate_set_size()` - sum file sizes in a set
   - `select_sets_to_delete()` - oldest-first with preservation logic
   - `delete_backup_set()` - atomic set deletion

### Phase 2: Space Management Abstraction

1. Define `SpaceChecker` trait in `disk_space.rs`
2. Implement `RealSpaceChecker` using existing `get_disk_space()`
3. Implement `MockSpaceChecker` for tests
4. Add `estimate_backup_size()` function

### Phase 3: Integration Layer

1. Create `SpaceManager` struct coordinating:
   - Space checking
   - Size estimation
   - Deletion triggering
2. Add integration tests with mock checker

### Phase 4: CLI Integration

1. Add `--auto-delete` flag to CLI args
2. Thread flag through to backup function
3. Hook space management into backup process:
   - Check space before backup starts
   - Trigger deletions if needed
   - Resume backup after deletion
4. Add E2E tests

### Phase 5: Monitoring and Logging

1. Add logging for:
   - Space checks and decisions
   - Sets selected for deletion
   - Deletion progress
   - Space freed
2. Consider progress reporting for long deletions

## Open Questions

1. **Should we check space periodically during backup?**
   - Pro: Catch space issues mid-backup
   - Con: Performance overhead
   - Decision: Start with pre-backup check only, add monitoring later if needed

2. **What if auto-delete can't free enough space?**
   - Option A: Fail with clear error message
   - Option B: Delete as much as possible, attempt backup anyway
   - Decision: Fail clearly (Option A) - safer and more predictable

3. **Should we support minimum retention count?**
   - E.g., `--min-backups=3` never deletes if fewer than 3 sets exist
   - Decision: Defer to future enhancement - start with "preserve 1" hardcoded

4. **Should we respect .dhbignore when calculating sizes?**
   - Pro: More accurate size estimates
   - Con: Requires re-implementing ignore logic in set_manager
   - Decision: No - use actual on-disk size (simpler, accurate for space management)

## References

- GitHub issue #11: Removal of least important backups/files as space runs low
- ADR-003: Use sysinfo Crate for Disk Space Information (related to space checking)
- README.md: Outside-in testing approach
- Backup rotation theory: https://en.wikipedia.org/wiki/Backup_rotation_scheme
