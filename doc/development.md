# Development

## Build, Test, and Lint

```bash
cargo build              # Build the project
cargo test               # Run all tests
cargo test <test_name>   # Run a single test
./lint.sh                # Run all linting (fmt, clippy, license check, yamllint)
cargo run -- --source <dir> --destination <dir>  # Run the tool locally
```

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

Commits without these prefixes will not trigger a release.

## Release Process

Releases are fully automatic when pushing to main:

1. CI runs on every push to main
2. Pipeline checks if commits contain release-worthy prefixes (above)
3. If release-worthy commits exist:
   - Calculates next version using git-cliff
   - Rebuilds with version embedded in binary
   - Creates git tag
   - Publishes GitHub release with artifacts
4. If no release-worthy commits: just runs CI, no release

### Version Bumping

By default, releases increment the patch version (e.g., 0.5.1 -> 0.5.2).

To bump minor or major version, add a footer to your commit message:

```
feat: Add new backup compression feature

Implements gzip compression for backup files.

bump: minor
```

Valid footers:
- `bump: minor` - e.g., 0.5.2 -> 0.6.0
- `bump: major` - e.g., 0.5.2 -> 1.0.0

### Preview Release Notes

```bash
./release-notes.sh --preview  # Preview what next release notes will look like
./release-notes.sh            # View latest release notes
```

## Code Design

* [Outside-in-tests](https://pod.0x5.uk/25)
* [Architecture Decision Records (ADRs)](adr/)
* [Threading Architecture](threading.md) - Multi-threaded pipeline design for backup operations
