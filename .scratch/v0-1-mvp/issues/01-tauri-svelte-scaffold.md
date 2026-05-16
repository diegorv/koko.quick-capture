Status: ready-for-agent

# Tauri 2 + Svelte scaffold

## Parent

[v0-1-mvp PRD](../PRD.md)

## What to build

Bootstrap the project as a Tauri 2 desktop app with a Svelte frontend. macOS-only target. Bundle identifier `com.koko.quick-capture`. The result is a runnable shell with a single empty main window — no Capture logic yet — that compiles, launches, and quits cleanly. This is the foundation every later slice builds on.

Choose Vite + plain Svelte (not SvelteKit) and pnpm unless there is a concrete reason to deviate. Keep configuration minimal: the goal is a working baseline, not a feature-rich template.

The Rust side is wired up with the workspace and dependencies that v0.1 will need (`rusqlite`, `ulid`, `tauri-plugin-global-shortcut`), even though they are not invoked yet, so later slices add usage rather than packaging.

Test tooling is set up in this slice (per ADR-0005) so later slices add tests rather than wiring up the harness:

- Rust: stdlib `#[test]` works out of the box; create an empty `src-tauri/tests/` directory with a placeholder integration test that asserts `true` to confirm the integration harness compiles.
- Svelte: install `vitest` and `@testing-library/svelte`. Add a `pnpm test` script that runs Vitest in run-mode. Ship one trivial component test (e.g. mount the empty main view, assert it renders without throwing) to prove the harness is wired.

## Acceptance criteria

- [ ] Repo has a working Tauri 2 + Svelte project that builds with `pnpm tauri build` and runs with `pnpm tauri dev` on macOS.
- [ ] Bundle identifier is `com.koko.quick-capture`.
- [ ] Target platforms config excludes non-macOS platforms.
- [ ] Frontend is plain Svelte via Vite (not SvelteKit). pnpm is the package manager.
- [ ] `Cargo.toml` declares `rusqlite`, `ulid`, and `tauri-plugin-global-shortcut` as dependencies (no usage required yet).
- [ ] Launching the app shows a single empty window titled "quick-capture".
- [ ] `README.md` (or equivalent in package.json scripts) documents how to run dev and build.
- [ ] No lint or type errors on `pnpm check` / `cargo check`.
- [ ] `vitest` and `@testing-library/svelte` are installed; `pnpm test` runs and passes one trivial component test that mounts the main view.
- [ ] `cargo test` runs and passes a placeholder integration test under `src-tauri/tests/`.
- [ ] All checks green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed in one commit using Conventional Commits per ADR-0006.

## Blocked by

None - can start immediately.
