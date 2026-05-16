# quick-capture

Frictionless macOS capture inbox. Tauri 2 + SvelteKit + Rust.

See `CONTEXT.md` for the domain glossary, `docs/adr/` for architectural decisions, and `.scratch/v0-1-mvp/` for the active plan.

## Requirements

- macOS
- Rust toolchain (`rustup`)
- Node.js + pnpm 10

## Dev

```sh
pnpm install
pnpm tauri dev
```

## Build

```sh
pnpm tauri build
```

## Checks

```sh
pnpm test     # Vitest (Svelte components)
pnpm check    # svelte-check (types)
cargo test    # Rust unit + integration tests (run from src-tauri/)
cargo check   # Rust typecheck (run from src-tauri/)
```

Every slice must leave all four green before commit (see ADR-0006).
