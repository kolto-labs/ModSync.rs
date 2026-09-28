# Source-normalization plan review

Reviewer session `7d613fee-5036-4826-8f25-6a707bc06374`, Sonnet low, independent read-only source and evidence review. No implementation, Rust tests, or harnesses. F01/F05 remain partial.

## First review

Confirmed: `ModComponentSerializationService.cs:2141-2144` uses `string.Equals(strAction, "Choose", StringComparison.OrdinalIgnoreCase)` — case-insensitive. This is a real, previously-unstated implementation requirement the plan's "Skip entirely for `Action == "Choose"`" wording (plan.md:170) doesn't make explicit, and it matters because `normalize_component` runs pre-canonicalization.

I have sufficient independent findings. Verdict:

## Verdict: NOT APPROVED — pending wording/scope corrections only; implementation boundary is sound

**Implementation boundary (model.rs:560, :622-641, :638-640; source_key at :431-463) is correct and bounded.** The proposed single Source step inside the existing `normalize_component`/Options walk, reusing `fix()` (model.rs:433-446) and the placeholder list (model.rs:452-457), doesn't require new recursion structure, doesn't touch `paths.rs` (paths.rs:14-40, 61-90 verified unchanged/sufficient), and is consistent with dedup/`source_key` (model.rs:431). This can proceed as scoped.

**Required corrections before the plan is approved:**

1. **Date is wrong the other direction than I first read it.** `date -u` on this host shows local time is CDT (UTC−5); `receipt.json` mtime `2026-09-27 22:07 -0500` = `2026-09-28 03:07 UTC`. The C# log timestamps (`csharp-install.log`: `09/27/2026 22:06:21`, local) confirm the same offset. So **both** headings should read **2026-09-28 UTC**, not just the "Expanded" section — plan.md:73's "(2026-09-27 UTC)" is the one that's wrong; plan.md:55's "(2026-09-28 UTC)" is correct. Parent's correction direction holds.

2. **"F01/F05 close-out" (plan.md:3-5, :73) is overbroad — confirmed independently.** ACCEPTANCE.md:45,49 define F01 and F05 as whole-format/whole-instruction-action matrices already marked "partial" across dozens of prior entries. This round covers one field (`Source`) normalization for JSON+TOML+YAML+XML plus one Markdown route, explicitly excluding the legacy-bullet route (finding 6) and format-context plumbing. Should read "evidence for the Source-normalization gap in F01/F05" — not "close-out." Both remain PARTIAL.

3. **model.rs:170's Choose-skip must be explicitly case-insensitive.** `normalize_component` runs on the raw `serde_json::Value` before the `action()` deserializer (model.rs:536-559) canonicalizes casing, so a raw `"choose"`/`"CHOOSE"` action string would reach the new Source step unless it's checked with `eq_ignore_ascii_case` (matching the existing Delete-check pattern at model.rs:628-631, and matching C#'s own `StringComparison.OrdinalIgnoreCase` at `ModComponentSerializationService.cs:2141`). This is a required implementation note, not optional.

4. **plan.md:87 "identical guide" doesn't hold — it's two different files.** `csharp-install-guide.json` (component `csharp_relative_install`, GUID `…070`) and `guide2.json` (component `copy_relative`, GUID `…031`) differ (`diff` output above); only Source/Destination shape matches. Reword to "the same Source shape."

5. **Validation-equivalence overclaim.** `--skip-validation` was used for the real C# install (receipt.json `csharp_installs_unprefixed_source`); no C# run without it exists in the evidence. The claim "it does not disable path safety checks in PrefixPath/FixPathFormatting" is asserted, not demonstrated by any captured artifact — mark unresolved, narrow to "Copy execution and byte output are proven equivalent; validation-path equivalence is not."

6. **Edge probes are JSON-only for the structured-format matrix.** `structured_source_values` (receipt.json) ran only `--format json`; the TOML/YAML/XML comparison (`source-normalization-formats/receipt.json`, cited plan.md:67-71) only reproduced the original five baseline shapes, not the nine expanded edge cases (empty array item, nested Options, traversal, absolute, placeholder case). Given C#'s own comment that XML previously diverged in backslash handling (`Serializer.cs:152-154`), this gap should be named explicitly as unmeasured rather than implied covered by "five formats."

7. **Markdown embedded-route comparison is YAML-only, not "YAML/TOML."** `embed.md`/`embed-csharp.json`/`embed-rust.json` test one YAML fixture; no TOML embedded-block probe exists in this directory. Finding 5's prose (plan.md:130-131, "share the ... embedded YAML/TOML block") should say the direct comparison covered YAML; TOML sharing the same code path is inferred from source reading, not independently probed.

**Everything else checked out:** binary hashes (`c3e2f60f…`, `89d9b236…`) match live files and SHA256SUMS passes clean; C# checkout is confirmed at `d31ccbd3` per plan.md:16-17; all cited C#/Rust line numbers (Serializer.cs:129-178,233-241; ModComponentSerializationService.cs:2141-2144; model.rs:431-463,560,622-641; paths.rs:14-40,61-90) match file contents exactly; the byte-identical Copy claim (`5891b5b5…`) is independently reproduced in both `csharp-install.log` and `install_normalized.log`; `guide.json` cited at receipt.json's `install_rejection.guide` field does not exist in the directory (only `guide2.json` — likely a stale reference from an earlier probe iteration; confirm before merging).

## Re-review after corrections

XML pre-normalizes `Source` into a JSON array before `Guide::from_value` (xml.rs:160-164), so it's always array-shaped by the time `normalize_component` sees it — no wrapped/object shape reaches the new helper. Check 3 clears: a model.rs-only helper handling string-or-array is sufficient for all four structured formats including XML.

All three potential blockers resolve in the plan's favor. Verdict: **APPROVED**.

## Verdict: APPROVED

**claim: plan.md:31, "Skip Choose case-insensitively" is grounded at both C# call sites**
referent: `ModComponentSerializationService.cs:2141-2144` and `:2350-2353`
checked: both sites read `if (!string.Equals(strAction, "Choose", StringComparison.OrdinalIgnoreCase))` — identical, case-insensitive
verdict: holds
confidence: high

**claim: plan.md:46, "Traversal... and symlinks must remain rejected at execution" describes existing behavior, not a new requirement**
referent: `src/paths.rs:100-119` (`Roots::check`)
checked: loop walks every path component under the resolved root and calls `fs::symlink_metadata` per-component, erroring `"Symbolic links are not allowed"` if any component is a symlink — this is already in production code, unchanged by the plan
verdict: holds
confidence: high

**claim: plan.md:26 "Own `src/model.rs` only" is sufficient for XML too**
referent: `src/xml.rs:153-164`
checked: XML's `normalize()` unconditionally coerces `Source` into a `Value::Array` before the value reaches `Guide::from_value`/`normalize_component`, so the model.rs helper's string-or-array contract (plan.md:34-35) covers XML with no separate XML-side change
verdict: holds
confidence: high

**claim: plan.md:76-78, ModBuildConverter.cs:3576 gates full validation**
referent: `CLI/ModBuildConverter.cs:3576`
checked: `if (!opts.SkipValidation) { ... "Running full installation validation (wizard-equivalent pipeline)" ... }`
verdict: holds
confidence: high

**claim: plan.md:75, guide.json/guide2.json distinction is now accurate ("different component identifiers; not identical guides")**
referent: `receipt.json:124`, `diff csharp-install-guide.json guide2.json`
checked: receipt.json:124 explicitly flags the missing original `guide.json` as non-reproducible; the retained diff shows only GUID/name differ, Source/Destination shape matches
verdict: holds
confidence: high

**claim: mixed-case placeholder measured failing in both engines, at different points (plan.md:81-82)**
referent: `mixed.log` ("Path must start with... <<MODDIRECTORY>>..."), `csharp-mixed.log` (case-sensitive filesystem lookup, `MissingSourceFiles`)
checked: Rust fails at `paths.rs:67` exact-case `strip_prefix`; C# fails later at case-sensitive file lookup — genuinely different failure points, correctly described as "fail... at different points," not claimed equivalent
verdict: holds
confidence: high

**claim: captured-commands.json contains only disposable-root commands, no reasoning**
referent: `captured-commands.json` full grep of shell_command fields and `--bind`/`cd` targets
checked: every command operates under `/tmp/srcnorm-expanded` or copies into the owned `docs/parity/source-normalization-expanded` output dir; no reasoning/narrative text found
verdict: holds
confidence: high

**claim: SHA256SUMS and binary hashes are internally consistent**
referent: `sha256sum -c SHA256SUMS` (all OK), `sha256sum wave2-reviewed-binary` = `c3e2f60f…`
checked: clean pass, matches plan.md:64-67
verdict: holds
confidence: high

Minor non-blocking polish (does not block approval): plan.md:33 "Keep all existing deduplication and Choose-reference remapping behavior" could note that filtering empty array items changes dedup *outcomes* by design (e.g. `["","x"]` now equals `["x"]`), not just that the algorithm is unchanged — worth one clause if the implementer re-runs the existing dedup fixtures and sees a diff. Everything else in the plan, receipt, and contract is independently supported by file:line evidence and matches captured raw output, not just receipt prose.
