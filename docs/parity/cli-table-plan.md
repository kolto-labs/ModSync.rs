# Full CLI parity implementation plan

Status: planned from [cli-table-audit.md](cli-table-audit.md) at Rust `efce242f916020cfac76ea5352b9c75ed3e52c49` (CLI source unchanged from parent `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`) and pinned C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`. F26 remains partial (`docs/parity/ACCEPTANCE.md:5-9,61`). This plan changes no source and authorizes no build, test, release, or user game/mod mutation.

## Binding constraints

- Preserve the user-owned dirty `AGENTS.md` and all unrelated worktree state. This planning pass owns only `docs/parity/cli-table-audit.md` and `docs/parity/cli-table-plan.md`.
- Do not create, modify, port, expand, or run Rust tests or Rust harnesses before full C# parity. Verification in this plan is through production binaries, direct CLI/native invocations, disposable fixtures, stream/exit capture, filesystem manifests and independent source review (`docs/parity/ACCEPTANCE.md:26`).
- Do not build during this audit/plan pass. A future implementation owner uses the host boundary and a private target directory when a production build is actually authorized; the current environment note requires `host-spawn`, `-j 2`, and no shared target directory.
- Keep desktop and CLI installation behavior in `src/engine.rs`; adapters may parse, normalize, format and select a shared entry point, but may not implement a second installer (`AGENTS.md:7-8`; `src/main.rs:897-1163`).
- Validation remains non-mutating and may not run external programs (`AGENTS.md:10`; `src/engine.rs:265-433`). Every validation probe compares before/after manifests of both disposable roots.
- Preserve authored guide order, stop on failure, restore the verified snapshot before retrying, and never backfill an earlier failed mod at the end (`AGENTS.md:11-13`; `src/engine.rs:786-861`).
- Reject unsafe or unsupported C# controls explicitly. Do not silently implement `--best-effort`, continue-on-failure, no-checkpoint, skip-validation, or ignore-dependency-errors as successful no-ops (`AGENTS.md:13`; `docs/parity/ACCEPTANCE.md:75`; C# MBC:625-668,3359-3365,3656-3671).
- Never log, echo, serialize, or retain API keys. The pinned C# key-prefix log is a defect and is not a parity target (C# MBC:4070-4079).
- Preserve every currently documented Rust-only command and JSON contract unless a named migration task and compatibility note explicitly changes it (`src/main.rs:21-364`; `COMPATIBILITY.md:529-579,621-655`).
- Update `COMPATIBILITY.md`, `docs/parity/GUI-CLI.md`, `docs/parity/ACCEPTANCE.md`, and the audit status only after direct evidence lands; a parser/source change is not runtime proof (`AGENTS.md:14`; `docs/parity/GUI-CLI.md:15-17`).
- Do not publish a release or open a pull request as part of this plan (`AGENTS.md:15`).

## Audit conclusions that govern the design

The implementation target is compatibility without reference-defect cloning:

1. Add the missing C# verbs, aliases and defaults listed in [cli-table-audit.md](cli-table-audit.md), while retaining Rust extensions (`C# MBC:364-746`; `src/main.rs:21-364`).
2. Keep conventional successful help/version exits and clean machine JSON even though the pinned C# artifact returned 1 and prefixed profile JSON with a log line; those differences are directly observed under `/tmp/modsync-cli-table-audit-20260927/` and copying them would damage automation (`src/main.rs:12-20`; C# MBC:755-778).
3. Match the C# validation JSON field contract where it is intentional, but preserve truthful Rust diagnostics and exit semantics. The early-error shapes already match and their messages differ (`src/ModSync.Core/Services/Validation/ValidationPipelineJsonFormatter.cs:22-179`; `src/validation.rs:107-145`; `/tmp/modsync-cli-table-audit-20260927/runtime/*validate-missing*`).
4. Resolve all aliases into typed shared requests before execution. Do not scatter alias-specific branches through `run`, `Engine`, profile storage, downloads, or desktop state (`src/main.rs:401-1163`).
5. Treat GUI preload/protocol invocation as an application-launch contract, not a protocol-inspection contract. The current raw Windows URL route is wrong because it reaches JSON-only `ProtocolHandoff` (`src/protocol_registration.rs:19-37`; `src/main.rs:403-415`).

## Shared-library API versus presentation

### Shared request normalization

Create a library-owned module such as `src/cli_contract.rs`, exposed from `src/lib.rs`, only if review confirms the request types are also useful to the desktop or non-Clap callers. Otherwise keep pure argument normalization in `src/main.rs`. The deciding criterion is reuse by at least two production callers; no speculative library API is justified by tests because tests are deferred.

The minimal shared types are:

```rust
pub enum OutputMode {
    Human,
    ValidationReportJson,
    LegacyEventNdjson,
    InstallProgressNdjson,
}

pub struct ConvertRequest { /* resolved input/output/format and supported transforms */ }
pub struct MergeRequest { /* resolved paths/output format/merge::Options */ }
pub struct InstallRequest { /* resolved phase/profile/selection/safety/output controls */ }
pub struct GuiLaunchRequest { /* preload paths, settings store, protocol, instance policy */ }
```

Names may change during review only for a concrete ownership or compatibility reason. Required semantics are fixed:

- C# aliases and existing Rust spellings normalize to one canonical field before any I/O (`C# MBC:385-746`; `src/main.rs:21-364`).
- Conflicting duplicates fail before I/O; do not copy the C# GUI parser's last-value-wins behavior (`C# GUI args:59-96`).
- Exactly one machine-output schema owns stdout. Human diagnostics and progress go to stderr in machine mode (`src/main.rs:1072-1105,1148-1159`).
- Unsupported unsafe flags return a specific nonzero diagnostic before validation, snapshot, download or install. They never become ignored compatibility switches (`docs/parity/ACCEPTANCE.md:75`; `src/engine.rs:786-861`).
- Conversion and merge call shared guide load/render/merge/download orchestration. The `convert --merge` alias must normalize into the same `MergeRequest`; it must not reproduce C#'s lossy adapter (`C# MBC:1905-1943`; `src/main.rs:696-794`).
- Profile/settings verbs call library storage APIs. Presentation controls human text/JSON formatting; storage owns defaults, redaction, atomic writes and locks (`src/profile_cli.rs:42-115`; `src/profiles.rs:12-20,43-89`).
- GUI compatibility flags normalize into the same launch function as `gui`, and raw protocol-handler invocations launch/forward rather than print parse diagnostics (`src/main.rs:417-432,1168-1200`; `src/ui.rs:3179-3233`).

### Presentation-owned behavior

`src/main.rs` owns Clap declarations, compatibility aliases, help text, stdout/stderr routing and process exit selection (`src/main.rs:12-364,1203-1207`). `src/profile_cli.rs` may remain a profile presenter, but it must not own storage semantics (`src/profile_cli.rs:42-115`). Human logging, ANSI/plain text and JSON formatting remain adapter concerns; engine events and validation/install facts remain shared facts (`src/main.rs:1072-1159`; `docs/parity/GUI-CLI.md:38-52`).

## Task sequence and serialized ownership

All tasks below are serialized. No two workers edit `src/main.rs`, `src/profile_cli.rs`, `src/lib.rs`, or `src/engine.rs` concurrently. `src/ui.rs` remains serialized behind the active order **install-progress → completion-pages → mod-selection**; no F26 worker touches it until those owners release it. `src/engine.rs` also requires an explicit ownership handoff before any F26 change.

| Task | Owned files | Responsibility | Start gate |
|---|---|---|---|
| 0. Freeze contract | `docs/parity/cli-table-audit.md`, `docs/parity/cli-table-plan.md` only | Approve table labels, deliberate divergences and defaults | Current documents reviewed independently |
| 1. Parser boundary repair | `src/main.rs`; `src/cli_contract.rs` and `src/lib.rs` only if reuse criterion lands | Restrict raw protocol handoff to `nxm://`/`modsync://`; fix meaningless flag combinations; fix named-output format inference; add C# short aliases where semantics already exist | Exclusive `src/main.rs`; no engine/UI ownership needed |
| 2. Profile and settings surface | `src/profile_cli.rs`; new `src/settings_cli.rs` if needed; existing shared profile/settings modules; `src/main.rs` only for dispatch | Match profile action requiredness/aliases without removing capture/apply; add settings list/get/set with typed values, atomic persistence and default redaction | Task 1 merged; exclusive profile/settings ownership |
| 3. Convert/merge surface | `src/main.rs`; shared convert/merge/download modules only as required | Normalize positional and C# named forms; add explicit format/default behavior; route `convert --merge` to same request; add supported selection/spoiler/local-generation options | Task 2 merged; exclusive `src/main.rs` |
| 4. Validation surface | `src/main.rs`; validation formatter/library only if a schema defect is proven | Add C# `--output text|json`, `-g/-s`, errors-only and supported aliases; make report-mode grammar command-specific; retain legacy NDJSON under an unambiguous Rust extension | Task 3 merged; engine ownership only if source trace proves adapter cannot express contract |
| 5. Safe install surface | `src/main.rs`; download/profile adapters; `src/engine.rs` only after explicit handoff | Add compatible aliases/defaults, source-dir default, download/profile plumbing and explicit rejections for unsafe flags; preserve mandatory preflight/snapshot/order | Task 4 merged; install-progress and engine owners finished/released |
| 6. Missing system verbs | new narrow modules; `src/main.rs` dispatch | Implement safe `set-nexus-api-key` without secret logging; decide/package `holopatcher` and `install-python-deps` only with supported runtime provenance | Task 5 merged; packaging owner approves external-program boundary |
| 7. GUI preload/protocol aliases | `src/main.rs`, `src/protocol_registration.rs`; `src/ui.rs` only if the released UI owner proves state changes necessary | Accept C# preload spellings and `key=value`; route bare URLs and platform handlers into GUI launch/forward; retain explicit `gui` syntax | Active UI serialization chain complete; exclusive `src/ui.rs` if touched |
| 8. Production direct probes | no Rust source/tests/harnesses; evidence directory plus parity docs | Build authorized production binaries once, then run the table corpus against isolated fixtures; capture argv/stdout/stderr/exit/manifests/hashes | Tasks 1-7 reviewed; disk/load gate passes |
| 9. Ledger closure | `COMPATIBILITY.md`, `docs/parity/GUI-CLI.md`, `docs/parity/ACCEPTANCE.md`, audit status | Mark each table row with live evidence and keep unsupported rows explicit; F26 closes only when every required row has a ruling and proof | Task 8 receipts and final independent review accepted |

Workers own only the listed files and must report any expansion before editing. They are not alone in the codebase and must accommodate unrelated changes without reverting them. A commit is not a handoff until its owned-file diff, direct-probe receipt, and claim blocks have been reviewed.

## Detailed implementation requirements

### Task 1 — parser boundary repair

- Replace the unrestricted external-subcommand behavior with a raw-argument pre-parser or a constrained launch path that accepts exactly one bare `nxm://` or `modsync://` URL and rejects other unknown verbs through normal Clap usage (`src/main.rs:23-24,403-415`).
- Make raw URL handling equivalent to `gui --protocol`, including single-instance forwarding; do not retain JSON-only behavior under the same invocation (`src/main.rs:417-432`; `src/ui.rs:3179-3204`).
- Scope `--install-progress-json` to install and structured validation flags to validate in the parser, closing the current runtime/ignored combinations (`src/main.rs:343-363,897-923,1072-1146`).
- Include `output_path` when inferring conversion format and make positional/named output equivalent (`src/main.rs:780-789`).
- Add `-g/-s` for game/source and other non-conflicting C# aliases already backed by identical fields; existing Rust names remain canonical and documented (`C# MBC:573-580,610-617`; `src/main.rs:298-303`).

Proof: help for every affected command; unknown singleton; bare NXM/ModSync cold and secondary launches; named versus positional conversion into distinct extensions; illegal flag combinations. Capture stdout, stderr and exit. No Rust test/harness is created or run.

### Task 2 — profile and settings

- Preserve Rust profile actions `capture` and `apply`, but match C# aliases and allow `--name` as the documented source fallback for clone/rename only if the C# direct probe confirms it (`C# MBC:704-724,3725-3734`; `src/profile_cli.rs:7-110`).
- Decide one output rule: default human text with `--json` matching C#, or keep JSON default with an explicit compatibility text mode. Default below preserves current Rust JSON and makes `--json` meaningful by selecting a versioned machine schema; it does not copy C# logger contamination (`src/profile_cli.rs:111-115`; `/tmp/modsync-cli-table-audit-20260927/runtime/*profile-list*`).
- Add settings list/get/set over the shared store. Values parse case-insensitive booleans/null, integer, float, object/array, otherwise string; empty set removes. List/get redact `nexusModsApiKey` unless explicit reveal is present (`C# MBC:726-746,3861-4043`; `src/ModSync.Core/Services/Settings/SettingsFileStore.cs:126-189`).
- Writes are atomic and preserve unrelated keys. No secret appears in logs, error context, process summaries or receipts (`AGENTS.md:13`; C# MBC:4070-4079).

Proof: isolated C#/Rust profile list/show/create/clone/rename/delete/activate and settings list/get/set corpora, with normalized timestamps/paths only where documented. Verify byte manifests, redaction and unrelated-key preservation. These are direct CLI probes, not Rust tests.

### Task 3 — convert and merge

- One `MergeRequest` handles `merge` and `convert --merge`. Every accepted flag either maps once or fails explicitly; no adapter silently drops it (`C# MBC:1905-1943`).
- Support C# named inputs/outputs and existing Rust positional forms. With an output path and no format, use its extension consistently for both forms; without output, default TOML (`C# MBC:388-395,488-498`; `src/main.rs:780-792`).
- Preserve stdin/input conflict, local archive generation and direction drafting. Keep Rust review-draft as an extension (`C# MBC:475-482`; `src/main.rs:158-177,750-779`).
- Add missing merge field controls only after tracing the shared model field they govern. `prefer-existing-modlinks` cannot be blindly aliased to `prefer-existing-resource-registry` until a worked fixture proves equivalent field ownership (`C# MBC:463-464,554-555`; `src/main.rs:134-135`).
- Conversion/download failures may not be logged and then returned as success. Aggregate failure becomes nonzero after any structured result is emitted (`C# MBC:2341-2370,2769-2793`).

Proof: each input/output grammar, every format, stdin, both merge entry points, each preference flag on a fixture where existing/incoming differ in exactly that field, and failure exits. Compare normalized guide trees and exact output file manifests; do not use Rust tests.

### Task 4 — validation

- `validate --output json` selects the single validation report envelope; `--output text` selects human pipeline output. Retain Event NDJSON only under the explicit Rust `--json` extension or rename it if collision review says ambiguity is unacceptable (`C# MBC:603-604,3005-3010`; `src/main.rs:340-357,1072-1085`).
- `--errors-only` filters presentation, not stages or exit calculation (`C# MBC:594-595,3149-3190`).
- `--ignore-errors` is rejected as unsupported while the binding fail-closed rule remains; it is never accepted and ignored (`C# MBC:597-598`; `AGENTS.md:11-13`).
- C# full/dry modes require the two environment roots. Rust phase/profile preparation may provide them, but the final normalized request must satisfy the same data prerequisites before invoking the engine (`C# MBC:3042-3078`; `src/main.rs:924-973`).
- Remaining-phase validation continues to use the shared verified completed-prefix path and cannot become a presentation-side reconstruction (`src/main.rs:1053-1060,1122-1145`; `src/engine.rs:864-896`).

Proof: early setup error, clean success, warning, stage failure, dry-run issue and remaining-prefix reports through both binaries. Compare JSON key/type/omission behavior, stdout/stderr and exit; compare game/mod before/after manifests for every case (`AGENTS.md:10`).

### Task 5 — safe install

- Default source directory to the input file's parent only for ordinary explicit-file invocation, matching C#. Profile-derived paths and explicit `--source-dir` retain precedence (`C# MBC:616-617,3342-3347`; `src/main.rs:948-973`).
- Add `-g/-s`, patcher spellings and compatible selection/profile aliases at normalization time (`C# MBC:610-677`; `src/main.rs:287-364`).
- Route `--download` into the shared acquisition plan/run/record contract when that implementation is accepted; do not recreate C# download orchestration inside `main.rs` (`docs/parity/download-orchestration-plan.md:191-245`; `src/main.rs:633-695,800-896`).
- `--no-checkpoint`, `--skip-validation`, `--continue-on-missing-sources`, `--continue-on-mod-failure`, `--best-effort`, and `--ignore-errors` parse only to produce a precise unsupported-safety error and exit nonzero before writes. `--yes` may be a harmless compatibility no-op only if there is provably no prompt; otherwise it remains rejected (`C# MBC:625-668`; `src/engine.rs:786-861`).
- Managed deployment flags must normalize into the existing shared deployment/profile architecture or remain explicit unsupported errors; a second install path is forbidden (`src/main.rs:25-26,237-286,433-480`; `COMPATIBILITY.md:655-663`).
- Preserve typed install-progress and legacy Event streams unchanged (`src/main.rs:1072-1121`; `docs/parity/GUI-CLI.md:38-52`).

Proof: disposable successful install, preflight failure, instruction failure, missing source, recovery/restore, selection/profile, patcher, direct-Markdown, progress streams and each rejected safety flag. Verify strict order, no backfill, exact bytes, source immutability, snapshot state, stdout/stderr and exit. No user game/mod path and no Rust test/harness.

### Task 6 — missing system verbs

- `set-nexus-api-key` validates with the shared Nexus client, then atomically updates only the intended setting. Errors redact the entire key (`C# MBC:680-688,4070-4129`; `src/main.rs:633-650,674-688`).
- `install-python-deps` is included only if the shipped Rust product still embeds a Python/HoloPatcher environment requiring it. If no production dependency exists, the verb returns a documented unsupported/migration diagnostic; fake success is forbidden (`C# MBC:690-695,4134-4325`).
- `holopatcher --args` requires a packaged executable with explicit provenance. Prefer structured trailing argv over C#'s one raw shell-like string unless an exact argument-boundary probe proves the string contract can be forwarded without shell interpretation (`C# MBC:697-702,4329-4401`).
- External program execution is never part of validation (`AGENTS.md:10`).

Proof: isolated settings, redaction, invalid/valid key with mocked or explicitly authorized network boundary, absent/present packaged patcher, argument-boundary receipt and child exit propagation. No credential appears in artifacts.

### Task 7 — GUI preload and protocol

- Accept C# compatibility forms `--kotorPath=`, `--modDirectory=`, `--instructionFile=`, `--nxm=`, `--modsync=` and `--allow-multiple-instances`, plus existing Rust `gui` options (`C# GUI args:48-115`; `src/main.rs:29-45`).
- Compatibility keys are case-insensitive. Duplicate scalar values fail as ambiguous rather than silently overwriting. Both `--key=value` and existing Rust separated forms normalize into one `GuiLaunchRequest`.
- Existing instruction-file preload should either auto-load like C# or be documented as an intentional interaction divergence; current Rust only seeds displayed strings during construction (`src/ui.rs:1617-1645,1719-1752`; `src/ModSync.GUI/MainWindow.axaml.cs:458-468`). Default below is to auto-load an existing explicit guide after state construction using the same production load path, never a preload-only parser.
- NXM and ModSync protocol values use the same downstream parser and single-instance queue as interactive handoff (`src/ui.rs:1788-1806,3179-3204`).
- Fix Windows registration to launch the GUI path, matching the already-correct Linux `gui --protocol %u` shape (`src/protocol_registration.rs:19-37,48-69`).

Proof: direct native cold launch and secondary-instance forwarding for every spelling, existing/missing preload paths, both schemes and multiple-instance mode. Record process tree, socket/forward result and screenshots only where presentation matters. This task waits for the serialized UI chain.

## Direct CLI/native probe matrix

Task 8 creates a durable evidence directory under `docs/parity/cli-table-native/` only after implementation. Each case records command argv with secrets redacted, binary/source hashes, stdout, stderr, exit, fixture input hashes and before/after manifests. The probe driver is shell orchestration or direct commands; it is not a Rust test or harness.

| Gate | Minimum cases | Pass condition | Source contract |
|---|---|---|---|
| Parser | root/per-verb help, version, missing required, unknown verb, duplicate/conflict, every alias | exact accepted grammar; deliberate help/version divergence documented | C# MBC:364-778; `src/main.rs:12-364` |
| Convert | positional/named/stdin; all formats; output inference; draft/local generation; failure | normalized output equality or row-specific documented divergence; failure nonzero | C# MBC:1815-2370; `src/main.rs:738-794` |
| Merge | both entry points and every field preference | same normalized result and no dropped flag | C# MBC:1905-1943,2384-2793; `src/main.rs:696-737` |
| Validate | setup error, clean, warning, error, dry-run, full, remaining | JSON key/type/exit contract; zero game/mod mutations | C# MBC:3014-3275; `src/main.rs:1122-1159`; `AGENTS.md:10` |
| Install | success, preflight failure, missing input, instruction failure, restore, profile/selection, direct Markdown | strict order, exact bytes, fail-stop, snapshot/receipt truth, stream/exit contract | C# MBC:3279-3696; `src/engine.rs:786-861` |
| Profile/settings | every action, default/explicit store, redaction, malformed input, unrelated keys | expected normalized JSON/text; atomic/preserved state; no secret output | C# MBC:3700-4129; `src/profile_cli.rs:42-115` |
| GUI preload | every old/new spelling, cold/secondary/multiple, NXM/ModSync | same launch/forward state and correct loaded paths/guide | C# GUI args:48-115; `src/ui.rs:3179-3233` |
| Rust extensions | every retained verb's help plus one no-op/read-only or isolated happy path | no regression to existing documented schema/exit | `src/main.rs:21-286,401-896`; `COMPATIBILITY.md:529-579,621-655` |
| No-default-features product | help and GUI failure only | parser works; GUI returns documented unsupported error | `src/main.rs:1168-1200`; `Cargo.toml:9-11` |

The current audit probe root `/tmp/modsync-cli-table-audit-20260927/` is discovery evidence only. It is not durable acceptance evidence and does not bind the Rust binary to the audited commit (`docs/parity/cli-table-audit.md`, “Probe receipt”).

## Review gates

### Gate A — table/source review

An independent reviewer enumerates C# attributes and Rust Clap declarations without reading the implementation owner's summary first, then checks every audit row. Any claim without a file:line or command output is `unverified`. Criterion: zero unclassified verbs/flags and zero false matched rows (`C# MBC:364-746`; `src/main.rs:12-364`).

### Gate B — API and safety review

Review normalized requests against shared library ownership. Criterion: no execution logic duplicated in adapters; no accepted safety flag weakens mandatory validation, snapshot, guide order or stop-on-failure (`AGENTS.md:7-14`; `src/engine.rs:786-861`). Security review separately searches all output/receipts for supplied secret sentinels.

### Gate C — direct differential review

A reviewer runs the durable receipt verifier, spot-runs at least one success and one early failure per major verb family, and independently checks before/after manifests. Criterion: every required table row has a live result or explicit approved divergence; source presence and help text are insufficient (`docs/parity/GUI-CLI.md:15-17`).

### Gate D — integration and ledger review

Confirm the exact installed production binary hashes, launcher/protocol registrations, and ledger wording. Criterion: `COMPATIBILITY.md`, `GUI-CLI.md`, `ACCEPTANCE.md`, and the audit agree; F26 is not marked complete while any required row is missing/unresolved (`docs/parity/ACCEPTANCE.md:61`; `AGENTS.md:14`).

Every reviewer reports in claim blocks:

```text
claim: <verbatim>
referent: <command/output or file:line>
checked: <exact check>
verdict: holds | does not hold | unresolved
confidence: <low | medium | high> and what would change it
```

## Open questions with defaults

1. **Must erroneous C# help/version exit 1 and logger-prefixed profile JSON be byte-compatible?** Default: **no**. Preserve Rust help/version exit 0 and clean JSON; document the intentional quality correction. Evidence that would change it: a deployed external consumer that requires those erroneous behaviors. Observed behavior: `/tmp/modsync-cli-table-audit-20260927/{csharp,rust}/root.*`, `runtime/*version*`, and `runtime/*profile-list*`.
2. **What happens when users pass C# unsafe install flags?** Default: accept their spelling only to fail early with a precise safety diagnostic and exit 1. Do not ignore them and do not weaken recovery/order. Evidence that would change it: an explicit user ruling overriding `AGENTS.md:11-13` and `docs/parity/ACCEPTANCE.md:75`.
3. **Should `--output json` replace Rust `validate --json` Event NDJSON?** Default: add C# `--output json` for the single pipeline report and retain `--json` as the documented Rust legacy stream until a versioned migration exists (`src/main.rs:340-357,1072-1085`; `docs/parity/GUI-CLI.md:38-52`).
4. **Should profile default output become C# human text?** Default: preserve Rust JSON default for compatibility, add an explicit human mode, and make `--json` select a documented clean schema. Evidence that would change it: a public contract declaring C# default text mandatory and no Rust JSON consumers (`src/profile_cli.rs:111-115`; C# MBC:3817-3915).
5. **Is `install-python-deps` still a real product feature in the Rust package?** Default: expose a nonzero unsupported/migration result until packaged runtime evidence proves an embedded Python dependency exists; never claim success without mutation proof (C# MBC:4134-4325; `src/main.rs:21-236`).
6. **Does `prefer-existing-modlinks` equal Rust resource-registry preference?** Default: no alias until a one-field merge fixture proves identical resulting ownership. Current names cover different apparent scopes (C# MBC:463-464,554-555; `src/main.rs:134-135`).
7. **Should C# preload automatically load the guide in Rust?** Default: yes, after the active UI ownership chain releases `src/ui.rs`, using the same production `load` path and preserving explicit-path precedence (`src/ui.rs:1617-1752`; `src/ModSync.GUI/MainWindow.axaml.cs:458-468`).
8. **Can F26 close without successful install/download/system-verb probes?** Default: no. Help and early failures prove parsing only; F26 requires invocation, JSON, exit, preload and wrapper behavior (`docs/parity/ACCEPTANCE.md:61`).

## Completion claim blocks

claim: This task sequence can implement F26 without racing the active UI or engine lanes.
referent: The ownership table serializes `src/main.rs`, defers `src/ui.rs` until install-progress/completion-pages/mod-selection finish, and requires explicit `src/engine.rs` handoff.
checked: File ownership compared with current task constraints and `src/main.rs:897-1163` / `src/engine.rs:786-861` boundaries.
verdict: holds
confidence: high; a new overlapping owner must update the sequence before dispatch.

claim: Parser aliases alone are sufficient to close F26.
referent: Missing settings/system verbs, differing JSON/exit contracts, GUI preload behavior and wrapper defects in `cli-table-audit.md`.
checked: Full verb/flag/source table and bounded binary probes.
verdict: does not hold
confidence: high; only removing those rows from the F26 definition would change it.

claim: Direct production probes can replace deferred Rust tests for this phase without inflating proof.
referent: Task 8 probe matrix captures argv, hashes, streams, exits and filesystem manifests while `docs/parity/ACCEPTANCE.md:26` forbids Rust test/harness work.
checked: Each contract row has an invocation-level or filesystem oracle; internal-only branch coverage is not claimed.
verdict: holds
confidence: medium; it proves public behavior, not exhaustive internal paths, and full test-target compilation remains explicitly unproven.

claim: F26 may be marked complete after Task 9.
referent: Gate D requires every required audit row to have accepted live evidence or an explicit approved divergence and requires all ledgers to agree.
checked: Not checkable until Tasks 1-8 land.
verdict: unresolved
confidence: low today; durable receipts and final independent review would raise it.
