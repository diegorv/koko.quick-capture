# kokobrain destinations: per-destination tags + Link title round-trip

Two small additions to the existing `kokobrain` destination kind introduced by ADR-0012. Neither item changes the lifecycle of a Capture; both extend the deep-link URI emitted on routing so the brain side has richer routing metadata to store on the resulting note.

## Motivation

1. **Multi-tag**: Today the URI carries exactly one tag — the destination name in kebab-case. Power users want to attach additional tags (e.g. `source/quick-capture`, `triage/inbox`, project codes) that vary per destination but apply to every Capture routed to it. The destination name alone is not expressive enough for downstream search/queries on the brain side.

2. **Link title round-trip**: When a Capture's `kind` is `Link`, the URI content is rendered as `[title](url)` so the markdown body is useful by itself. But the brain side has no separate `title:` field in the resulting note's YAML — it has to parse the markdown body to recover the title. The brain side recently grew an optional `title` query parameter on `kokobrain://capture` that injects the value into the note's frontmatter as `title:`. Sending it from quick-capture closes the loop.

## Scope

### In

- New optional `tags` field on `kokobrain` destination config (JSON column on `destinations` table). Free-form user input, comma-separated string in the Settings UI.
- URI builder kebab-cases each user-supplied tag and appends them to the existing kebab destination-name tag, deduplicated.
- For `Link` captures only, the URI carries a `title` query param with the same fallback chain as `content` already uses (`source_title` > `payload.title` > URL).

### Out

- Per-Capture override tags (not user-requested; would expand the picker UX).
- Tags on `label` destinations (kind has no URI side-effect).
- `title` for `Note`/`Clip` captures (there is no semantic "title" — the body is the content).
- Migration of existing destinations: the field is purely additive, so existing rows continue to work with `tags` absent.

## Slices

Each slice is its own commit per ADR-0006. All four checks (`pnpm test`, `pnpm check`, `cargo test`, `cargo check`) must pass before each commit.

- [x] Slice 1 (Rust store): Extend `normalize_destination_config` to accept and persist an optional `tags` array on `kokobrain` configs. Validate each tag is a non-blank string after trim. Re-emit the JSON in normalized form so callers cannot smuggle extra fields. Tests in `tests/store.rs`.
- [x] Slice 2 (Rust kokobrain): Replace `parse_vault` with `parse_kokobrain_config` returning a struct `{ vault: String, tags: Vec<String> }`. Update `build_capture_uri` to kebab-case each user tag, append to the kebab destination-name tag, deduplicate while preserving order (destination name first). Emit `tags=a,b,c` when more than one tag results. Update `kokobrain::mod` tests.
- [x] Slice 3 (Rust kokobrain): For `Link` captures, also append `title=<value>` to the URI using the existing title fallback chain. `Note`/`Clip`/`Shot`/`File` continue to omit `title`. Update tests.
- [x] Slice 4 (Svelte): Add a `tags` input field (string, comma-separated) to the `DestinationsSection` create + edit forms, visible only when `kind === "kokobrain"`. Parse the string into the config JSON `tags` array on submit; render the stored array back as a comma-separated string on edit. Component tests for both forms.

## Risks

- **Tag dedupe order**: putting the destination name first means a user who repeats it in the free-form input ends up with the destination name in the same slot, not duplicated. That is the intended UX. Test covers it.
- **kebab-case of unicode**: existing `kebab_case` preserves non-ASCII alphanumerics, so a destination with accented characters keeps them. The user-supplied tags pass through the same function — consistent behavior.
- **Config schema drift**: the brain side validates configs at write time (Rust `normalize_destination_config`); changing the shape requires every downstream parser to handle the new field. Today the only downstream parser is `parse_kokobrain_config` inside QC; the brain side reads the URI, not the config, so it is unaffected.

## Status

Shipped. Slice 1 (c84319c), Slice 2 (b4ea157), Slice 3 (0ea855c), Slice 4 (this commit).
