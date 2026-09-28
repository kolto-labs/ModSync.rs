# Stored Source normalization: production evidence

2026-09-28 UTC. Implementation commit `62b6f707147b734714e4dda7f46118beff3d6140` changes only `src/model.rs`. The existing path rewrite now normalizes stored non-Choose Sources and equality before duplicate-option removal. Choose is exempt case-insensitively. Non-Choose arrays filter blank strings while retaining malformed members for established errors; bare blank scalar strings remain available for downstream errors. Recursive Options use the same rewrite. Destination, arguments, execution path checks, Markdown parsing, exports and selection modes were not changed.

F01/F05 remain **partial**. This is direct production CLI evidence, not Rust tests, full-format/action parity, desktop acceptance, live-game proof or full-build acceptance.

## Binding and reproduction

[gates.json](gates.json) records production commands and build/fmt/strict-Clippy logs. Both CLI-only and default builds/checks used the existing debug target `/home/brunner56/.cache/modsync-rs-target-lead`, `--lib --bin modsync`, and `-j2`. No release build, new target, all-target build, Rust test or custom Rust harness was used. Final default binary SHA-256: `a665ace4f9db3f9ae4dcf6be95b075e1b5ff1d6f822ed0885493e984ffca31b8`.

Pinned C# DLL SHA-256: `89d9b2361aee4364f95151b41db59a4133122a6ad41bc2703e232cf10a4f5a6b`, reference source `d31ccbd3`. Each C# call uses bubblewrap with read-only host, isolated network, mounted `/dev` and `/proc`, and only a disposable fixture root writable; HOME/XDG/TMPDIR are inside it.

[freeze.json](freeze.json) compares 53 whole files or complete test suffixes against `9ecf8e4`: AGENTS, Cargo manifests, all tracked test files, and the seven pre-existing `cfg(test)` suffixes. All are byte-identical. The existing frozen test/CI conflict remains unresolved.

[probe.py](probe.py), [supplement.py](supplement.py), [safety.py](safety.py), and [validation.py](validation.py) drive real production executables with generic disposable fixtures. Adjacent `*.receipt.json` files preserve exact argv, exit, stdout and stderr; inputs and conversion outputs are retained. Reproduction assumes the recorded pinned binaries and reference fixture directories remain available. Scripts reconstruct fresh temporary roots. [verify.py](verify.py) validates the retained results; [verification.json](verification.json) records 68 passing assertions without compiling or executing Rust tests. [SHA256SUMS](SHA256SUMS) covers all proof files except itself.

## Measured outcomes

| Evidence | Result and limit |
|---|---|
| [receipt.json](receipt.json), `matrix-*` | Twelve Source shapes match C# across JSON/TOML/YAML/XML: relative, filtered arrays, backslashes, repeated separators, three placeholders, uppercase placeholder spelling, unknown placeholder, drive path, absolute-looking path and traversal; also includes an option. Each format's normalized projection is idempotent after JSON save/reload. This is Source projection equality, not full-document serialization parity. |
| `scalar-*` | Directly authored relative/empty/whitespace/numeric scalars in all four formats. Relative values match. JSON/TOML/YAML blank scalars remain in Rust but C# rejects them. XML already turns Source into arrays, so blank text becomes an empty Rust list. Typed numeric values remain Rust errors while C# silently empties Source; XML numeric text becomes `<<modDirectory>>/42` in both. |
| `embedded-*` | Common Markdown embedded YAML **and TOML** yield matching Source projections in both engines. |
| `dedup-*`, `special-*`, `fenced-*` | Thirteen inherited fixtures match C# option GUID/order and Choose projections. Fixture `g` is fenced Markdown: Rust keeps both options and Choose entries, C# returns no components. A new filtered-array case deduplicates and cleans Choose references before execution, matching C#. Rust-only fenced JSON normalizes Sources but preserves both options and Choose entries, maintaining its exemption. |
| [supplement-receipt.json](supplement-receipt.json) | Missing/null/empty defaults, colon-adjacent separators, leading slashes, mixed-case Choose, two option levels, malformed arrays and unrelated fields are checked per format. Rust reloads are idempotent. Destination/Arguments match the fixed old Rust production output across all four formats. See intentional differences below for `equal: false`. |
| [safety-receipt.json](safety-receipt.json), `copy-*` | Both engines install the **same authored guide bytes**, SHA `03d4f6ef9fca4907a527891abc05b2cf3c674aaf1b40e82189fd856862282e36`, with bare-relative `x/file.txt`. Both write payload SHA `4edc2ebc3b8f737678840127749239b1e288264249da9590249e64b421ee7341`. C# writes additional checkpoint metadata, so whole game manifests differ. |
| `copy-csharp-full.receipt.json` | Full C# validation was attempted and fails because its pinned Resources lack HoloPatcher. Successful Copy uses **`--skip-validation`**; this proves Copy bytes, not full validation equivalence. |
| `absolute-inside*`, `reject-*` | An absolute-looking source copies the different inside-mods payload while the actual outside file's distinct sentinel remains unchanged. Traversal, drive, unknown placeholder, symlink and uppercase placeholder are independently rejected with specific captured errors and no game files. |
| [validation-receipt.json](validation-receipt.json) | Real Rust validation accepts the relative positive and rejects bare empty, bare whitespace and all-filtered arrays. Before/after mod/game file manifests are identical for all four runs. |

The first 94-command matrix pass, final 36-command supplement, corrected nine-command safety pass and four validation calls are separately recorded. These are invocation counts, not a Rust test-suite count or domain coverage claim.

## Differences deliberately outside the source-only fix

- C# discards grandchildren Options in supplemental guides. Rust preserves and normalizes descendants recursively as the approved contract requires. This explains the supplemental `equal: false` values without hiding them.
- C# filters whitespace-only Choose entries. Rust deliberately leaves all Choose Source entries untouched, preserving GUID case and literal backslashes. All formats have direct receipts.
- XML `<Source json:Array="true" />` yields an empty Rust list but this C# path stringifies a dictionary into a prefixed path. Missing Source remains empty in both. TOML/XML null cases are explicitly omission, not claims of native typed-null support.
- C# silently discards numeric members of typed Source arrays; both old/new Rust production binaries reject them. XML values are untyped text. Separate JSON object, bool, numeric-member and null-member failures remain errors.
- Uppercase placeholders remain authored at conversion but the Rust execution resolver still rejects them. Conversion does not establish executable paths.
- Legacy prose/bullet Markdown, broader metadata, full exports, Windows/macOS, GUI and full installs remain unproven by this slice.

## Diagnostic corrections

The first rejection pass shared a mods tree containing a symlink, making unrelated failures inconclusive. The corrected pass creates that symlink only for its own case; final `reject-*` receipts show independent traversal/drive/unknown errors. The main receipt records the safety refresh root and corrected outcomes.

Two supplemental authoring attempts used a YAML `thisMod` wrapper rejected by C# and repeated XML elements each annotated `json:Array`, rejected by Rust. Diagnostic inputs/receipts are retained. Final comparisons use common YAML document streams and standard repeated XML elements, with array annotation only on singletons. Neither diagnostic received a parser change.
