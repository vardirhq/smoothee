# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

Smoothee is **pre-implementation**. The repository currently contains only a
design specification (`PROJECT_SUMMARY.md`) and a stub `README.md`. There is no
source code, build system, dependency manifest, tests, or CI yet.

Before writing code, read `PROJECT_SUMMARY.md` in full — it is the source of
truth for scope, commands, architecture, module layout, and the phased roadmap.
This file summarizes the parts that constrain how code should be written; the
spec has the detail.

Because nothing is scaffolded, there are no build/lint/test commands to document
yet. When the first Rust crate is created, update this section with the real
`cargo` invocations (e.g. `cargo build`, `cargo test`, `cargo test <name>` to
run a single test, `cargo clippy`, `cargo fmt`).

## What Smoothee is

A cross-platform CLI that sits **above** Git (and GitHub via `gh`) as a guided
workflow layer. It does not reimplement or replace Git. It explains repository
state, safely synchronizes branches, guides merge-conflict resolution, and makes
risky operations reversible. The durable value is safety, reversibility,
transparency, and terminal UX — AI is an assist, not the product.

Primary commands (see spec for full behavior): `status`, `sync`, `resolve`,
`commit`, `pr`, `undo`, `explain`, `doctor`. The MVP is deliberately narrow:
`status`, `sync`, `resolve`, `undo`, `pr`.

## Non-negotiable design principles

These are product invariants, not style preferences. Any code that violates them
is wrong regardless of whether it compiles.

- **Safe by default.** Create a restore point (a Git ref/tag) *before* any risky
  operation. Recovery relies on standard Git mechanisms — refs, reflog, temporary
  branches, tags — never a private/invented repository format.
- **Explain before acting.** Show the plan and the reasoning, then ask for
  confirmation before mutating the repo.
- **Preserve access to Git.** Always show the underlying `git` commands being run
  so advanced users can inspect, copy, or run them. Git must never become hidden
  machinery.
- **Every mutation is reversible and journaled.** Each operation is recorded in a
  local operation journal (see spec) that powers `undo`, crash recovery, and
  diagnostics.
- **AI suggests, humans approve.** AI may explain, summarize, propose conflict
  resolutions, and flag risks. It must never silently make ambiguous product or
  business decisions, and refusing to resolve an ambiguous conflict is a feature,
  not a gap.

## Architecture: the deterministic/AI boundary

The AI layer **never controls the shell**. All Git mutation is deterministic and
gated behind explicit user approval. The required pipeline is:

```
User request
  → deterministic repository inspection
  → structured repository state
  → AI explanation or proposed patch (structured JSON in/out)
  → validation and safety checks
  → user approval
  → deterministic Git operation
```

Consequences to enforce in code:

- The model receives only a **limited, structured context** (conflict hunks,
  surrounding lines, commit messages, branch metadata, related tests). Sensitive
  data is excluded *before* it is ever assembled — `.env*`, private keys,
  credential files, known secret patterns, ignored files, files outside the repo,
  and user-configured paths. Confirm before sending anything to a remote provider.
- AI responses are **structured** and must be **validated** before being offered
  to the user (syntax parsing, type checking, conflict-marker scanning,
  formatting/linting, targeted or full tests).
- AI providers are pluggable (`openai`, `anthropic`, `ollama`, `none`); a fully
  local / no-AI path must always work.

## Intended stack and layout

Rust is the chosen implementation language (fast startup, single executable,
cross-platform). Shell out to the **installed `git` binary** and parse
machine-readable output (e.g. `git status --porcelain=v2 --branch`,
`git for-each-ref`, `git merge-base`, `git rev-list`) rather than reimplementing
Git — this preserves compatibility with hooks, credentials, filters, submodules,
worktrees, and signing. GitHub operations delegate to the official `gh` CLI so
auth/PR infrastructure isn't rebuilt.

Planned crates and the `src/` module layout (`cli/`, `git/`, `github/`, `ai/`,
`verification/`, `operations/`, `ui/`, `config/`) are enumerated in
`PROJECT_SUMMARY.md`. Follow that structure when scaffolding; the separation
between the `git/` command layer, the `ai/` layer, and the `operations/` journal
directly encodes the deterministic/AI boundary above.

## Configuration

- Per-repository config lives in `.smoothee.toml` (base branch, sync strategy,
  `[verification]` commands, `[ai]` settings, `[privacy]` exclude globs).
- Global config via `smoothee config --global ...`.
- Verification commands are project-defined, not hardcoded — read them from
  `[verification]` rather than assuming `npm`/`cargo`/etc.

## Tone

Personality is allowed but clarity comes first, especially in failure states.
Reassure about data safety ("No changes have been lost"), state what needs
attention plainly, and keep humor occasional and secondary. No mascot theatrics
over a broken branch.
