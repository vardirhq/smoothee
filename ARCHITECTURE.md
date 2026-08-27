# Smoothee architecture

Smoothee is intentionally a small Rust CLI. The codebase should stay easy to inspect, easy to test, and difficult to accidentally turn into a second implementation of Git.

## Dependency direction

The preferred dependency flow is:

```text
cli -> operations -> git
 |         |
 |         -> verification
 |
 -> github
 -> config
 -> ui
```

The boundaries are deliberate:

- `src/cli/` parses arguments, presents plans, asks for approval, and renders results. It should not contain reusable Git mechanics.
- `src/operations/` owns multi-step Smoothee workflows, restore points, journaling, and reversible state transitions.
- `src/git/` is the deterministic boundary around the installed `git` binary and machine-readable Git output. Git-specific parsing and command construction belong here.
- `src/github/` is the equivalent deterministic boundary around the GitHub CLI. GitHub command construction belongs here rather than in feature modules.
- `src/config/` owns configuration loading and paths.
- `src/verification/` contains pure checks that decide whether an operation is safe to continue.
- `src/ui/` owns terminal presentation and prompting. It must not decide repository state.

Higher-level modules may depend on lower-level modules. Lower-level modules should not call back into CLI or UI code.

## Mutation rules

Repository mutations must remain explicit and inspectable:

1. construct a deterministic `GitCommand` or `GhCommand`;
2. show the user the planned command when appropriate;
3. obtain approval for ambiguous or risky actions;
4. execute through the command boundary;
5. journal reversible Smoothee operations where applicable.

Do not shell out through ad-hoc `std::process::Command` in production code. Test helpers may invoke Git directly when setting up isolated repositories.

AI must never own the mutation boundary. AI may eventually suggest explanations, grouping, titles, or conflict resolutions, but deterministic code validates and executes any resulting action after user approval.

## Module size

Large test suites are welcome. Large production modules are not.

CI enforces a soft architectural ceiling of 350 production lines per Rust source file, counting only the portion before an inline `#[cfg(test)]` module. When a module approaches the ceiling, split by responsibility rather than by arbitrary line ranges.

Good splits are things such as:

- analysis vs execution;
- parsing vs command construction;
- workflow state vs terminal presentation;
- journal storage vs recovery policy.

A file exceeding the ceiling is a design signal, not an invitation to increase the number.

## Testing expectations

Pure parsers and planners should have deterministic unit tests. Git workflows should use throwaway repositories and verify observable repository state. Mutating paths should cover failure and recovery behavior, not only the happy path.

CI is expected to run formatting, Clippy with warnings denied, tests, a release build, the declared MSRV, and the source-size guard. Cargo commands use `--locked` so a PR is tested against the committed dependency graph.

## Growth rules

Before adding a new command, prefer extending an existing lower-level abstraction when the capability is genuinely shared. Do not create a generic framework in anticipation of hypothetical commands.

When a feature needs Git or GitHub behavior that another feature could reasonably reuse, put the reusable mechanism in `git` or `github` and keep the command module focused on workflow and presentation.

Smoothee should remain one binary and one crate until there is a concrete boundary that benefits from becoming independently reusable. More crates are not inherently more architecture. Sometimes they are merely more Cargo.toml files.