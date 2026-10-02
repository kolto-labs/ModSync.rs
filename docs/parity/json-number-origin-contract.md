# JSON number-origin contract (TOML / YAML output)

## Problem

With `arbitrary_precision` enabled, `serde_json::Number` stores the exact
source lexeme and `serde_json::Number::serialize` (reached through ordinary
`Value::serialize`) emits a private newtype-struct protocol
(`$serde_json::private::Number`) that only `serde_json`'s own
`Serializer`/`Deserializer` understand. Any other `Serialize` consumer of a
`serde_json::Value` — `toml::to_string_pretty`, `serde_yaml_ng::to_string` —
sees that protocol as an ordinary map/struct and emits a wrapper map instead
of a native number. This repository must never let a `Value` containing a
`Number` reach such a serializer directly.

## Adapter

`src/json_native.rs` provides `to_toml_pretty` and `to_yaml_string`. Both
wrap the `Value` in a private `NativeValue<'a>` type whose `Serialize` impl
mirrors `serde_json::value::ser::impl Serialize for Value` arm for arm
(`serialize_unit`, `serialize_seq(Some(len))`, `serialize_map(Some(len))`,
same traversal order) for every variant except `Number`, which is rendered
natively:

- Lexeme has no `.`, `e`, or `E` → **integer**. Parsed as `i64` first; if
  that fails, as `u64` (TOML only accepts `i64` — see below); any value that
  fits neither is an explicit serialization error, never a float.
- Otherwise → **decimal/exponent**. Parsed as `f64` via `str::parse`.
  `-0.0` round-trips exactly (`"-0.0".parse::<f64>()` preserves the sign
  bit). A lexeme that overflows to a non-finite `f64` (e.g. `1e400`) is an
  explicit serialization error, never silently coerced to `0`/`Infinity`/a
  wrapper map.
- TOML only has a signed 64-bit integer type; a `u64`-only lexeme (value
  `> i64::MAX`) is passed to `serializer.serialize_u64`, which the TOML
  serializer itself rejects with its own error — this repository does not
  special-case it further, since `toml` 0.9 has no 65th bit to put it in.

Because every nested element always recurses through `NativeValue` (never
through `Value::serialize`), an ordinary JSON map whose key happens to equal
the private sentinel string stays an ordinary map in the output — there is
no special-casing of that string anywhere in the adapter.

Because the non-`Number` arms copy the original `Value::serialize` behavior
exactly, TOML/YAML output for everything *except* numbers is byte-identical
to before this change.

### `-0` (integer-shaped) vs `-0.0` (decimal-shaped)

Before this patch (non-`arbitrary_precision` build), the legacy JSON parser
read source text `-0` as `ParserNumber::F64(-0.0)` — i.e. an integer-looking
zero with a leading minus was promoted to a negative float, and TOML/YAML
output (and the scalar-text getter in `src/model/getter.rs`) would show
`-0.0` / `"-0"`.

With `arbitrary_precision` now always enabled for this crate, the exact
source lexeme `-0` is retained verbatim and classified by this adapter using
lexeme shape (presence of `.`/`e`/`E`), not by the legacy float-promotion
rule. `-0` has neither, so it is treated as integer, parsed as `i64`
(`"-0".parse::<i64>() == 0`), and serializes as plain `0`. The scalar-text
getter's `wide_integer_lexeme` check is also lexeme-shape-based, so a literal
JSON `-0` scalar now formats as `"0"` rather than `"-0"`.

This is an observable change from the pre-patch build. It matches the pinned
reference: Newtonsoft reads a JSON `-0` integer token as `long` 0, and Tomlyn
reads a TOML `-0` integer as 0. Neither keeps a negative zero for an
integer-shaped lexeme.

## JSON output

`arbitrary_precision` also changes JSON saves: `serde_json` now writes each
number's source lexeme unchanged. `1e2` stays `1e2` and an integer beyond the
64-bit range keeps all its digits. Before, numbers outside the integer range
were rounded through `f64` and written in Rust's float form (`100.0`). The
reference rewrites floats through `double` (`100.0`, `1E+300`). Neither build
matched it exactly. Keeping the authored text is the chosen JSON behavior; it
is not a claim of byte parity with Newtonsoft output.

## Bounds

`ValueBudget` (`src/json_native.rs`) is a single mutable instance created
once per `render()` call (in `src/model.rs::Guide::render` and
`src/markdown/export.rs::render`) and threaded by `&mut` through every
TOML/YAML call in that invocation — the YAML metadata block, every
component, and (in Markdown export) every per-component embedded YAML
block. Its `check` is called immediately before each document is handed to
its serializer, so it preflights *that* document, not the whole save in one
pass up front: if component 3 of 10 trips the shared budget, components 1
and 2 (and the metadata block) have already been fully encoded and
allocated. This is deliberately a "reject before serializing this one, but
after allocating the previous ones" check, not a true single up-front
rejection — stated explicitly because the alternative (walk every value in
the save before serializing any of them) was not implemented to keep the
change localized to each existing call site.

Per-document walk charges, before any serializer runs:

- **Logical bytes** (cap 8 MiB, shared across the whole save): string value
  bytes, object key bytes, and number lexeme bytes (`Number::as_str().len()`).
  Container punctuation, indentation, and encoded-output expansion are not
  counted here — see "Encoded-output acceptance" below.
- **Node count** (cap 100,000, shared across the whole save): every scalar,
  array, and object counts once.
- **Depth** (cap 128, per document, not shared): root value is depth 0; up
  to 128 further nested levels are allowed below it (i.e. depth values
  `0..=128` are accepted, `129` is rejected).

This preflight is independent of, and in addition to, the pre-existing 8 MiB
*encoded output* acceptance check already present elsewhere in the codebase
(`src/model/getter.rs::Context::charge`, `MAX_OUTPUT`) — that check is on the
rendered text size for the scalar/list getters and is unchanged by this
work. The two 8 MiB caps are deliberately separate instruments measuring
different things (logical input size here vs. rendered getter-text output
there) and are not meant to be unified.

### Derived finite-formatter expansion bound (informational, not enforced)

A number's *logical* lexeme (what the preflight counts) can be much shorter
than its *encoded* output once a float formatter re-expands it:

- **TOML** (`toml_writer` 1.1.2, `src/value.rs`, read from the vendored
  registry copy): a finite, non-zero `f64` is formatted as `{self}` (plain
  `Display`) plus a trailing `.0` when the `Display` output contains neither
  `.` nor `e`. Rust's `f64` `Display` is not scientific notation — for an
  extreme value like `1e308` this expands to roughly 309 decimal digits plus
  a sign and the `.0` suffix (~311 bytes); `5e-324` (the smallest subnormal)
  expands to roughly 324 leading zeros plus digits (~327 bytes).
- **YAML** (`serde_yaml_ng` 0.10.0, `src/ser.rs`): `serialize_f64` uses
  `ryu::Buffer`, whose output is always shortest round-trippable decimal,
  bounded at 24 bytes regardless of magnitude.

So a conservative **writer-side expansion bound** for one document is:
`logical_bytes + nodes_that_are_numbers × 327` (using the TOML worst case,
since it dominates YAML's 24-byte ryu bound). This is *derived* from reading
the formatter source above, not independently enforced anywhere in this
change — the only rejection this repository performs is the preflight
`ValueBudget` check on logical size/node count/depth, before serialization.
If a pathological document passes that preflight (e.g. many small numbers
with huge-magnitude values) and then produces an very large encoded TOML
string, the existing getter-text `MAX_OUTPUT` check only applies to the
getter paths, not to `Guide::render`'s TOML/YAML output — this is the
"real writer bound vs. rejection after allocation" distinction: the
preflight rejects *before* allocating the serializer's output buffer; the
TOML-float-expansion worst case is a property of that writer's output once
allocation has already happened, and nothing here re-checks the output
after the fact.

## Coercions beyond the adapter

`src/model/getter.rs`, `src/model/action.rs`, `src/model/boolean.rs` read
`serde_json::Value` numbers via `as_i64`/`as_u64`/`as_f64`. All three now
call `crate::json_native::wide_integer_lexeme` first: it returns `Some(lexeme)`
only when the lexeme is integer-shaped (no `.`/`e`/`E`) and both `as_i64`
and `as_u64` fail (i.e. the integer genuinely exceeds `u64::MAX` or is more
negative than `i64::MIN`).

- **Boolean** (`optional_for_format`/`optional`): wide integer → `false`.
  A decimal/exponent lexeme where `as_f64` returns `None` (non-finite, e.g.
  `1e400`) is **not** treated as `false` — it is an explicit error, since
  `as_f64`'s `None` can only mean non-finite in that branch once the
  wide-integer case has already been excluded.
- **Action** (`action::numeric`): wide integer → `None`, which `intake`
  already treats as the `Unset` default. A non-finite decimal/exponent
  value is likewise an explicit error rather than falling through to
  `Unset`.
- **Getter scalar/collection text** (`getter::Context::primitive`): wide
  integer in a collection → exact decimal digits via the new
  `signed_digits`/`format_signed_digits`, reusing the existing lazily
  acquired `NumberCultureSnapshot` for the `NegativeSign` only when the
  value is negative (mirroring `signed`'s existing behavior of never
  touching the culture snapshot for non-negative values). Wide integer as
  scalar text (not a collection) has no native-type evidence, exactly like
  the existing `u64`-too-wide case just above it: empty string for
  `Format::Json`, explicit error otherwise.

## Compatibility risk: third-party `visit_map`-only visitors

This patch makes `Number`'s `Deserialize` impl able to reconstruct a value
from either the original `arbitrary_precision` map-wrapper protocol *or*
the new `visit_newtype_struct`-based private-lexeme path. A custom
third-party `Visitor` that implements only `visit_map` to intercept the
`arbitrary_precision` wrapper (a known pattern for inspecting the private
protocol) will not receive a `visit_newtype_struct` call for values that
flow through the new path, and so will not see numbers produced that way.
This is a real compatibility risk for any code outside `src/json_native.rs`
that depends on observing `serde_json`'s private number wrapper shape
directly instead of going through `Number`'s public API.

This patch is **not** a general guarantee that arbitrary third-party `Serde`
`Content`-based renderers (debug dumps, generic "render any `Serialize`"
helpers, etc.) will show native numbers — only `src/json_native.rs`'s
`NativeValue` adapter is guaranteed to do so. Any other code that calls
`Value::serialize` directly, or passes a `Value` to a generic non-JSON
`Serializer`, still risks emitting the private wrapper protocol.
