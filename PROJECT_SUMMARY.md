# Smoothee

A safer, clearer Git and GitHub workflow for developers who want confidence without memorizing recovery commands.

## The idea

Smoothee is a cross-platform CLI that simplifies common Git and GitHub workflows while preserving transparency and control.

It helps developers:

- understand repository state
- safely synchronize branches
- resolve merge conflicts
- create clean commits
- open pull requests
- recover from mistakes
- understand confusing Git errors

Smoothee does not replace Git. It sits above Git as a guided workflow layer.

The goal is not to hide complexity until something breaks. The goal is to explain complexity, prevent avoidable mistakes, and make every risky action reversible.

## Core principles

### Safe by default

Before performing risky operations, Smoothee creates a restore point.

```
smoothee sync

Your branch is 4 commits ahead and 7 commits behind main.

Plan:
  1. Fetch origin
  2. Create a restore point
  3. Rebase your commits onto origin/main
  4. Verify the result

Restore point:
  smoothee/restore/feature-login/2026-07-15-1432

Continue? [Y/n]
```

### Explain before acting

Smoothee shows what it intends to do and why.

```
Recommended: Rebase

Reason:
  Your branch has not been shared, and rebasing will keep its
  history clean without creating an unnecessary merge commit.
```

### Preserve access to Git

Smoothee should never turn Git into mysterious hidden machinery.

```
Running:
  git fetch origin
  git rebase origin/main
```

Advanced users can inspect, copy, or run the underlying commands themselves.

### Make mistakes reversible

Every operation performed through Smoothee should be recorded.

```
smoothee undo

Last operation:
  Rebased feature/login onto origin/main

A restore point is available.

Restore the branch to its previous state? [y/N]
```

### AI suggests, humans approve

AI may explain changes, propose conflict resolutions, summarize diffs, and identify risks.

It should not silently make ambiguous product or business decisions.

## Primary commands

```
smoothee status
smoothee sync
smoothee resolve
smoothee commit
smoothee pr
smoothee undo
smoothee explain
smoothee doctor
```

### `smoothee status`

Shows repository state in plain language.

```
smoothee status

Repository: shop-platform
Branch: feature/login
Base branch: main

Working tree:
  3 modified files
  1 untracked file

Branch state:
  4 commits ahead of origin/feature/login
  6 commits behind origin/main

Warnings:
  package-lock.json changed locally and on main
  Your last successful test run was before the latest changes

Recommended next step:
  smoothee sync
```

It should answer the questions developers actually have:

- What branch am I on?
- Is anything uncommitted?
- Am I ahead or behind?
- Is it safe to push?
- What should I do next?

### `smoothee sync`

Safely updates the current branch.

```
smoothee sync
```

Default behavior:

1. detect the base branch
2. fetch remote changes
3. inspect whether the branch is shared
4. recommend merge or rebase
5. create a restore point
6. perform the selected operation
7. guide conflict resolution if needed
8. run configured verification checks

Optional flags:

```
smoothee sync --rebase
smoothee sync --merge
smoothee sync --dry-run
smoothee sync --no-verify
```

Example:

```
Fetched origin.

Your branch:
  3 commits ahead
  5 commits behind main

Recommended action:
  Rebase onto origin/main

Why:
  The branch has not been pushed by another contributor.
  Rebasing avoids an unnecessary merge commit.

Continue? [Y/n]
```

### `smoothee resolve`

Provides guided merge-conflict resolution.

```
smoothee resolve

Conflict 1 of 2
File: src/auth/login.ts

Your branch:
  Adds login telemetry

Main:
  Adds input validation

Suggested resolution:
  Preserve both changes.
  Run validation before recording telemetry.

Confidence: High

Actions:
  [a] Apply suggestion
  [e] Edit manually
  [o] Keep ours
  [t] Keep theirs
  [d] Show complete diff
  [s] Skip
```

After applying a resolution:

```
Resolution applied.

Checks:
  ✓ Conflict markers removed
  ✓ File parses successfully
  ✓ Related tests passed
```

Smoothee should understand more than the conflict markers themselves. It should inspect:

- surrounding code
- changed functions
- relevant commits
- renamed symbols
- related tests
- branch intent
- nearby documentation

For ambiguous cases:

```
This conflict changes the discount from 20% to 10%.

I cannot determine which business rule is correct.

Recommendation:
  Ask the owner of the pricing logic before resolving.
```

That refusal is a feature. Confident nonsense is already abundantly available.

### `smoothee commit`

Creates intentional, reviewable commits.

```
smoothee commit

Smoothee analyzes staged and unstaged changes.

Detected two unrelated changes:

1. Add refresh-token rotation
2. Update dashboard spacing

Recommended:
  Split these into separate commits.

Actions:
  [s] Split interactively
  [c] Commit together
  [v] View affected files
```

It can also:

- generate commit messages
- detect secrets
- identify generated files
- warn about large binaries
- suggest commit splitting
- run pre-commit checks
- summarize the final staged diff

Example:

```
Suggested commit:

  feat(auth): rotate refresh tokens after use

Details:
  - invalidate previously used refresh tokens
  - add reuse detection
  - extend authentication tests

Use this message? [Y/e/n]
```

### `smoothee pr`

Creates a GitHub pull request from the current branch.

```
smoothee pr
```

Before opening the PR, it checks:

- whether the branch is pushed
- whether it is synchronized with the base branch
- whether tests pass
- whether there are unresolved conflicts
- whether the diff contains suspicious files
- whether a similar pull request already exists

Example:

```
Pull request preview

Title:
  Add refresh-token rotation

Summary:
  Introduces single-use refresh tokens and detects token reuse.

Changes:
  - rotate refresh tokens after successful use
  - revoke token families on reuse
  - add authentication tests

Verification:
  ✓ 42 tests passed
  ✓ Type checking passed
  ✓ No secrets detected

Create pull request? [Y/e/n]
```

Initial GitHub integration can delegate to the official `gh` CLI.

```
Git operations:
  git

GitHub authentication and API operations:
  gh
```

This avoids rebuilding authentication and pull-request infrastructure before the product has earned the privilege of becoming complicated.

### `smoothee undo`

Reverses the last Smoothee-managed operation.

```
smoothee undo

Last operation:
  Synced feature/login using rebase

Changes made:
  - rebased 4 commits
  - resolved 2 conflicts
  - updated local branch pointer

Restore point:
  smoothee/restore/feature-login/2026-07-15-1432

Restore previous state? [y/N]
```

Smoothee should support recovery from:

- merges
- rebases
- resets
- conflict resolutions
- commit splitting
- accidental branch changes
- failed synchronization
- selected push operations

It should rely on standard Git mechanisms such as refs, reflog, temporary branches, and tags rather than inventing a private repository format.

### `smoothee explain`

Explains Git state, errors, commands, or ranges.

```
smoothee explain
smoothee explain conflict
smoothee explain HEAD~3..HEAD
smoothee explain "why was my push rejected?"
smoothee explain reflog
```

Example:

```
Your push was rejected because the remote branch contains commits
that are not present locally.

Safe options:

1. Sync using rebase
   Best when your local commits have not been shared.

2. Sync using merge
   Best when the branch is shared and history should not be rewritten.

Force-pushing is not recommended because another contributor has
updated the branch.
```

### `smoothee doctor`

Checks the developer environment and repository configuration.

```
smoothee doctor

Environment:
  ✓ Git 2.51.0
  ✓ GitHub CLI authenticated
  ✓ Repository remote detected
  ✓ Default branch: main

Configuration:
  ✓ Pull strategy configured
  ⚠ No test command configured
  ⚠ Commit signing is disabled

Suggested fixes:
  smoothee config test-command "npm test"
  smoothee config commit-signing true
```

## AI architecture

The AI layer should never directly control the shell.

```
User request
    ↓
Deterministic repository inspection
    ↓
Structured repository state
    ↓
AI explanation or proposed patch
    ↓
Validation and safety checks
    ↓
User approval
    ↓
Deterministic Git operation
```

The model receives a limited, structured context:

- conflict hunks
- surrounding source code
- commit messages
- branch metadata
- related tests
- symbol changes
- repository instructions

The model returns structured output:

```json
{
  "summary": "Both branches add independent behavior",
  "confidence": "high",
  "resolution": {
    "type": "combined",
    "content": "..."
  },
  "reasons": [
    "Preserves validation added on main",
    "Preserves telemetry added on the feature branch"
  ],
  "risks": [],
  "recommended_checks": [
    "npm test -- auth/login.test.ts"
  ]
}
```

Smoothee validates the response before offering it to the user.

Validation may include:

- syntax parsing
- type checking
- conflict-marker scanning
- formatting
- linting
- targeted tests
- full tests
- generated-file detection

## Privacy model

Smoothee should support remote and local AI providers.

```
smoothee config ai.provider openai
smoothee config ai.provider anthropic
smoothee config ai.provider ollama
smoothee config ai.provider none
```

Before sending repository content remotely:

```
The following data will be sent to the configured AI provider:

  2 conflict hunks
  96 surrounding lines
  3 commit messages
  1 related test file

Excluded:
  .env files
  detected credentials
  ignored files
  files outside the repository

Continue? [Y/n]
```

Privacy controls:

```
smoothee resolve --local
smoothee resolve --no-ai
smoothee explain --redact
smoothee config ai.confirm-before-send true
```

The tool should automatically exclude:

- `.env` files
- private keys
- credential files
- known secret patterns
- ignored files
- user-configured sensitive paths

## Configuration

Global configuration:

```
smoothee config --global sync.strategy auto
smoothee config --global ai.provider ollama
smoothee config --global show-git-commands true
```

Repository configuration:

```toml
# .smoothee.toml

base_branch = "main"
sync_strategy = "rebase"

[verification]
format = "npm run format:check"
lint = "npm run lint"
types = "npm run typecheck"
test = "npm test"

[ai]
enabled = true
send_surrounding_lines = 40
confirm_before_send = true

[privacy]
exclude = [
  ".env*",
  "secrets/**",
  "customer-data/**"
]
```

## MVP

The first useful release should remain deliberately small.

### MVP commands

```
smoothee status
smoothee sync
smoothee resolve
smoothee undo
smoothee pr
```

### MVP workflow

```
smoothee status
smoothee sync
smoothee resolve
smoothee pr
```

### MVP capabilities

- inspect repository state
- detect the base branch
- fetch and synchronize safely
- create restore points
- guide merge-conflict resolution
- propose AI-assisted resolutions
- validate resolved files
- push branches
- create GitHub pull requests through `gh`
- undo the latest managed operation

### Not required for the first release

- custom Git hosting support
- graphical interface
- team dashboards
- autonomous code changes
- issue management
- full commit-history visualization
- hosted accounts
- cloud repository indexing

Those can wait until actual users demonstrate that they want them, a radical product-management technique seldom attempted in captivity.

## Suggested implementation

### Language

Rust is the strongest long-term fit.

Benefits:

- fast startup
- single executable
- cross-platform distribution
- strong type safety
- reliable filesystem handling
- good terminal libraries
- low runtime overhead

Suggested crates:

| Crate | Purpose |
| --- | --- |
| `clap` | command parsing |
| `inquire` | interactive prompts |
| `console` | terminal formatting |
| `indicatif` | progress output |
| `serde` | structured data |
| `serde_json` | model responses |
| `reqwest` | API clients |
| `tokio` | asynchronous operations |
| `anyhow` | application errors |
| `thiserror` | typed errors |
| `tracing` | logs and diagnostics |
| `directories` | configuration paths |
| `tempfile` | safe temporary files |

Use the installed `git` binary rather than relying entirely on a Git reimplementation.

Native Git offers the best compatibility with:

- hooks
- credentials
- filters
- submodules
- worktrees
- signing
- repository-specific configuration

Smoothee can execute Git commands through a controlled command layer and parse machine-readable output.

Examples:

```
git status --porcelain=v2 --branch
git diff --no-ext-diff --unified=3
git for-each-ref
git merge-base
git rev-list
```

### Internal modules

```
src/
  cli/
    commands/
      status.rs
      sync.rs
      resolve.rs
      undo.rs
      pr.rs

  git/
    command.rs
    repository.rs
    status.rs
    branches.rs
    conflicts.rs
    restore_points.rs

  github/
    gh.rs
    pull_requests.rs

  ai/
    provider.rs
    prompts.rs
    schema.rs
    redaction.rs

  verification/
    syntax.rs
    commands.rs
    conflict_markers.rs

  operations/
    journal.rs
    undo.rs
    plan.rs

  ui/
    prompts.rs
    output.rs
    theme.rs

  config/
    global.rs
    repository.rs
```

### Operation journal

Every Smoothee operation should be journaled locally.

```json
{
  "id": "op_01J2...",
  "type": "sync_rebase",
  "repository": "/home/chris/projects/shop",
  "branch": "feature/login",
  "started_at": "2026-07-15T14:32:00+02:00",
  "before": {
    "head": "abc123",
    "restore_ref": "refs/smoothee/restore/op_01J2..."
  },
  "after": {
    "head": "def456"
  },
  "status": "completed"
}
```

This powers:

- undo
- troubleshooting
- operation history
- crash recovery
- support diagnostics

## Tone and interface

The product can have personality, but clarity must come first.

Good:

```
Things got chunky.

2 conflicts need attention.
No changes have been lost.
```

Bad:

```
Oopsie! Git had a little meltdown 🥤✨
```

Developers facing a damaged branch do not need a mascot performing stand-up comedy over the remains.

Humor should be occasional and secondary.

## Branding

### Name

Smoothee

Pronunciation:

> «smoothie»

### Tagline

Make Git smooth.

Alternative taglines:

- Git without the panic.
- Safer Git, clearer workflows.
- Understand Git. Undo mistakes.
- Conflicts in. Clarity out.
- A calmer way to Git.

### Command name

`smoothee`

Possible abbreviation:

`smt`

The full command is distinctive and readable enough that an abbreviation should not be necessary initially.

### Visual identity

The visual style should feel:

- calm
- modern
- technical
- friendly without becoming childish
- dependable during failure states

A subtle cup, swirl, branch, or blending motif could work, but the brand should not become so literal that the website resembles a juice bar.

## Positioning

Smoothee should not be marketed primarily as:

> «Git with AI.»

That framing is generic and makes the product sound dependent on a feature competitors can reproduce.

The stronger positioning is:

> «Smoothee is a safer, clearer Git and GitHub workflow with guided conflict resolution and reliable undo.»

AI strengthens the product through:

- conflict understanding
- diff summaries
- commit suggestions
- error explanations
- risk detection

The product's durable value comes from:

- safety
- reversibility
- transparency
- workflow design
- deterministic validation
- excellent terminal UX

## Target users

Primary users:

- developers who understand basic Git but fear complex operations
- junior and intermediate developers
- developers working in small teams
- open-source contributors
- Linux users underserved by polished GitHub tools
- developers switching between terminal and GitHub workflows

Secondary users:

- experienced developers who want faster routine workflows
- educators teaching Git
- support teams helping developers recover repositories
- organizations standardizing safe Git practices

## Success criteria

Smoothee succeeds when users can:

- understand their branch state in seconds
- update branches without fearing data loss
- resolve common conflicts with confidence
- recover from mistakes without searching for obscure commands
- create clean pull requests from the terminal
- gradually learn the Git commands underneath the workflow

The ideal outcome is not permanent dependence on Smoothee.

The ideal outcome is that developers become more capable while using it.

## One-sentence pitch

Smoothee is a cross-platform Git and GitHub CLI that explains repository state, safely synchronizes branches, guides merge-conflict resolution, and makes risky operations reversible.

## README opening

```
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
```

## Initial roadmap

### Phase 1: Foundation

- repository discovery
- structured Git command runner
- human-readable status
- configuration
- restore refs
- operation journal

### Phase 2: Safe synchronization

- fetch
- base-branch detection
- ahead/behind analysis
- merge-versus-rebase recommendation
- guided synchronization
- abort and recovery handling

### Phase 3: Conflict workflow

- conflict-file detection
- hunk parsing
- ours/theirs/base inspection
- manual interactive resolution
- validation
- AI-assisted proposals

### Phase 4: GitHub workflow

- GitHub CLI detection
- authentication checks
- push support
- pull-request generation
- PR summaries
- issue linking

### Phase 5: Commit workflow

- diff grouping
- secret detection
- generated commit messages
- commit splitting
- staged-change review

### Phase 6: Broader integrations

- GitLab
- Forgejo
- Gitea
- Bitbucket
- local model improvements
- editor integrations
- optional desktop interface
