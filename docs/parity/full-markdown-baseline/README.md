# Full Markdown conversion output comparison

This is a comparison of eight retained production conversion outputs, not an install-parity receipt. No converter, build, test, reload, network operation or installer was run for this analysis. Input/output baseline files and production files were unchanged; only external analysis artifacts were written.

## Evidence and reproduction

The external baseline contains `receipt.json`, eight `k{1,2}-{rust,csharp}-{plain,draft}.json` outputs, and their corresponding `.receipt.json` command receipts. Those command receipts retain exact argv, exit, stdout/stderr and input/output hashes; do not copy their private content into this repository. All eight recorded exits are zero, and this analysis independently verified each output SHA256 against its command receipt and equal input hashes within each pair.

The author-supplied source binding is Rust `62b6f70` and pinned C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. The baseline receipt binds these executable artifacts:

| Artifact | SHA256 |
| --- | --- |
| Rust executable | `a665ace4f9db3f9ae4dcf6be95b075e1b5ff1d6f822ed0885493e984ffca31b8` |
| C# DLL | `89d9b2361aee4364f95151b41db59a4133122a6ad41bc2703e232cf10a4f5a6b` |
| External aggregate receipt | `bfcc60413c55df3d90aabb17e20841abf0a1840462d9f33d9f3322a7b4cb334c` |

Reproduce the compact anonymous summary from the existing external baseline, without running either engine:

```sh
python3 generate_summary.py "$BASELINE_DIRECTORY" "$ANALYSIS_DIRECTORY/summary.json"
```

The generator refuses mismatched receipts, different paired input hashes, different tree shapes, duplicate GUIDs, mismatched ordinal Name/Author identities or unequal instruction counts. It does not write its inputs. `summary.json` retains per-command receipt/output/input hashes, raw field-presence differences and both instruction interpretations. The multi-megabyte detailed comparison remains external.

## Compared content

| Pair | Root components | Option nodes | Instructions including options | Resources |
| --- | ---: | ---: | ---: | ---: |
| K1 plain | 186 | 0 | 0 | 198 |
| K1 draft | 186 | 1 | 72 | 198 |
| K2 plain | 145 | 0 | 0 | 153 |
| K2 draft | 145 | 3 | 56 | 153 |

Every paired component/option has exactly matching Name and Author at the same ordinal tree position. All generated GUIDs differ. Each output's GUIDs were independently mapped to those verified ordinal paths before comparing relationship references. Under that identity mapping, there are zero differing shared values in root/option metadata, relationships, resource identities or resource metadata. Missing fields remain separately recorded; GUID normalization does not turn these into raw-equal documents.

Draft action distributions also match: K1 has 40 Move, 8 Patcher, 21 Delete, 1 Extract and 2 CleanList instructions; K2 has 29 Move, 6 Patcher, 17 Delete, 1 Rename, 1 Extract and 2 CleanList instructions. Source, destination, arguments and relationship fields match after separately accounting for conditional omission of empty fields.

## Three distinct comparisons

1. **Raw export:** Rust explicitly writes `Overwrite=false` on 21 K1 and 17 K2 Delete instructions. C# omits that field. Other omitted instruction fields include empty Arguments and Destination; the summary retains their presence differences.
2. **Model-default interpretation:** Both instruction models default missing Overwrite to true. Applying that default yields 21 K1 and 17 K2 differing instructions, all on Overwrite. This is a source-based interpretation of serialized files; no reload was executed.
3. **Draft-generation reconstruction:** C#'s natural-language drafter initializes Delete with Overwrite=false, matching Rust. Reconstructing that specific serializer omission instead yields zero differing instructions. This is generation-intent equivalence, not raw equality or reload equivalence. It must never silently replace comparison 2.

Source referents for these distinctions are pinned C# `src/ModSync.Core/Instruction.cs:63-66,209-220` (defaults and action-dependent serialization), `src/ModSync.Core/Parsing/NaturalLanguageInstructionParser.cs:884,943-950` (draft initialization and prose overrides), and inspected Rust `src/model.rs:63-84` and `src/draft.rs:287,314-317`. C# output omission cannot prove every in-memory per-instruction value: the generation reconstruction uses those source rules, while raw/model-default differences remain retained.

## Metadata preserved rather than erased

Rust adds nonempty `NameFieldContent` to all 186 K1 and 145 K2 roots. It retains one unresolved master reference on each of K1 ordinals 47 and 182 and K2 ordinals 67 and 140. Draft outputs additionally retain `DraftReview` on 68 K1 and 62 K2 roots. These are observable Rust-only fields, not normalization noise; their presence and component ordinals remain in the summary.

C# emits resource defaults `FileSize=0` and `UpdateAvailable=false` on all 198 K1 and 153 K2 resources; Rust omits them. C# also emits empty root/option metadata fields omitted by Rust. The summary records these differences without inventing values or removing the additional Rust diagnostics.

## Limits

These results establish correspondence of the retained conversion outputs within the stated comparisons. They do not establish that all prose was converted into executable instructions, that unresolved dependencies are harmless, that generated instructions cover a complete guide, or that any install succeeds. Plain outputs contain no generated instructions; equal draft counts are not guide coverage. No live host, download, archive, game-tree mutation, install-order execution, runtime acceptance or F29 parity was validated here. Baseline execution environment and no-write guarantees belong to the original command receipts; this analysis independently establishes only output receipt consistency and comparison results.
