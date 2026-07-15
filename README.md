# Smoothee

Make Git smooth.

Smoothee is a safer, clearer command-line workflow for Git and GitHub.

It explains what is happening, creates restore points before risky
operations, guides merge-conflict resolution, and helps you recover
when things go wrong.

Instead of memorizing recovery commands:

    smoothee sync
    smoothee resolve
    smoothee undo
    smoothee pr

Smoothee uses Git underneath and shows you the commands it runs.
You stay in control.

## Status

Early development. Phase 1 (Foundation) is implemented and the first
command, `smoothee status`, works today:

```
$ smoothee status

Repository: shop-platform
Branch: feature/login
Base branch: main

Working tree:
  • 3 modified files
  • 1 untracked file

Branch state:
  • 4 commits ahead, 6 commits behind main

Recommended next step:
  Your branch is behind its base and can be updated safely.
  smoothee sync
```

The remaining MVP commands (`sync`, `resolve`, `undo`, `pr`) are declared
in the CLI and report the roadmap phase that delivers them. See
[`PROJECT_SUMMARY.md`](PROJECT_SUMMARY.md) for the full design and the
phased roadmap.

## What works today

- Repository discovery (honours worktrees, submodules, `GIT_DIR`)
- A structured runner over the installed `git` binary that always keeps
  the real commands inspectable
- Plain-language `status`: working-tree summary, base-branch detection,
  ahead/behind analysis, and a single recommended next step
- Per-repository configuration via `.smoothee.toml`
- An append-only operation journal (JSON lines under `.git/smoothee/`)
  that will power `undo` and crash recovery

## Building and testing

Smoothee is a single Rust binary. You need a recent stable Rust
toolchain (1.80+) and a `git` binary on `PATH`.

```sh
cargo build              # debug build → target/debug/smoothee
cargo build --release    # optimized single executable
cargo run -- status      # build and run the status command
cargo test               # run the full test suite
cargo test <name>        # run a single test by name substring
cargo clippy             # lints
cargo fmt                # format (cargo fmt --check in CI)
```

## Design principles

Smoothee is deliberately conservative:

- **Safe by default** — a restore point is created before any risky operation.
- **Explain before acting** — the plan and reasoning are shown before mutating.
- **Preserve access to Git** — the underlying `git` commands are always visible.
- **Reversible and journaled** — every mutation is recorded so it can be undone.
- **AI suggests, humans approve** — AI never controls the shell or makes
  ambiguous decisions on its own.

## License

MIT — see [`LICENSE`](LICENSE).
