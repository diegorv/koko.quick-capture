# Test-and-commit per slice with Conventional Commits

Each issue under `.scratch/<feature>/issues/` is treated as one phase. The workflow for that phase is fixed:

1. Implement the slice end-to-end.
2. Run the full test suite (`cargo test` on the Rust side and `pnpm test` on the Svelte side, plus `cargo check` and `pnpm check` for type / lint regressions). The slice is only considered done when every test passes.
3. Only then create a git commit that captures the work of that phase.
4. Move on to the next slice.

Commits follow the Conventional Commits convention so message intent is machine-readable and history reads cleanly. The leading type is one of `feat`, `fix`, `chore`, `refactor`, `test`, `docs`, `perf`, `build`, or `ci`. The subject is in the imperative mood, under ~72 characters, no trailing period. The body, when included, explains the *why* and references the slice file (e.g. `.scratch/v0-1-mvp/issues/02-composer-note-tracer.md`).

Examples:

- `feat(store): add SQLite-backed Capture store with ULID ids`
- `feat(composer): wire Ctrl+Opt+Cmd+Space to Composer window`
- `test(kind_detect): cover URL detection branches`
- `chore: scaffold Tauri 2 + Svelte project`
- `refactor(commands): split capture wiring out of shortcuts module`

Squashing multiple slices into one commit is rejected: each slice is independently verifiable, and one-commit-per-slice keeps `git bisect` and revert surgical. Skipping the test gate is also rejected: the whole point of ADR-0005's full coverage is that "green tests" is the only signal the slice actually works on this greenfield codebase.
