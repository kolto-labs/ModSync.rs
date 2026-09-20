# Task 1 report: shared GUI/CLI selection

## Result

Implemented shared category/tier matching in `src/selection.rs` and routed CLI
selection preparation through it. Existing GUI tier/category helpers now use the
same predicate without changing their additive mutation behavior.

## Behavior and reference evidence

- C# `ModBuildConverter.ApplySelectionFilters` resets every top-level component,
  ORs values within category and tier groups, and ANDs the two groups. Rust CLI
  now does the same for `category:Name` and `tier:Name` selectors.
- C# tier normalization recognizes Essential, Recommended, Suggested, and
  Optional names and applies priority-through-tier selection. The shared Rust
  predicate accepts those names, full labels such as `2 - Recommended`, numeric
  labels, and case variants.
- C# ignores malformed prefixed filters after warning. Rust emits a selection
  warning and, when no valid selector remains, follows the reference fallback of
  selecting all components.
- A valid unmatched category/tier filter selects zero components without being
  treated as malformed, matching the reference filter result.
- Rust's prior bare component name/GUID extension remains supported. Unknown
  bare values still fail atomically instead of silently changing selection.
- GUI `select_tier` and `select_categories` remain additive and continue to
  exclude widescreen-only components. They share matching only; CLI remains a
  reset operation and does not inherit the GUI base-build exclusion.

## Tests

- Focused: `CARGO_TARGET_DIR=/tmp/modsync-rs-target cargo test --test selection`
  — 13 passed.
- Required formatting and lint: `cargo fmt --all -- --check` and
  `cargo clippy --all-targets -- -D warnings` — passed.
- Required default-feature suite: `cargo test --all-targets` — passed; 204
  passed and 1 explicitly ignored (513 MiB streaming regression).
- Required reduced-feature suite: `cargo test --no-default-features` — passed;
  182 passed and 1 explicitly ignored.

All Cargo commands used `CARGO_TARGET_DIR=/tmp/modsync-rs-target`.

## Limitations

- This task does not port the C# selection UI. It only converges the shared
  category/tier predicate and CLI preparation while preserving the existing Rust
  GUI actions.
- Mixed bare exact selectors and category/tier filters are a Rust extension:
  exact matches are unioned with the C#-style grouped filter result.
- CLI filtering changes top-level component flags only, as the reference CLI
  filter does; GUI actions still run the existing dependency-resolution flow.
