# Full Rust functionality progress

Updated 2026-10-02. Goal: every F01–F29 requirement in [ACCEPTANCE.md](ACCEPTANCE.md)
verified against pinned C# `d31ccbd3`, including identical K1/K2 full installs
against C# and manual references. A delivered slice does not close a whole domain.

**Overall: incomplete.** F01–F28 are partial; F29 is unverified. No percentage
is assigned because the remaining domains differ greatly in scope and proof cost.

## Current working source — 2026-10-02

A missing patcher in the validation dry run (2026-10-04, F05/F08/F09): when a
component's own `Patcher` step names a patcher folder or executable that is not in the
mod folder, `validate --dry-run` and the desktop Validate step now report the
component as `DryRunSkipped` (a warning) and pass, as C# does: its Patcher branch
returns `FileNotFoundPre` (`ModComponent.cs:1630-1641`), mapped to
`MissingSourceFiles` (`:1862-1866`) and reported as a skip
(`DryRunValidator.cs:515-527`). Rust failed the report with `Missing external
program/mod`. Install still refuses; a wildcard or empty Patcher Source stays refused
and a folder Source stays bound (documented). Proof: 58 match / 11 known against the
live C# CLI; 32 fail on `0f71baa`. The regression probes stay at baseline. See
[patcher-missing-source-contract.md](patcher-missing-source-contract.md).

A failing chosen option in the validation dry run (2026-10-04, F04/F05/F08): when an
option that a `Choose` step runs is missing a source, `validate --dry-run` and the
desktop Validate step now fail that component (`ERROR: [EngineSimulation] <component>
/ step <n> (Choose): ...`, exit 1), as C# does: it maps any option failure to
`OptionalInstallFailed` and `UserCancelledInstall` (`ModComponent.cs:1648-1667`,
`:1857-1860`), which its dry run reports as an error (`DryRunValidator.cs:535-546`).
Rust reported the component as `DryRunSkipped` and passed, although its install refused
the selection. A miss in the component's own steps stays `DryRunSkipped`. Proof: 67
match / 1 known against the live C# CLI; 37 fail on `b89484d`. The regression probes
stay at baseline. See [option-failure-contract.md](option-failure-contract.md).

Install order and reference checks (2026-10-04, F04): `install` now runs the selected
components in the C# CLI's order, a breadth-first sort of every component by
Dependencies, InstallAfter and InstallBefore from the resolver's order
(`InstallCoordinator.cs:97-186`); Rust took the first ready selected component in file
order (`A`, `B` after `A`, `C`: C# and now Rust `A, C, B`, Rust before `A, B, C`).
Widescreen-only components take their breadth-first place, Markdown guides order by
Dependencies only (C# drops InstallBefore/InstallAfter there) and `--direct-markdown`
keeps source order, as in C#. Validate, install and `conflicts` now refuse a missing,
self or circular InstallBefore/InstallAfter reference on any component of a
non-Markdown guide before the report, and list components in the resolver's order, as
C# does (`ModBuildConverter.cs:3092`, `:3418`). New: `--phase base`/`remaining` keep
widescreen-only components after the base build, and `--phase base` refuses before
any write when a base component must follow one. The dry run follows the install
order and `--ignore-errors` stays refused (documented). Proof: 161 match / 1 known
against the live C# CLI; 85 fail on `e4c0b4d`, 23 on the first version `174482a`. The
regression probes stay at baseline. See
[install-order-contract.md](install-order-contract.md).

Component order in `convert` (2026-10-04, F01/F04): `convert` now writes components in
the C# dependency resolver's order (`DependencyResolverService.cs:48-403`, called at
`ModBuildConverter.cs:2046`) and refuses missing, self and circular
InstallBefore/InstallAfter references unless `--ignore-errors`, which it now accepts;
Rust wrote the file order and refused nothing. Markdown input keeps file order and is
never refused, as in C# (its Markdown reader drops those fields). With
`--ignore-errors` a cycle stays refused (C# writes an empty guide). Install order was
unchanged (since compared, above). Proof: 126 match / 14 known against the live C# CLI; 57 fail on `24897e6`. The regression probes stay at
baseline. See [convert-dependency-order-contract.md](convert-dependency-order-contract.md).

File selection when the file selects nothing (2026-10-04, F04/F08): `validate` and
`install` with `--use-file-selection` now select every component of a guide that
selects none, as the C# file loader does (`ModComponentSerializationService.cs:895-903`);
Rust refused (`No components are marked IsSelected in the instruction file.`, or the
install FOMOD gate). Markdown keeps its flags in both; profile runs are unchanged.
Proof: 58 match against the live C# CLI; 30 fail on `a0df4b7`. The regression probes
stay at baseline. See
[file-selection-fallback-contract.md](file-selection-fallback-contract.md).

Missing destination folders in the validation report (2026-10-04, F08): the report's
archive stage (`validate --output json`, the desktop Validate step) now reports each
Copy, Move, CleanList or DelDuplicate step whose Destination folder does not exist as
C# does: `Destination cannot be found! Got '<full path>'`, one error per step, counted
in `errorCount`. Rust stopped at the first such step with `archive/destination
validation: Destination cannot be found: <Destination>`. C# creates the missing folder
while validating, so it reports a folder once; Rust never creates folders and reports
every step (documented). Proof: 75 match and 12 known differences against the live C#
CLI; 32 fail on `1e2d133`. The regression probes stay at baseline. See
[missing-destination-contract.md](missing-destination-contract.md).

Missing archives in the validation report (2026-10-04, F08): the report's archive
stage (`validate --output json`, the desktop Validate step) now reports a missing
download as C# does: one `Missing Required Archives: [<Sources>]` error per Extract
step, counted in `errorCount`, with the component's other checks skipped. Rust named
only the first missing archive per component, as `archive/destination validation:
Source matches no files: ...`, and counted one error. Rust still refuses a partly
missing component or an option-only missing archive, where C# passes the stage
(fail-closed, documented). Proof: 47 match and 22 known differences against the live
C# CLI; 27 fail on `3024e4d`. The regression probes stay at baseline. See
[missing-archives-contract.md](missing-archives-contract.md).

Patcher INI names with brackets (2026-10-04, F09/F19): the virtual Patcher simulation
now takes `[` and `]` in a patcher's own INI names literally (InstallList folders and
`File*`/`Replace*` names, namespaces.ini `IniName`/`DataPath`), as C#
`TslPatcherVfsSimulator` does with `Path.Combine`. Rust refused them as wildcards
(`Wildcards are not allowed here: [HD]tex.tga`), so validate, the dry-run report,
`conflicts` and install preflight failed a guide whose mod ships such names; C#
validates it and reports its conflicts. `*` and `?` stay refused (fail-closed, as for
archive members). Proof: 39 match and 6 known differences against the live reference
pipeline and `FileConflictAnalyzer`; 33 fail on `c2225e3`. The regression probes stay
at baseline. See [patcher-ini-brackets-contract.md](patcher-ini-brackets-contract.md).

Execute and Run Arguments (2026-10-04, F05/F09): an `Execute` or `Run` step's
`Arguments` text is now split with the Windows command-line rules on every host, as
.NET splits C# `ProcessStartInfo.Arguments` on Linux too. Rust used POSIX shell rules
on Linux and macOS, so the program received other arguments: backslashes vanished
(`<<kotorDirectory>>\Override` became `<game>Override`), `'a b'` grouped, `#` began a
comment and a newline separated. Validation could not show it. Unclosed quotes and
unknown placeholders stay refused, and a root with spaces stays one argument
(documented). Proof: 89 match and 21 known differences against a live reference
oracle; 21 fail on `0cc50a7`. The regression probes stay at baseline. See
[execute-arguments-contract.md](execute-arguments-contract.md).

Archive members with brackets (2026-10-04, F06/F05): an archive whose member names
contain `[` or `]` (`Override/x[1].tga`, `[HD] textures/`) now extracts and installs as
in C#, whose `TryGetZipSafeArchiveEntryExtractPath` keeps those characters. Rust
admitted member names with the literal guide-path rule, so one bracketed member failed
the archive: the FOMOD gate refused plain `validate` and `install`, the Validate
step's archive stage failed, and download verification would also refuse it (not probed).
Checkpoint capture, managed process capture and FOMOD mapping (archive name, wrapper
folder, ModuleConfig `source`) now also take bracketed names literally (review
follow-up). `*` and `?` in a member name stay refused (no Windows file can have them;
documented). Proof: 83 match and 8 known differences against a live reference oracle
and mapper; 9 fail on `4a09589`, 45 of the first 68 on `31e98ec`. The regression
probes stay at baseline. See
[archive-member-brackets-contract.md](archive-member-brackets-contract.md).

Install root as Destination (2026-10-04, F05/F07): a Copy, Move, CleanList or
DelDuplicate whose Destination is `<<kotorDirectory>>`, `<<gameDirectory>>` or
`<<modDirectory>>` (with or without a trailing separator) now passes the
archive/destination validation stage, and DelDuplicate runs in that root, as in C#.
Rust resolved a bare root with a trailing separator and missed the root directory, so
the Validate step failed every such guide and a root DelDuplicate failed install. A
bare-root Source still matches nothing (a review follow-up: the first fix let
`Extract [<<modDirectory>>]` extract every archive under the root). A `.` Destination
part fails that stage, and DelDuplicate/CleanList into it fail at execution, as in C#.
Rename to a bare placeholder stays a fail-closed known difference. Proof: 119 match and
4 known differences against a live reference oracle; 17 fail on `6e67b3c`, 45 on
`8c79761`. The regression
probes stay at baseline. See [root-destination-contract.md](root-destination-contract.md).

CleanList rows and listed names (2026-10-04, F07-D): a CleanList row that lists a
name no file can have (a quoted field, `*`, `?`, `<`, `>`, `|`, `:`) now skips that
name as C# `File.Exists` does; Rust failed the whole step, so validate and install
refused a list such as `"Mod, by Author",a.tga`. `[` and `]` are literal. `..`,
absolute, drive-qualified, trailing-dot and device names stay refused (fail-closed,
documented). Row grammar, matching, comments and blank rows already matched. Proof:
121 match and 20 known differences against a live reference oracle; 30 fail on
`f9eab15`. The regression probes stay at baseline. See
[cleanlist-rows-contract.md](cleanlist-rows-contract.md).

CleanList list-file choice (2026-10-04, F07-D): a CleanList whose Sources name
more than one existing list now reads the first authored Source's list, as C#
`ExecuteCleanListAsync` reads `RealSourcePaths[0]`. Rust read the match with the
smallest sorted path, so `CleanList [l/b.csv, l/a.csv]` applied `a.csv` and deleted
the wrong files. Every Source must still match (fail-closed, unchanged); within one
wildcard Source with several matches Rust keeps sorted order, where C# order is
unspecified (documented). Proof: 39 match and 3 known differences against a live
reference oracle; 9 fail on `0997b0f`. The regression probes stay at baseline. See
[cleanlist-list-choice-contract.md](cleanlist-list-choice-contract.md).

Directory Sources and wildcard rules (2026-10-04, follow-ups to the Delete slice): a
`Delete`, `Copy`, `Move`, `Rename`, `CleanList`, `Execute` or `Run` Source that names a
directory now reaches no file beneath it, as in C# (`EnumerateFilesWithWildcards`
returns files only). Delete deletes nothing (strict Delete fails); the others fail.
Before, Rust deleted, moved, copied or renamed the whole subtree, read a CleanList from
the folder, or (with `--allow-processes`) ran every program in it. Every Source's
wildcards now follow C# `WildcardPathMatch`: `**` stays within one level and `[`/`]`
are literal. Missing Rename, CleanList and Execute/Run Sources are classified as in C#
in the dry-run report (one shared literal-versus-wildcard test; the actions differ on
an all-wildcard empty expansion). Extract, Patcher and the FOMOD gate keep the folder rule, so
Rust still extracts a folder of archives that C# refuses (documented). Proof: 169 match
and 11 known differences against a live reference oracle; 4 fail on `ab6b389`, 13 on
`df67229`. The S2b proof now has 265 checks (`[e]xample.zip` is a
missing source; a literally so named archive installs); the other regression probes
stay at baseline. See [delete-directory-contract.md](delete-directory-contract.md).

Copy/Move onto itself, follow-up (2026-10-04): a self-target next to a source that
miss-only recovery binds from a `ResourceRegistry` archive was still refused by
recovery qualification; with `Overwrite = false` it is now skipped with a warning,
as on the direct path and in C#. Install now also emits one ordinary event,
`WARNING: Destination file already exists and overwrite is false: <path>`, for each
file a Copy/Move skips with `Overwrite = false`, as C# logs one warning per skipped
file; exit codes and results are unchanged. Proof: 51 match and 4 known
differences against a live reference oracle; 17 fail on `6b8baff`. See
[copy-move-self-contract.md](copy-move-self-contract.md).

Copy/Move onto itself (2026-10-04): a Copy or Move with `Overwrite = false` whose
target is the source file itself (a wildcard back into its own folder, or a Source
list that names a file already in the destination) now warns, skips that file and
goes on, as C# does. Rust refused it with `Source equals destination`. With
`Overwrite = true` or omitted Rust still refuses before any write; C# fails after
deleting the file. Proof: 30 match and 2 known symlink differences against a live
reference oracle; 20 fail on `354055b`. See
[copy-move-self-contract.md](copy-move-self-contract.md).

Archive-generation check (2026-10-04): `tools/check_archive_generation.py` runs
without wrappers again (`-f json`, `DOTNET_ROLL_FORWARD`, reference mounted into its
sandbox). It requires a 7-Zip CLI that lists ZIP, which the reference falls back to,
and records its environment in the receipt. It gives 27 of 28 CLI cases matching
plus the annotated `.exe` difference. Its second, raw-Source capture reproduces
`tests/fixtures/csharp/archive-generation.json` byte for byte. The CLI capture now
keeps C#'s loaded Sources, which `62b6f70` (intended) made Rust match; that explains
four of the six earlier differences. The other two came from a missing 7-Zip CLI.
No Rust source change; see COMPATIBILITY.

Ordinal path identity (2026-10-03): engine VFS, managed process capture,
deployment and conflict path keys share the pinned .NET `OrdinalIgnoreCase`
key, and conflict rows sort in reference UTF-16 ordinal order. Checkpoint tree
alias checks use the same key. Production gates only; see COMPATIBILITY.

Guide `lastModified` (2026-10-03): TOML and YAML saves now normalize the
value as the reference does. Values the reference cannot parse are omitted,
and unmodeled forms keep the authored text with a warning. The C#/Rust convert
differential gives 217 matches and 18 documented retentions; see
[lastmodified-contract.md](lastmodified-contract.md). Review follow-up: the
`LastModified` key choice now follows each loader, retained native TOML values
no longer leak a table into YAML, key order is kept and warnings are deduped.
Live oracle regenerated: 249 match, 18 retained, 2 reference load errors,
0 failures. In-repo tests deferred under the AGENTS.md freeze.

FOMOD residuals (2026-10-03c): the gate scope now includes hard dependencies like C#
`ExpandWithHardDependencies`, a missing registered archive without prompt state gets
C#'s per-component soft warning, and run-choice ownership, archive-suffixed folder
sources and per-archive preservation are proved at runtime; see
[fomod-gate-scope.md](fomod-gate-scope.md) (74/74; 12 fail on `15b11cb`).

FOMOD archive-scoped warning (2026-10-03d): C#'s soft gate warning "marked configured but
has no matching archive-scoped instructions" now has a Rust form. It fires once per
registering component when a `configured` FOMOD archive's installer maps no files for any
choice, the same partition as a live reference oracle. The proof is 37/37 (7 fail on
`223ed92`), and the regression probes still pass. See
[fomod-archive-scoped.md](fomod-archive-scoped.md).

FOMOD gate warnings after a failure (2026-10-03e): soft gate warnings (missing archive,
"configured but has no matching archive-scoped instructions", no route) collected before a
later hard failure of the gate (an unselected dependency, a restriction conflict or an
in-gate issue) are no longer dropped. They stay in the failed report stage before the error,
as in C#, and are emitted as `WARNING:` events on plain validate and install. Errors and
exits are unchanged. The proof is 55/55 (17 fail on `438bb73`), and the regression probes
still pass. See [fomod-gate-warnings.md](fomod-gate-warnings.md).

FOMOD gate collects every issue (2026-10-04): the gate no longer stops at its first
in-gate FOMOD issue. Like C# `FomodConfigurationGate`, it collects every issue and every
warning. Issues are a missing archive with prompt state, an unreadable archive, and
missing, unparseable or stale saved choices. Cancellation, unsafe paths and guide errors
still abort. The report stage lists all warnings, then every issue, with `errorCount`
counting each issue. Plain validate and install keep a single issue's text unchanged and
number several issues in one line: `<n> issues: [1] ...; [2] ...`. The proof is 67/67
(22 fail on `d78aea2`), and the regression probes still pass. See
[fomod-gate-collect.md](fomod-gate-collect.md).

Empty Source list (2026-10-04): Delete, Copy, Move and Extract with no usable Source
failed every verb with the untyped `Instruction has no Source`. As C# does, a
non-strict Delete is now a no-op. A strict Delete, Copy, Move or Extract raises the
typed missing-source cause: `DryRunSkipped` in the dry-run report, rejected by plain
validate and install. The proof is 51/51 against a live reference oracle (26 fail on
`fba4eb1`), and the regression probes still pass. See
[empty-source-contract.md](empty-source-contract.md).

Scalar empty Source (2026-10-04): XML `<Source></Source>` (or whitespace) and Markdown
metadata `Source = ""` / `Source: ''` were read as an empty list, so a non-strict Delete
installed as a no-op and a Copy passed the dry-run report. They are now kept as `[""]`
and refused by every verb, as TOML, JSON and YAML already were; C# refuses the XML guide
at load and drops the Markdown metadata block (documented difference). `<Source />`
stays an empty list and a Choose stays an empty no-op, as in C#. Saving writes the
blank scalar back, so `convert` to any format keeps the refusal. The proof is 99/99
against a live reference oracle (30 fail on `3911e74`), and the regression probes
still pass. See
[scalar-empty-source-contract.md](scalar-empty-source-contract.md).

Choose with a scalar Source (2026-10-04): the shared loader now decides Choose on the
raw Action text and splits a scalar Choose Source on `;` and `,`, as C# does. A blank
(`""`, `"  "`, `" , "`) Choose Source is an empty no-op Choose in every format; before,
standalone TOML, JSON and YAML (and `" , "` anywhere) refused it. A Markdown
`Action = 9` with a blank Source is now refused, as in TOML, JSON and XML. The XML and
Markdown Choose special cases are removed. The scalar-empty-source proof is now
190/190 (235/235 with live reference loads; 117/73 on `1b3faeb`), and the regression
probes still pass. See
[scalar-empty-source-contract.md](scalar-empty-source-contract.md).

Choose option matching (2026-10-04): Rust matched Choose entries as parsed GUID
values, so a braced, parenthesized, dash-free or padded entry installed the option.
C# `GetChosenOptions` matches only the hyphenated `Guid.ToString()` text, ignoring
case, and skips such an entry. Rust now uses the C# text rule, and an entry that names
no option is still refused (deliberate fail-closed; C# ignores it). The archive
generators now write `Option.Guid.ToString()` text into a Choose, as C# does, and the
editor's reference check uses the same rule. Before, a braced authored option ID made
`--auto-generate-local` write a Choose entry the engine refused. Legacy Markdown
already split a Choose Source on `,` only, as C# does, so it needs no change. The proof
is 69/69 (92/92 with live reference runs; 48/21 on `a09ca4a`, 63/6 on `5eb80d5`), and
the regression probes still pass. See [choose-option-match-contract.md](choose-option-match-contract.md).

Delete of a missing file with Overwrite=true (2026-10-04): Rust ignored `Overwrite`
on Delete. As C# `DeleteFile` does (`FileNotFoundPost`, then `MissingSourceFiles`), a
strict Delete now deletes the present matches and fails on a missing literal or an
empty match, with the typed missing-source cause. The dry-run report gives the
reference `DryRunSkipped` warning; plain validate and install reject. The proof is
39/39 against a live reference oracle (15 fail on `8fad43c`), and the regression
probes still pass. See [delete-strict-contract.md](delete-strict-contract.md).

Extract onto existing files with Overwrite=false (2026-10-04): Rust kept an existing
file when an `Extract` step with `Overwrite = false` reached it, so stale output, a
game file or the first of two archives survived. C# extraction never reads
`Overwrite` and writes every entry with `FileMode.Create`. Rust now always replaces,
and the last archive wins; configured FOMOD extraction keeps its fail-closed refusal.
The proof is 20/20 against a live reference oracle (5 fail on `5905259`), and the
regression probes still pass. See [extract-overwrite-contract.md](extract-overwrite-contract.md).

Rename onto an existing file (2026-10-04): a `Rename` with `Overwrite=false` whose
target exists no longer passes silently. As C# `RenameFile` does
(`RenameTargetAlreadyExists`), the engine renames the other sources and then fails
the instruction, so validate fails its dry run and install is blocked before any
write. Copy and Move keep the reference warn-and-skip. The proof is 18/18 against a
live reference oracle (8 fail on `5643276`), and the regression probes still pass.
See [rename-overwrite-contract.md](rename-overwrite-contract.md).

Runtime revalidation of `25296da` (2026-10-03b): all retained CLI suites and the C#/Rust differentials pass, native GUI probes recalibrated, one Windows Clippy fix; see [runtime-revalidation-2026-10-03b.md](runtime-revalidation-2026-10-03b.md).

Step-2 broken-choices follow-up (2026-10-03b): unparseable saved `FomodChoices` on a
prompt-eligible Fetch consumer again raise the blocking "FOMOD choices need attention"
Retry/Dismiss prompt and keep Validate/Install blocked (plan G7/G8, step-2 review P1),
instead of only an Activity line. The step-2 probe now passes 48/48 and step 1 46/46;
see the resolved item in [runtime-revalidation-2026-10-03b.md](runtime-revalidation-2026-10-03b.md).

Latest source wave (after `f528e4b`): Extract, Copy, Move and Patcher recover a
missing source only after a direct miss, through registry auto-extraction,
extracted descendants, Move variants and redundant game Moves, all staged before
publication; direct hits keep their previous behavior. Execution, validation and
conflicts compare component GUIDs by value without rewriting stored IDs. JSON
numbers keep their source lexeme through a vendored `serde_json` patch, and
TOML/YAML saves render them natively with shared size bounds. Nexus mod-info
Name/Version accept numbers and booleans as the reference does, and install,
validate and conflicts refuse the reference skip/continue/auto-confirm flags by
name. Independent SPEC and QUALITY reviews accepted each scope. See the
source-resolution, GUID, JSON number-origin, vendor provenance and update-check
contracts and the production-gate receipt. No tests, runtime or CI ran;
behavioral acceptance stays open.

Latest source continuation adds worker-owned Save Selection with bounded remaining
FOMOD overlay and stale-result guards, raw UUID generation/copy, drop-dialog
cancellation/focus, explicit Help issue/sponsor links, and ordinary validation's
empty-selection setup-error boundary. See the GUI-tools desktop receipt and
production-gate log for implementation/review versus compilation evidence.
Locale-aware settings parsing and editor section controls/local-archive
generation preview are also implemented; see the numeric and GUI-tools receipts.
The install source fallback and -g/-s aliases are corrected. The combined source
passes both Linux production feature configurations, strict Clippy, formatting and
the Windows GNU CLI build/link boundary recorded in the latest production receipt.
The subsequent rootless basic-validation adapter now produces a bounded read-only
component report and failed FOMOD stage, with independent review corrections for
authored selection, JSON setup errors and explicit archive directory membership.
Convert/merge TOML defaults, private legacy Nexus-key import shared by CLI/desktop,
and Microsoft-date grammar are also implemented and source reviewed. Production
checks for this combined boundary passed in both Linux feature modes; older
Windows gates do not cover it.
The source also implements explicit Linux maintenance through shared CLI/desktop
preview, apply, status and reverse recovery, with owned workers and manual sharing
resume. Independent review corrected interruption, persistence, path and work
bounds. Root-aware FOMOD choice attachment now handles real archive-shaped
directories without dropping ordinary folder or future VFS archive ownership.
Both slices are reviewed and passed the same production gates. Native platform,
cross-process acquisition fencing, dependency/corpus and behavioral acceptance
remain open. See the maintenance and rooted-choice contracts. Acceptance rows
remain open.

The subsequent source slice adds a durable mod-root maintenance fence and
target-dependent terminal cleanup, shared acquisition/root locks, current-guide
revalidation of existing-file skips, standalone output guards, bounded worker and
retry settings, cancellable keyed execution grouping and direct torrent metadata
cancellation. Independent review corrected stale claims, matching work bounds,
GUID identity, session cleanup and the game-only terminal cleanup window.
The Darwin maintenance source profile now uses pinned native ACL/flag checks and
exclusive rename, with stable capability admission preserving old Linux records.
Native execution is unverified. An offline OdyPatcher producer source now binds
the exact root package, source/license and emitted executable to a retained
receipt; it has not been invoked and the backend allowlist remains empty.
See the acquisition, maintenance and package contracts and current production
gate receipt. Windows maintenance, Unicode mapping, broader date normalization,
actual backend provenance and all domain/native acceptance remain open.

The next source continuation records the exact Unicode 15.0 maintenance mapper
and preserves legacy ASCII intent admission. Shared FOMOD/selection/authoring
name keys use a pinned simple-uppercase table with the .NET 9 ICU ordinal
exceptions. Planning, discovery and execution now share keyed cancellation/work
bounds; stale peer cleanup preserves nonempty contents and rejects Windows
reparse points. These corrections have independent source review.

Windows maintenance now has a local fixed NTFS source profile with full native
identity, unchanged owner/group/DACL proof, default-stream-only admission,
readonly repair, exclusive handle-relative rename and protected metadata at
creation. Independent review corrected named-stream proof, interrupted journal
truncation rights and checked EOF append semantics. Native acceptance remains
open. Profile date source now canonicalizes UTC/Unspecified values and includes
a bounded immutable Unix local-zone provider; unsupported local rules retain
authored values with warnings. Exact remaining provider/fallback forms are
listed in the date contract. Both Linux production build/lint configurations
and Windows GNU library/CLI cross-compilation and linking passed for this
reviewed boundary; see the production gate receipt. Native behavior remains
unverified.

The subsequent source continuation adds shared DateTime calendar mechanics,
Unix neighboring-year marker seasons, historical/future seam selection, daylight
day transfer and endpoint lookup. Windows now has bounded immutable native/
registry capture and a pure ordinary adjustment-rule engine. Windows year markers,
shifted cross-year windows, extreme delta rebasing, adjusted endpoints and native
fallback discovery remain explicit follow-ups. Unsupported conversions retain
authored text with bounded warnings. Both local providers received independent
review corrections for negative-daylight local ambiguity; their distinct UTC
intervals are unchanged.

Telemetry source now shares canonical OTLP endpoint admission across readiness
and transport, fixing migrated C# full-signal URLs without doubling paths.
The desktop has an owned first-run consent prompt that persists both choices
through matching save/lifecycle receipts, checks readback and retains retries
on failure. Logging errors are separate from configuration errors. Independent
source review accepted this slice; both Linux build/strict-lint modes and
Windows GNU production compilation/linking passed in the current gate receipt.
No native/transport acceptance is inferred from source review.

The next reviewed source wave shares compiled pinned Unicode casing across
maintenance, FOMOD and merge, preserving recorded table bytes and descriptors.
Merge name/author/instruction/option keys use simple invariant lowercase;
destination/argument comparisons and Language union use ordinal casing.
Profile fallback now admits bounded complete ISO, slash and English month-name
forms, with binary64 nearest-even fractions and distinct typed fallback-local
range handling. Windows rules now cover year markers, neighboring seasons,
cross-year fold retries, single-day rebasing and endpoint lookup. Native fallback
discovery and broader invariant grammar remain named implementation follow-ups.
Independent source review accepted this wave. Production evidence is recorded
in the gate log; full domains and runtime acceptance remain open.

The next source increment pins merge punctuation to the reference UTF-16 char
operation and Unicode 15 category dataset, preserving supplementary punctuation.
A narrow HTTP(S) loopback alias correction retains the separate full URI/IDN
boundary. Profile fallback adds bounded weekday/AMPM/delimiter syntax; Unix
future rules now validate effective daylight totals without independently
rejecting the rebased standard component. Windows native discovery captures
failed direct lookup, first-match registry search and qualified native/UTC
fallback decisions twice. Independent source reviews, both Linux production
build/strict-lint modes and Windows GNU compile/link passed for this wave.
Native and corpus acceptance remain deferred; no full domain is closed.

Continuation uses the actual `ModSync.rs` checkout through `eed8089` and its
working changes. The [remaining-work tracker](../plans/2026-09-28-1130-remaining-work-tracker-plan.md)
is the current source reconciliation. The sections below retain historical
commit-bound evidence and must not be treated as a current missing-source list.

Recent reviewed source includes explicit draft resolution, filename-based
inference, shared conversion/merge/download composition, completed-prefix
protection, active-profile execution binding, theme/game presentation context,
visible bounded FOMOD warnings, Unicode selection and bounded guide ingestion.
Owned standalone backend execution is implemented for Linux/macOS/Windows;
engine and desktop integration are implemented and source reviewed. Nexus
acquisition uses bounded cancellable readers, race-token-aware client waits and
URL-safe diagnostics; desktop patcher settings use matching engine/path receipts.
Native assets and packaging
production sources are prepared with provenance; no package was produced here.

Subsequent reviewed corrections invalidate readiness across resource-observer
pauses/rebaselines and reject failed or mismatched packaging receipts. The
desktop NXM preference now has a reviewed owned worker and settings consumer,
including guarded replacement confirmation and independent preference disable.
Further source corrections add component-removal confirmation with busy/focus
guards, focused structured-log highlighting, additional pinned ISO date spellings
and case-insensitive validation output/backend selectors. The combined source
passed both Linux production feature gates, formatting and the Windows x86_64
GNU CLI compilation/link boundary; native acceptance remains open.

All F01–F28 acceptance rows remain partial and F29 remains unverified. The user
requires implementation before tests/CI and prohibits filesystem-test creation,
editing and execution. Source review does not establish runtime acceptance.
The [production gates](production-gates-2026-10-02.md) now include both feature
configurations for CLI composition, owned process execution, bounded Nexus intake
and desktop bindings. They cover only their named source boundaries. The latest combined boundary additionally covers Windows checkpoint library and
persistent external-job integration, staged patcher INI preparation, resource
hierarchy/multi-selection, saved update rehydration, nonblocking typed logs and
CLI preference/key provenance. Linux production build and strict Clippy passed
in both feature modes. Windows x86_64 GNU CLI production compilation/linking
also passed with the documented production-only unrar_sys target correction.
Native Windows execution, the separate GUI launcher, package delivery and all
behavioral acceptance remain unverified.

The current bounded source batch adds pending protocol FIFOs/backpressure,
BOM-aware cleanup with complete deletion planning, one cancellable filename
inventory per planning/discovery invocation, and bounded FOMOD metadata/XML
inspection using the real engine cancellation flag. It also replaces YAML plain
scalar inference with the pinned default-object reader profile, retaining tags,
style, null-key context, ordered mappings and bounded alias expansion. The four
bounded contracts and YAML intake contract record explicit Rust admission and
representation differences. Independent source reviews and production compilation
are recorded in the production receipt; runtime and domain acceptance remain open.

## Historical work snapshot — 2026-09-28

- Current continuation starts from merged downloads GUI step 1, `d438528` (PR #5).
  Step 2 adds peer-settings disclosure/preserving saves, a bound serial FOMOD queue,
  error retry/dismiss ownership, readiness guards and responsive navigation.
  Independent source reviews are in [the step-2 review](downloads-gui-step2-review.md);
  production/native evidence is tracked in [the step-2 proof](downloads-gui-step2/README.md).
  The bounded step-2 slice is accepted: five production gates, 46/46 native
  assertions across five isolated scenarios, and 53 unchanged frozen files/regions.
  Next: the remaining downloads step-3 native matrix, then persistent sharing.
  Earlier in-flight statements below are historical to their recorded commits.
- Wave 1 is recorded at `origin/main` `4bfacda`. Wave 2 is local on
  `codex/rust-parity-wave2-resume` in the isolated integration worktree.
- Seven earlier approved lanes are merged: P2P proxy core, profiles/settings,
  download library, texture priority, loader deduplication, completion CLI,
  and completion-page UI. Deployment round four is merged as well.
- Combined production builds and Clippy pass in CLI-only and desktop modes.
  All 82 deployment CLI checks pass against the combined binary.
- Twelve focused CLI assertions cover an actual disposable install, peer-staging
  exclusion, completion summaries, and API-key/endpoint merge behavior. Native
  Xvfb proof covers startup/preload rendering only. Earlier completion-page
  native evidence keeps its original source/binary binding.
- Final integration spec review identified profile-switch locking and ignored
  source-profile errors. Commit `2252894` fixes these; seven direct production
  CLI scenarios pass, including held locks and failed-purge refusal. Spec
  re-review passed. Quality review reproduced a different soft-skip case where
  modified files lose their manifest and switching proceeds; the follow-up
  uses existing read-only blockers before any purge and moves the active-profile
  read under the game lock. Correction `714e59b` plus receipts `90efdf2` passes
  all 39 switch CLI assertions and both production feature gates. Spec re-review
  and quality re-reviews pass for the bounded integration.
  Wave 2 has not been pushed.

[Evidence index](wave2-integration/README.md) ·
[Deployment receipts](deployment-round4/build-receipt.json) ·
[Unchanged frozen tests](wave2-integration/freeze-check.json)

## Next implementation sequence

1. Wave-two integration and profile-switch findings are reviewed and recorded
   locally; maintain their source-bound receipts as later slices land.
2. Implement the [approved selected-guide CLI slice](fetch-downloads-cli-plan.md):
   orchestrator gate hardening, a concrete peer adapter, `fetch-downloads`, and
   direct local transfer proof. Source `efefba1` plus correction `2889ec5`, with
   current evidence `14783d2`, passed independent spec and quality reviews.
   Acquisition checks pass 312/312, production gates pass in both feature modes,
   and 53 frozen test files/regions are unchanged. The controller's separate
   [41-check prior-candidate rerun and narrow final NDJSON probe](fetch-controller-review/README.md)
   retain their respective binary bindings. These counts cover fixtures, not domains.
   The [loopback HTTPS proof slice](origin-loopback-probe-plan.md) implementation at `6c08656` has passed independent source SPEC and QUALITY reviews. Its 53 isolated runtime cases, both production gates and
   frozen-source comparison are retained in [the receipt](origin-loopback-proof/README.md).
   Shared wizard/panel/settings wiring follows. [Downloads GUI](downloads-gui-plan.md)
   step 1 is implemented but not reviewed: commit `6ac257f` plus an uncommitted
   review follow-up. It covers the
   desktop job adapter (Fetch, the nxm handoff through the shared orchestrator,
   run-bound progress, Stop, record-on-clone and redaction) and safe probe/settings
   initialization. [Native Xvfb proof](downloads-gui/README.md) passes 46/46 checks
   in an isolated network namespace, including a same-fixture CLI comparison. The
   regression probes still pass (S2b 254, S2a 224, S1 339, inherited 41, H1a 138,
   fetch-shared-url 381). Next: its SPEC then QUALITY review, then step 2 (settings,
   guarded FOMOD queue, navigation) and step 3 (native acceptance). Persistent
   sharing and remaining CLI FOMOD behavior have separately approved plans. Standalone torrent verbs
   and library presence do not satisfy those frontend/lifecycle tasks.
3. Fix loader Source normalization. [Seven JSON cases](source-normalization-probe/receipt.json)
   and equivalent [TOML/YAML/XML cases](source-normalization-formats/receipt.json)
   reproduce five differences per format against C#. Expanded evidence reproduces
   the same gap in Markdown embedded YAML and shows an actual bare-relative Copy
   succeeding in C# but rejected by Rust. Rust copies identical bytes after the
   Source is normalized by C#. The expanded implementation plan is independently
   approved; source `62b6f70` and [production evidence](source-normalization-proof/README.md)
   are committed. Independent source SPEC and QUALITY reviews approved this bounded fix. Rust Markdown export and
   its missing legacy-bullet metadata route remain separate approved work.
4. The first [FOMOD integration slice](fomod-remaining-plan.md) is now the sole
   production implementation: CLI policy/dispatch, consumer binding and cancellable
   terminal input. S2a (source commit `0d6aad1`, review follow-up commit
   `93837e5`, round-2 follow-up commit `a436c05` and a round-3 follow-up) adds `--fomod-choices` same-archive reconfiguration, stale-choice
   refusal against the current archive, an actionable C#-configured import
   requirement, a repair hint for unparseable saved choices, and the no-Extract route
   warning; [evidence](fomod-s2a-proof/README.md) is 224/224. The round-3 follow-up is
   `ae8d7e9`; S2a SPEC and QUALITY are [approved](fomod-s2a-review.md). S2b
   (commit `37cd4cc`, review follow-up `c28dee0` that dedupes and lists gate
   archives, restores C#'s exact fallback text, adds iota-subscript case mapping and
   scopes run choices to referencing components, and second follow-up `6dfbf63` (SPEC and QUALITY approved)
   that matches that scope to engine source resolution so folder and glob
   character-class Extract sources keep their choices; completed components, and so a
   completed component no entry references, are never touched; second follow-up not
   yet reviewed) makes the run-scoped
   `validate`/`install --fomod-choices` Rust extension merge per archive, matches
   FOMOD names like C# `OrdinalIgnoreCase` including Unicode, runs the FOMOD gate on
   plain `validate` and `install`, and logs C#'s mapper fallback warnings;
   [evidence](fomod-s2b-proof/README.md) is 254/254 (19 of 232 fail on `37cd4cc`; 10
   of 254 fail on `c28dee0`). Fetch now
   runs components that may publish one archive file as one serialized group
   instead of a race (commit `4bf5c54`, its review follow-up `339b3d6`, and a second
   follow-up that makes groups transitive and requires the same name for inherited
   failures). A later component takes a file already published under its name
   (subject to its own hash) or the outcome of identical sources for the same name
   without a request; otherwise, for example a different URL after the owner failed,
   it downloads for itself. Every component is reported, recorded and bound as a
   FOMOD consumer. The second follow-up is `3f0caf4`; SPEC and QUALITY are approved.
   A third follow-up, `d4ccbc1` (PR bodencrouch/ModSync.rs#3), makes the two changes
   that review recommended: an unnamed target whose transfer reveals a name already
   present adopts that file under its own hash, or with no hash only when the bytes
   match, and otherwise fails clearly without replacing it (grouped, ungrouped with one
   worker, and on reruns; the Peer publish paths match by source only); and an unnamed
   target inherits only an unnamed member's failure (354/354; 38 new or changed checks
   fail on `45b3c64`). An independent review approved `d4ccbc1` for SPEC and QUALITY.
   The polish follow-up `7ba8b1b` (SPEC and QUALITY approved) addresses that review's five
   non-blocking findings: a race adopts a present file before it stops Origin, and a
   refused Peer adoption lets Origin continue; a refused adoption ends the target only
   when a later source would reveal the same name, else it falls through; a cancelled
   run never adopts; an unnamed Origin publish that loses a no-clobber race adopts
   instead of failing with `File exists`; and the README states the reported source and
   what `origin_denies_peer` does. [Evidence](fetch-shared-url-proof/README.md) is
   381/381 (10 new checks fail on `d4ccbc1`; the race and Fallback changes are
   source-only). The later residuals are settled in source: an Origin-present name in a
   race is adopted before Peer stops and a refusal leaves Peer running; hash passes take
   the run's cancel flag; and revealed or known names adopt through `DestinationExists`.
   The [download residuals proof](download-residuals-proof/README.md) (102/102; 16 fail
   on `9548a73`) accepts these over the production CLI. It also fixes two runtime gaps:
   a known name taken by an ungrouped target *before* its transfer started failed with
   the untyped `Download destination already exists` (Origin now publishes a known name
   through the same `Directory` path, and MEGA drops its pre-transfer check), and Origin
   `transferring.total` was read after the sniff prefix (`0` for a small archive). Ctrl+C
   during planning now exits 130. Its review's three Low follow-ups are fixed: a hashed
   target whose name is already present is adopted or refused with no transfer; a Peer
   copy for a known name that collides is typed and adopted or refused as Origin is
   (source only, no Peer on this kernel); and Ctrl+C during execution-time discovery
   exits 130. The extended proof is 183/183 (9 fail on `59e4e57`). Only the race's
   refusal-to-Peer path is still without runtime evidence (no Peer seeding on this
   kernel). (An older note here said one-run `validate/install --fomod-choices`
   replaces all embedded archive choices; S2b superseded that with a per-archive merge.)
   The FOMOD residuals are settled by the [gate-scope contract](fomod-gate-scope.md):
   the gate expands the selection with hard dependencies like C#
   `ExpandWithHardDependencies` and reports their FOMOD findings before Rust's stricter
   selection error; a missing registered archive without prompt state gets C#'s
   per-component soft warning; run-choice ownership, archive-suffixed folder sources and
   per-archive preservation are proved at runtime; and the `--fomod-choices` help says
   it merges per archive. [Evidence](fomod-gate-scope-proof/README.md) is 74/74 (12
   fail on `15b11cb`); S1 339/339, inherited 41/41, S2a 224/224 and S2b 254/254 hold.
   Soft gate warnings now survive a later hard gate failure
   ([gate-warnings contract](fomod-gate-warnings.md), 55/55; 17 fail on `438bb73`),
   and the gate collects every issue and warning instead of stopping at the first issue
   ([gate-collect contract](fomod-gate-collect.md), 67/67; 22 fail on `d78aea2`).
   Desktop reconfiguration, the S3 corpora and native acceptance follow.
   The [actual full.md conversion baseline](full-markdown-baseline/README.md)
   retains eight production conversions and an independent anonymous comparison.
   Both engines draft 72 instructions for 186 K1 components and 56 for 145 K2
   components. Generated instruction content matches under documented GUID and
   omission rules; the Delete Overwrite export/default distinction is retained.
   These results do not establish complete prose coverage or any installation.
5. Use the [full remaining-scope map](remaining-scope-map.md) to reconcile actual
   source and derive bounded tasks where a current plan is missing: engine actions/
   archives, patchers, per-component checkpoints, hosts/FOMOD, editor/conflicts/
   update/profile UI, complete wizard/main-window controls, CLI contracts,
   diagnostics and packaging. Historical checkpoints do not establish that all
   remaining domains already have complete implementation plans.
   Nexus H1a (shared same-origin API redirect policy, query-encoded nxm keys) has
   [isolated production-CLI evidence](nexus-h1a-proof/README.md), 108/108 checks.
   It still needs independent SPEC and QUALITY review. The https-to-http downgrade
   is covered by source only, and the rest of H1 is still open.
6. Perform requirement-by-requirement acceptance, ending with F29: authorized,
   isolated K1/K2 installs with identical inputs/selections, ordered per-step
   comparisons, final resource content checks, and live-game evidence.


The next integrated source boundary adds format-specific known-property access
and raw review fences, Unix effective-total offset clamping with conditional
rule reconstruction, qualified HTTP(S) numeric IP merge domains and bounded
System32 ICU alternative Windows timezone IDs. Independent specification and
quality review and both Linux production build/strict-lint configurations passed;
Windows GNU no-default compile/link also passed. See the format-property, merge
and profile-date contracts and production receipt. Behavioral/native acceptance
and the remaining Action, GUID, input/autodetection and IRI source work stay open.


The subsequent source boundary adds the pinned Action enum/getter profile,
shared .NET character GUID parsing for profiles, bounded guide input and terminal
autodetection errors, UTF-8 stdin replacement preservation and qualified Unicode
DNS host canonicalization. Independent specification/quality reviews accepted
all four scopes; both Linux production build/strict-lint configurations and
Windows GNU no-default compile/link passed. Stored guide IDs and checkpoint
proofs are not rewritten. No tests, harnesses, runtime or CI ran. Boolean getter,
JSON nonobject intake, merge GUID comparison and CLI routing remain next source
contracts; behavioral and whole-domain acceptance stay open.



The Boolean/JSON/merge/CLI source boundary now implements optional Boolean
conversion and XML selfclosing-null qualification in consumed containers, JSON
nonobject component filtering after raw review fences, typed GUID merge equality
and target-first set unions, early validation-only flag refusal, and bounded bare
protocol GUI routing. Independent SPEC and QUALITY reviews accepted all scopes.
Linux default/no-default production builds and strict Clippy plus Windows GNU
no-default compile/link passed. Frozen inline test regions, existing tables and
Cargo.lock remain unchanged. No tests, harnesses, runtime/CLI/native probes,
installs, packaging producers or CI ran. Protocol cancellation/owned desktop
results and metadata scalar/list conversion remain subsequent source work;
whole-domain behavioral acceptance stays open.



The protocol/text source boundary adds cancellable library and CLI guide loading,
an owned joined desktop protocol worker with run/input/token admission through
sharing acknowledgement, qualified component/option scalar and list getters,
and optional JSON BigInteger defaults within the represented UInt64 range.
Independent SPEC and separate QUALITY reviews accepted all scopes. Linux default
and no-default production builds/strict Clippy and Windows GNU no-default
compile/link passed. Existing UI inline tests remain byte-identical; other frozen
modules, casing tables and Cargo.lock are unchanged. No tests, harnesses, runtime,
native/CLI probes, installs, producers or CI ran. See the
[current source triage](source-triage-2026-10-02.md) for actual F05-F15 source gaps;
parser-origin, floating getter, protocol FIFO and broader acceptance stay open.

## Rules and unresolved proof boundaries

- Fresh implementer per task; specification review before code-quality review.
- No Rust tests or temporary Rust harnesses created, changed or run until full
  parity. Production compilation is not all-target/test/CI acceptance. The
  inherited frozen UI test error and CI test-execution conflict remain open.
- Preserve the dirty primary checkout, AGENTS.md, live games/baselines and
  other agents' work. No new live-host downloads in acquisition fixture work.
- Keep authentication, CAPTCHA and free-user wait boundaries explicit; peer
  copies never bypass them. Proxied peers fail closed without a usable proxy.
- No full-build evidence or cross-platform artifact acceptance exists for this
  integration. Current machine-local paths and tmpfs evidence must be refreshed
  or preserved before relying on them in the final acceptance run.
