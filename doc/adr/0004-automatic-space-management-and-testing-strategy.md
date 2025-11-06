# ADR-004: Automatic Space Management and Testing Strategy

**Date:** 2025-01-06

## Status

Accepted - Implementation in progress

## Context

When backing up to external drives or limited storage, backups can fail when the disk runs out of space. The current implementation will encounter IO errors when attempting to write files to a full disk, similar to BackInTime's behavior. This requires manual intervention to free up space and retry the backup.

[GitHub issue #11](https://github.com/timabell/disk-hog-backup/issues/11) requests automatic removal of old backups when disk space runs low, allowing backups to complete successfully by proactively managing available space.

### Key Requirements from Issue #11

1. **Just-in-time space detection**: Detect insufficient space when about to copy a file, rather than handling IO errors after the fact
2. **Delete complete backup sets only**: Never leave partial/incomplete backups
3. **Weighted deletion strategy**: Consider backup age
4. **Preserve hard-link targets**: Deleting the last previous backup set would eliminate hard-link targets, forcing full re-copies
5. **Opt-in behavior**: Use `--auto-delete` flag to prevent surprise deletions

### Testing Challenge

The core challenge is that this feature's behavior fundamentally depends on actual disk space availability. Traditional end-to-end tests would need to:
- Fill an entire disk to trigger low-space conditions
- Work with realistic backup sizes (potentially GBs)
- Handle unpredictable disk states on test machines

This makes the feature difficult to test in a deterministic, fast, and reliable way.

## Decision

We will implement automatic space management with **just-in-time deletion** triggered at the point where files are being copied. This ensures we only delete backups when actually necessary and provides immediate feedback to the user.

### Architecture Decision

Freeing up space will be done at the **last possible moment** in the copy-files loop. The implementation will:

1. **Check space before each file copy**: In `copy_file_with_streaming()`, before attempting to copy a file, check if there's sufficient space
2. **Pause and delete if needed**: If insufficient space:
   - Block the backup process
   - Output to terminal explaining what's happening
   - Run the auto-delete logic (delete one old backup using weighted random)
   - Resume by exiting the blocking synchronous call
3. **User visibility**: The terminal output keeps users informed when auto-deletion is triggered

The implementation will use **dependency injection** and **trait abstraction** to separate disk space checking from backup logic for testability:

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

### Testing Strategy: Unit Tests Only

We will focus on **unit testing the space manager logic** with deterministic behavior. End-to-end testing of actual disk space exhaustion is deferred due to the challenges of simulating realistic disk-full scenarios without filling entire disks or creating huge test data sets.

#### Unit Tests: Space Manager Logic (Fast, Deterministic)

Test backup set management and selection logic without disk operations:

**Module**: `backup_sets/set_manager.rs`
- `list_backup_sets()` - enumerate existing sets with metadata
- `select_sets_to_delete()` - weighted random selection of sets to delete
- `delete_backup_set()` - remove a specific set
- `calculate_deletion_weight()` - pure weight calculation function

**Tests**:
```rust
#[test]
fn test_weighted_random_selection_with_seeded_rng() {
    let sets = vec![
        BackupSet { name: "set-1", size: 1000, created: days_ago(30) },
        BackupSet { name: "set-2", size: 2000, created: days_ago(20) },
        BackupSet { name: "set-3", size: 1500, created: days_ago(10) },
    ];
    // Use seeded RNG for deterministic test
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let result = select_sets_to_delete(&sets, 2500, 1000, &mut rng, 2.0);
    // With this seed, should select older sets with higher probability
    assert!(result.len() >= 1); // Freed some space
    assert!(sets.len() - result.len() >= 1); // Preserved at least 1
}

#[test]
fn test_preserve_at_least_one_set() {
    let sets = vec![
        BackupSet { name: "set-1", size: 5000, created: days_ago(30) },
    ];
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let result = select_sets_to_delete(&sets, 10000, 1000, &mut rng, 2.0);
    assert_eq!(result, vec![]); // Never delete the last set
}

#[test]
fn test_weight_calculation() {
    // Old backup with 10-day gap before it: high weight (more likely deleted)
    let weight_old = calculate_deletion_weight(10.0, 2.0);
    // Recent backup with 1-day gap before it: low weight (less likely deleted)
    let weight_recent = calculate_deletion_weight(1.0, 2.0);
    assert!(weight_old > weight_recent);
}

#[test]
fn test_exponent_affects_distribution() {
    // Higher exponent preserves old backups better
    let weight_exp_1 = calculate_deletion_weight(10.0, 1.0);
    let weight_exp_3 = calculate_deletion_weight(10.0, 3.0);
    // Both are higher for older backups, but exp=1 is more aggressive
    assert!(weight_exp_1 > weight_exp_3);
}
```

#### Testing Gap: End-to-End Validation

**Acknowledged limitation**: We are **not implementing end-to-end tests** for the actual disk space exhaustion scenario at this time.

**Rationale**:
- Testing actual disk-full conditions requires either:
  - Filling entire test disks with realistic data (GBs), which is slow and resource-intensive
  - Creating fake/mock disk volumes, which requires specialized test infrastructure
- Neither approach is practical for fast, deterministic CI/CD pipelines, especially on GitHub Actions
- **This is not ideal** - we're shipping a feature that interacts with disk space without end-to-end validation of that interaction. However, I'm not willing to delay this feature to figure out what is likely to be a complicated testing solution

**Future work**: We may return to this testing challenge once the feature is working in production and we have real-world usage patterns to validate against. At that point, we could consider:
- Manual testing scenarios with nearly-full test drives
- Integration tests with small disk images/containers
- Property-based testing with various space constraints

### Deletion Strategy

**Chosen approach**: Weighted random distribution
- Assign probability weights to each backup set based on age
- Older backups have higher probability of deletion
- Recent backups have lower probability of deletion
- Maintains good temporal distribution of backups over time
- Handles irregular backup schedules gracefully

**Algorithm** (from backup rotation theory):
1. For each deletable generation, calculate weight as:
   ```
   weight = (1 / time_span_to_previous_backup) ^ exponent
   ```
   Where `time_span_to_previous_backup` is days between this backup and the previous one

2. Higher exponent → more uniform distribution (preserves old backups better)
3. Lower exponent → skews toward recent backups (deletes old backups more readily)

**Constraints**:
- Always preserve at least 1 previous backup set for hard-linking
- Delete complete sets only (atomic operation)
- Only delete when space is actually needed

**Alternative considered**: Simple oldest-first deletion
- **Pros**: Deterministic, simple to implement and test
- **Cons**: Doesn't maintain good temporal distribution, especially with irregular backups
- **Verdict**: Rejected in favor of weighted random as specified in [issue #11](https://github.com/timabell/disk-hog-backup/issues/11)

**Rationale for weighted random**:
- Better handles missed or irregular backups
- Maintains coverage across all points in time
- Probabilistically keeps old backups longer than simple FIFO
- More sophisticated but still testable with seeded RNG

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

### Why Unit Tests Only?

**Focus on testable logic**: By isolating the weighted selection algorithm and backup set management logic, we can:
- Run tests in milliseconds
- Enable TDD red-green-refactor cycles
- Cover all edge cases deterministically
- Avoid flakiness from system dependencies
- Execute tests in parallel without conflicts

**Defer complex E2E testing**: The alternative approaches (filling disks, fake volumes) add significant complexity without proportional value at this stage. The just-in-time deletion approach means failures are graceful and per-file rather than catastrophic.

### Why Dependency Injection?

Dependency injection enables us to:
- Write tests first (TDD) without implementing real disk operations
- Test error conditions that are hard to reproduce (e.g., space check fails)
- Run tests in parallel without conflicts
- Achieve deterministic test behavior
- Maintain fast test suite (critical for TDD)

### Testing Randomness Deterministically

The weighted random distribution requires randomness, but tests must be deterministic. Solution:

1. **Use seeded RNG in tests**: `ChaCha8Rng::seed_from_u64(42)`
   - Same seed → same sequence of random numbers
   - Tests are repeatable and debuggable

2. **Test the weights, not specific outcomes**:
   - Verify weight calculation is correct (pure function)
   - Verify older backups have higher deletion probability
   - Run selection multiple times with different seeds, verify statistical properties

3. **Inject RNG via trait** (production uses thread_rng, tests use seeded):
   ```rust
   trait RandomSource {
       fn gen_range(&mut self, range: Range<f64>) -> f64;
   }
   ```

4. **Statistical tests** (optional, more advanced):
   - Run selection 1000 times with different seeds
   - Verify older backups deleted more frequently
   - Check distribution matches expected probabilities

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
   - `list_backup_sets()` - enumerate sets with metadata (creation time, size)
   - `calculate_set_size()` - sum file sizes in a set
   - `calculate_deletion_weight(time_span_days, exponent)` - pure weight calculation
   - `select_sets_to_delete(sets, space_needed, rng, exponent)` - weighted random selection with preservation logic
   - `delete_backup_set()` - atomic set deletion

**Key testing approach**:
- Use seeded RNG (`ChaCha8Rng::seed_from_u64`) for deterministic tests
- Test weight calculation as pure function
- Test that older backups have higher weights
- Test that at least 1 set is always preserved
- Test exponent parameter affects distribution

### Phase 2: Just-in-Time Space Management

1. Modify `copy_file_with_streaming()` in `dhcopy/streaming_copy.rs`:
   - Before copying each file, check if file size fits in available space
   - If insufficient space detected:
     - Output message to terminal: "Insufficient space, auto-deleting old backup..."
     - Call deletion logic (select and delete one old backup set)
     - Output message: "Deleted backup set X, resuming..."
     - Continue with file copy

2. Thread `--auto-delete` flag through to streaming copy function

3. Pass backup destination path to enable backup set enumeration during copy

### Phase 3: CLI Integration

1. Add `--auto-delete` flag to CLI args
2. Thread flag through backup call chain to `copy_file_with_streaming()`
3. Update function signatures to pass destination path where needed

### Phase 4: User Feedback

1. Add clear terminal output when auto-deletion triggers:
   - Which backup set is being deleted
   - Why (insufficient space for current file)
   - Confirmation when deletion completes
2. Ensure messages don't interfere with existing progress reporting

## Open Questions

1. **What exponent should we use for weighted random distribution?**
   - Lower (e.g., 1.0): More aggressive deletion of old backups
   - Higher (e.g., 2.0 or 3.0): Better preservation of old backups
   - Decision: **Start with 2.0 (square)** as mentioned in literature, make configurable later via `--deletion-exponent` flag if needed
   - Rationale: Square provides good balance, can be tuned based on user feedback

2. **What if auto-delete can't free enough space?**
   - With just-in-time approach: Delete one backup, attempt file copy, let it fail naturally if still insufficient
   - User will see the deletion attempt and the subsequent failure, providing clear context
   - Decision: **Let the file copy fail naturally** - simpler and provides immediate feedback

3. **Should we support minimum retention count?**
   - E.g., `--min-backups=3` never deletes if fewer than 3 sets exist
   - Decision: Defer to future enhancement - start with "preserve 1" hardcoded

4. **How to handle first backup (no previous backups)?**
   - Cannot delete anything (preserve 1 rule)
   - With just-in-time approach: Let individual file copies fail naturally if insufficient space
   - Decision: No special handling - natural IO error provides clear feedback

## References

- [GitHub issue #11](https://github.com/timabell/disk-hog-backup/issues/11): Removal of least important backups/files as space runs low
- ADR-003: Use sysinfo Crate for Disk Space Information (related to space checking)
- README.md: Outside-in testing approach
- Backup rotation theory: https://en.wikipedia.org/wiki/Backup_rotation_scheme
