# serde_json vendor provenance

`vendor/serde_json/` is a vendored copy of `serde_json` 1.0.151, used via a
`[patch.crates-io]` path override in the root `Cargo.toml`. `arbitrary_precision`
is enabled only for this crate; `raw_value` is not enabled.

## Upstream identity

- Package: `serde_json`, version `1.0.151`.
- Upstream VCS commit (from the registry copy's `.cargo_vcs_info.json`,
  verified on this machine at
  `~/.cargo/registry/src/*/serde_json-1.0.151/.cargo_vcs_info.json`):
  `de8500740cdcabffb9734f503e4889def823cf10`.
- Registry `.crate` checksum: `c841b55ecdae098c80dcae9cf767f6f8a0c2cdb3416bbef72181df4d0fe73f14`
  (as recorded by the task packet; **not independently verified** — no cached
  `.crate` archive was present on this machine under
  `~/.cargo/registry/cache/*/serde_json-1.0.151.crate` to hash).

## What was copied

Only the crate's production source was vendored:

- `vendor/serde_json/src/` (library source)
- `build.rs`, `Cargo.toml`, `LICENSE-APACHE`, `LICENSE-MIT`

No `tests/`, `benches/`, or `examples/` directories were copied (the upstream
registry copy has a `tests/` directory; it was not brought into `vendor/`).

## What was patched

Verified by `diff -rq` between the unmodified registry copy of 1.0.151 and
`vendor/serde_json/src/` on this machine. Exactly three files differ from
upstream:

- `src/de.rs`
- `src/number.rs`
- `src/value/de.rs`

(`src/lib.rs` and `src/macros.rs` reference the existing
`arbitrary_precision`/private-number machinery but are byte-identical to
upstream — they were not patched.)

The patch's purpose: keep the exact numeric lexeme (origin) alive through
Serde `Content` newtype/flatten boundaries, while leaving ordinary authored
JSON maps as maps. Specifically:

- `ParserNumber` gained a `String` variant so the deserializer can carry a
  captured lexeme through the normal number-parsing paths.
- `Number`'s owned/ref `Deserialize` impls dispatch through a synthetic
  `visit_newtype_struct` so a `Number` can be reconstructed from a private
  inner `Deserializer` holding that lexeme, instead of only from the
  `arbitrary_precision` map-wrapper protocol.
- `NumberVisitor` / `value::de::ValueVisitor` restore a `Number` from that
  private inner `Deserializer`.
- Ordinary `Value::Object` maps no longer mistake a user-authored map whose
  key happens to equal the private number sentinel (`$serde_json::private::Number`)
  for a wrapped `Number` — they stay maps.

## Licenses kept

Both `LICENSE-APACHE` and `LICENSE-MIT` are carried over unmodified in
`vendor/serde_json/`, per the dual-license terms of the original crate.

## Known residual risk (see also json-number-origin-contract.md)

`serde_json::Number::serialize` (via `Value::serialize`) still emits the
private `arbitrary_precision` wrapper protocol for any serializer that is not
`serde_json`'s own `Serializer`/`Deserializer` pair (for example TOML or
YAML). This repository never calls `Value::serialize` directly for TOML/YAML
output — see `src/json_native.rs` and its contract doc.
