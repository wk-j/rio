# Skill: Run Tests

## When to Use

Use this skill any time you need to run the Rio test suite — after implementing a
feature, fixing a bug, or verifying that existing tests still pass. Do **not** use
`--release` mode for test runs; dev mode compiles faster and is sufficient for
correctness checks.

## Commands

### All crates (default — use this unless you need a specific scope)

```bash
cargo test
```

### Single crate

```bash
cargo test -p rio-backend
cargo test -p copa
cargo test -p sugarloaf
cargo test -p rioterm
```

### Single test by exact name

```bash
cargo test -p rio-backend test_empty_config_file
cargo test -p rio-backend -- config::tests::test_filepath
```

### All tests in a module (filter by path prefix)

```bash
cargo test -p rio-backend -- config::tests
cargo test -p rioterm -- context::grid::tests
```

## Rules

- **Never use `--release`** — dev mode is fast enough and avoids long compile times.
- Run the narrowest scope that covers your change:
  - Changed only `rio-backend`? → `cargo test -p rio-backend`
  - Changed `context/grid.rs`? → `cargo test -p rioterm -- context::grid`
  - Changed multiple crates? → `cargo test`
- Always fix **all** failures before marking a task done. A single failing test is
  a blocker.
- After fixing compilation errors in tests, re-run to confirm the fix is complete.

## Interpreting Output

- `test foo::bar::test_xyz ... ok` — passing
- `test foo::bar::test_xyz ... FAILED` — failing; read the assertion output below
- `error[E0061]` (or any `error[...]`) — compilation failure; fix before re-running
- `warning:` lines — non-blocking but address them if they are in code you touched

## Linux-Specific Setup

On Linux, install system dependencies before running tests if not already present:

```bash
sudo apt-get install libasound2-dev libfontconfig1-dev
```

For platform-specific feature flags on Linux:

```bash
cargo test --no-default-features --features=x11
cargo test --no-default-features --features=wayland
```
