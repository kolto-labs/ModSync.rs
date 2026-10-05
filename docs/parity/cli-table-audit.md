# F26 CLI table audit

Status: source-complete comparison at Rust `efce242f916020cfac76ea5352b9c75ed3e52c49` (CLI source audited at parent `cf64f539776e730fd16ae25a5d83402ccdf7cfa0`; the intervening commit changes only `docs/parity/gui-tools-audit.md` and `docs/parity/gui-tools-plan.md`) and pinned C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`; bounded prebuilt-binary probes complete. This audit does not claim F26 complete. The ledger still calls the full table comparison open (`docs/parity/ACCEPTANCE.md:5-9,61`).

## Scope, evidence, and labels

The C# product has two CLI surfaces. The Core executable owns nine verbs through `ModBuildConverter`; the GUI executable separately parses preload and protocol arguments (`src/ModSync.Core/CLI/ModBuildConverter.cs:364-778`; `src/ModSync.GUI/CLIArguments.cs:19-115`). Rust combines its verb parser and explicit `gui` verb in `src/main.rs`, with profile action handling in `src/profile_cli.rs` (`src/main.rs:12-236,287-364`; `src/profile_cli.rs:7-41`). Both surfaces are in F26.

References below use these exact pinned paths:

- **C# MBC**: `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.Core/CLI/ModBuildConverter.cs`
- **C# GUI args**: `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/CLIArguments.cs`
- **Rust main/profile**: `src/main.rs` and `src/profile_cli.rs`

Labels are strict:

- **matched**: the invocation-level name, requiredness/default and observable contract match for the row.
- **differing**: an analogous capability exists, but syntax, default, semantics, JSON, stream, or exit status differs; Rust-only extensions also use this label with that reason.
- **missing**: the C# row has no Rust invocation-level equivalent. A lower-level library capability does not upgrade the row.

The C# Core parser recognizes exactly nine declared verbs and maps every parser error—including help/version—to `1` (`C# MBC:385-746,765-778`). Rust declares 25 named top-level verbs plus nested deployment actions, and captures every otherwise unknown subcommand as a prospective protocol handoff (`src/main.rs:21-236,401-415`).

## Verb-by-verb table

| C# verb or surface | C# contract | Rust counterpart | Status and reason | Referent |
|---|---|---|---|---|
| `convert` | convert; legacy merge mode; default TOML; stdout without output path | `convert` | **differing** — core conversion/stdin/drafting overlap, but C# download/selection/merge/spoiler controls are absent and Rust adds positional forms/review | C# MBC:385-483,1815-2370; `src/main.rs:154-178,738-794` |
| `merge` | two inputs; stdout or file; download/selection/FOMOD hooks | `merge` | **differing** — Rust requires positional output and lacks download/selection/FOMOD/spoiler controls; field names also differ | C# MBC:485-568,2384-2793; `src/main.rs:109-140,696-737` |
| `validate` | structural/full/dry-run pipeline; text or one JSON report | `validate` | **differing** — shared pipeline exists, but flag spelling/modes and ordinary JSON are different | C# MBC:570-605,3014-3275; `src/main.rs:141-142,287-364,897-1163` |
| `install` | validation, optional downloads, prompts, patcher and managed/best-effort controls | `install` | **differing** — shared safe pipeline exists, but many C# controls are absent or deliberately incompatible with fail-stop policy | C# MBC:607-678,3279-3696; `src/main.rs:145-146,287-364,897-1163` |
| `set-nexus-api-key` | validate and persist positional secret | none | **missing** — Rust consumes settings/env keys in download/update paths but has no setter verb | C# MBC:680-688,4070-4129; `src/main.rs:633-650,674-688` |
| `install-python-deps` | install embedded Python dependencies | none | **missing** | C# MBC:690-695,4134-4325; `src/main.rs:21-236` |
| `holopatcher` | locate and invoke bundled HoloPatcher with a raw argument string | none | **missing** — Rust invokes configured patchers only inside installation | C# MBC:697-702,4329-4401; `src/main.rs:321-339,1061-1070` |
| `profile` | list/show/create/delete/activate/clone/rename | `profile` | **differing** — seven actions overlap; Rust adds capture/apply, defaults action to list, and always emits JSON | C# MBC:704-724,3700-3748; `src/profile_cli.rs:7-115` |
| `settings` | list/get/set, typed values, redaction | none | **missing** | C# MBC:726-746,3752-3790,3861-4043; `src/main.rs:21-236` |
| GUI preload / protocol surface | implicit GUI; `key=value` preload; bare protocol URLs | `gui`; external-subcommand URL | **differing** — names and grammar differ; Rust bare URL currently parses/prints JSON instead of launching GUI | C# GUI args:48-115; `src/main.rs:23-45,401-432,1168-1200` |
| `deployment status/deploy/uninstall/purge` | none in C# CLI | same Rust verbs | **differing** — Rust-only extension | `src/main.rs:25-26,237-286,433-480` |
| `fomod-inspect` | no standalone C# verb | same Rust verb | **differing** — Rust-only extension over shared FOMOD concepts | `src/main.rs:46-65,482-580` |
| `list` | no standalone C# verb | same Rust verb | **differing** — Rust-only extension | `src/main.rs:66-67,581-584` |
| `protocol-parse/fetch/open/resolve/register-linux` | GUI handoff exists, no equivalent Core verbs | same Rust verbs | **differing** — Rust-only diagnostic/registration surface | `src/main.rs:68-81,585-632` |
| `nxm-download` | install/convert internal orchestration only | same Rust verb | **differing** — Rust-only standalone wrapper | `src/main.rs:82-90,633-654` |
| `update-apply/update-check` | no C# Core verb | same Rust verbs | **differing** — Rust-only standalone wrappers | `src/main.rs:91-108,655-695` |
| `conflicts` | validation stage, no standalone verb | same Rust verb | **differing** — Rust-only standalone JSON wrapper | `src/main.rs:143-144,897-1095` |
| `restore` | checkpoint logic, no C# Core verb | same Rust verb | **differing** — Rust-only standalone wrapper | `src/main.rs:147-153,795-799` |
| `download-plan/download/download-batch/download-provider` | convert/install internal orchestration | same Rust verbs | **differing** — Rust-only low-level wrappers | `src/main.rs:179-200,222-235,800-848,870-896` |
| `torrent-download/torrent-seed` | none | same Rust verbs | **differing** — Rust-only extension | `src/main.rs:201-221,849-869` |

## Parser-wide flags and exit behavior

Every C# verb inherits `BaseOptions`; Rust has no parser-wide equivalent (`C# MBC:364-383`; `src/main.rs:12-20`).

| C# name | Alias | C# default/shape | Rust equivalent | Status and reason | Referent |
|---|---|---|---|---|---|
| `--verbose` | `-v` | false | none | **missing** | C# MBC:366-367; `src/main.rs:12-20` |
| `--plaintext` | none | false | none | **missing** | C# MBC:369-370; `src/main.rs:12-20` |
| `--fomod-skip` | none | false | none | **missing** | C# MBC:372-373; `src/main.rs:287-364` |
| `--fomod-choices <PATH>` | none | null; inherited everywhere | `--fomod-choices <PATH>` on validate/conflicts/install | **differing** — same spelling only on install-family verbs | C# MBC:375-376; `src/main.rs:295-297` |
| `--interactive` | none | false | `fomod-inspect --interactive` | **differing** — scoped to a different verb; not install/download behavior | C# MBC:378-379; `src/main.rs:58-60` |
| `--non-interactive` | none | false | none | **missing** | C# MBC:381-382; `src/main.rs:287-364` |
| `--help` | C# long only; Rust `-h` too | prints help; observed C# exit 1, Rust exit 0 | generated help | **differing** — alias and exit | C# MBC:765-778; `src/main.rs:12-20`; `/tmp/modsync-cli-table-audit-20260927/{csharp,rust}/root.{stdout,exit}` |
| `--version` | Rust `-V` too; C# also pseudo-verb `version` | observed C# exit 1; Rust exit 0 | generated version | **differing** — version strings and exit | C# MBC:765-778; `src/main.rs:12-20`; `/tmp/modsync-cli-table-audit-20260927/runtime/*version*` |
| malformed/unknown verb | none | C# parse error exit 1 | Rust external-subcommand parse attempt then exit 1 for one token | **differing** — Rust treats unknown singleton as a URL candidate | C# MBC:767-778; `src/main.rs:23-24,403-415`; `/tmp/modsync-cli-table-audit-20260927/{csharp,rust}/bogus.{stdout,stderr,exit}` |

C# successful handlers normally return `0`, errors `1`; validation can propagate its pipeline exit and `holopatcher` can propagate the child exit (`C# MBC:3219-3240,4385-4392`). Rust returns `0` from a successful `run`, prints `Error: …` and exits `1` on any returned error, while Clap owns successful help/version termination (`src/main.rs:401-1166,1203-1207`).

## `convert` flag table

| C# flag | C# default/shape | Rust flag | Status and reason | Referent |
|---|---|---|---|---|
| `-i/--input` | optional path | `-i/--input`, plus positional input | **differing** — Rust adds positional form and explicit conflicts | C# MBC:388-389; `src/main.rs:155-163` |
| `-o/--output` | optional; stdout if absent | `-o/--output`, plus positional output | **differing** — Rust named output does not drive format inference | C# MBC:391-395; `src/main.rs:156-166,780-792` |
| `-f/--format` | `toml` | `-f/--format` optional; positional output extension may choose default | **differing** | C# MBC:394-395; `src/main.rs:164-166,780-786` |
| `-a/--auto` | false | none | **missing** | C# MBC:397-398; `src/main.rs:154-178` |
| `-d/--download` | false; requires source | none | **missing** | C# MBC:400-401,1972-1978; `src/main.rs:154-178` |
| `-s/--select` | repeatable category/tier | none | **missing** (since added: `-s, --select`, one value per flag; see [select-values-contract.md](select-values-contract.md)) | C# MBC:403-404; `src/main.rs:154-178` |
| `--source-path` | optional path | `--source-path` | **matched** for name/shape | C# MBC:406-407; `src/main.rs:171-174` |
| `--nexus-mods-api-key` | optional secret | none | **missing** | C# MBC:409-410; `src/main.rs:154-178` |
| `-m/--merge` | false; legacy redirect | none | **missing** | C# MBC:412-419,1905-1943; `src/main.rs:154-178` |
| `-e/--existing` | optional for legacy merge | none | **missing** | C# MBC:415-416; `src/main.rs:154-178` |
| `-n/--incoming` | optional for legacy merge | none | **missing** | C# MBC:418-419; `src/main.rs:154-178` |
| `--exclude-existing-only` | false | merge verb only | **differing** — no convert legacy form | C# MBC:421-422; `src/main.rs:137-139` |
| `--exclude-incoming-only` | false | merge verb only | **differing** — no convert legacy form | C# MBC:424-425; `src/main.rs:137-139` |
| `--use-existing-order` | false | merge verb only | **differing** | C# MBC:427-428; `src/main.rs:114-115` |
| `--prefer-existing-fields` | false | merge verb only | **differing** | C# MBC:430-431; `src/main.rs:116-117` |
| `--prefer-incoming-fields` | false, documented default | none | **missing** — Rust represents this only by absence of prefer-existing | C# MBC:433-434; `src/main.rs:109-140` |
| `--prefer-existing-name` | false | merge verb only | **differing** | C# MBC:436-437; `src/main.rs:118-119` |
| `--prefer-existing-author` | false | merge verb only | **differing** | C# MBC:439-440; `src/main.rs:120-121` |
| `--prefer-existing-description` | false | merge verb only | **differing** | C# MBC:442-443; `src/main.rs:122-123` |
| `--prefer-existing-directions` | false | merge verb only | **differing** | C# MBC:445-446; `src/main.rs:124-125` |
| `--prefer-existing-category` | false | merge verb only | **differing** | C# MBC:448-449; `src/main.rs:126-127` |
| `--prefer-existing-tier` | false | merge verb only | **differing** | C# MBC:451-452; `src/main.rs:128-129` |
| `--prefer-existing-installation-method` | false | none | **missing** | C# MBC:454-455; `src/main.rs:109-140` |
| `--prefer-existing-instructions` | false | merge verb only | **differing** | C# MBC:457-458; `src/main.rs:130-131` |
| `--prefer-existing-options` | false | merge verb only | **differing** | C# MBC:460-461; `src/main.rs:132-133` |
| `--prefer-existing-modlinks` | false | `--prefer-existing-resource-registry` on merge | **differing** — names and field scope differ | C# MBC:463-464; `src/main.rs:134-135` |
| `--concurrent` | false | none | **missing** | C# MBC:466-467; `src/main.rs:154-178` |
| `--ignore-errors` | false | none | **missing** | C# MBC:469-470; `src/main.rs:154-178` |
| `--spoiler-free <PATH>` | null | none | **missing** | C# MBC:472-473; `src/main.rs:154-178` |
| `--auto-generate-local` | false; requires source | same | **matched** for name/requirement | C# MBC:475-476,1980-1984; `src/main.rs:170-174` |
| `--stdin` | false; conflicts with input | same | **matched** | C# MBC:478-479,1957-1969; `src/main.rs:158-163` |
| `--parse-directions` | false | same | **matched** | C# MBC:481-482; `src/main.rs:167-169,762-764` |
| none | — | repeatable `--review-draft <GUID>` | **differing** — Rust-only extension | `src/main.rs:175-177,772-779` |

The C# legacy `convert --merge` adapter also drops `concurrent`, `ignore-errors`, `spoiler-free`, and inherited FOMOD settings other than verbose/plaintext when constructing `MergeOptions`; this is a pinned-source defect, not a behavior Rust should duplicate (`C# MBC:1905-1943`).

## `merge` flag table

| C# flag | C# default/shape | Rust flag | Status and reason | Referent |
|---|---|---|---|---|
| `-e/--existing` | required | positional `EXISTING` | **differing** — alias/grammar | C# MBC:488-489; `src/main.rs:110-113` |
| `-n/--incoming` | required | positional `INCOMING` | **differing** — alias/grammar | C# MBC:491-492; `src/main.rs:110-113` |
| `-o/--output` | optional; stdout if absent | positional `OUTPUT`, required | **differing** | C# MBC:494-498; `src/main.rs:110-113,736-737` |
| `-f/--format` | `toml` | inferred from output | **differing** | C# MBC:497-498; `src/main.rs:714-737` |
| `-d/--download` | false | none | **missing** | C# MBC:500-501; `src/main.rs:109-140` |
| `-s/--select` | repeatable | none | **missing** (since added: `-s, --select`, one value per flag; see [select-values-contract.md](select-values-contract.md)) | C# MBC:503-504; `src/main.rs:109-140` |
| `--source-path` | optional | none | **missing** | C# MBC:506-507; `src/main.rs:109-140` |
| `--nexus-mods-api-key` | optional | none | **missing** | C# MBC:509-510; `src/main.rs:109-140` |
| `--exclude-existing-only` | false | same | **matched** | C# MBC:512-513; `src/main.rs:137-139` |
| `--exclude-incoming-only` | false | same | **matched** | C# MBC:515-516; `src/main.rs:137-139` |
| `--use-existing-order` | false | same | **matched** | C# MBC:518-519; `src/main.rs:114-115` |
| `--prefer-existing-fields` | false | same | **matched** | C# MBC:521-522; `src/main.rs:116-117` |
| `--prefer-incoming-fields` | false/default behavior | none | **missing** as explicit compatibility alias | C# MBC:524-525; `src/main.rs:109-140` |
| `--prefer-existing-name` | false | same | **matched** | C# MBC:527-528; `src/main.rs:118-119` |
| `--prefer-existing-author` | false | same | **matched** | C# MBC:530-531; `src/main.rs:120-121` |
| `--prefer-existing-description` | false | same | **matched** | C# MBC:533-534; `src/main.rs:122-123` |
| `--prefer-existing-directions` | false | same | **matched** | C# MBC:536-537; `src/main.rs:124-125` |
| `--prefer-existing-category` | false | same | **matched** | C# MBC:539-540; `src/main.rs:126-127` |
| `--prefer-existing-tier` | false | same | **matched** | C# MBC:542-543; `src/main.rs:128-129` |
| `--prefer-existing-installation-method` | false | none | **missing** | C# MBC:545-546; `src/main.rs:109-140` |
| `--prefer-existing-instructions` | false | same | **matched** | C# MBC:548-549; `src/main.rs:130-131` |
| `--prefer-existing-options` | false | same | **matched** | C# MBC:551-552; `src/main.rs:132-133` |
| `--prefer-existing-modlinks` | false | `--prefer-existing-resource-registry` | **differing** | C# MBC:554-555; `src/main.rs:134-135` |
| `--concurrent` | false | none | **missing** | C# MBC:557-558; `src/main.rs:109-140` |
| `--ignore-errors` | false | none | **missing** | C# MBC:560-561; `src/main.rs:109-140` |
| `--spoiler-free <PATH>` | null | none | **missing** | C# MBC:563-564; `src/main.rs:109-140` |
| `--auto-generate-local` | false | none | **missing** | C# MBC:566-567; `src/main.rs:109-140` |

## `validate` flag table

| C# flag | C# default/shape | Rust flag | Status and reason | Referent |
|---|---|---|---|---|
| `-i/--input` | required | same unless profile supplies it | **differing** — Rust allows profile-derived input | C# MBC:573-574; `src/main.rs:298-306` |
| `-g/--game-dir` | optional except full/dry modes | `--game-dir`/`-g`, profile-capable | **differing** | C# MBC:576-577,3042-3078; `src/main.rs:300-303` |
| `-s/--source-dir` | optional except full/dry modes | `--source-dir`/`-s`, profile-capable | **differing** | C# MBC:579-580,3042-3078; `src/main.rs:302-303` |
| `--select` | list (several values after one flag; a repeated flag is refused) | repeatable name/GUID/category/tier | **differing** — Rust grammar is broader. 2026-10-05: C#'s list form `--select a b` (CommandLineParser sequence; a repeated flag is refused there) is now read as in C#, and a repeated flag still works ([select-values-contract.md](select-values-contract.md)) | C# MBC:582-583; `src/main.rs:315-317,989-997` |
| `--full` | false | same, requires `--validation-report` | **differing** — C# mode directly selects pipeline; Rust gates it behind report mode | C# MBC:585-586; `src/main.rs:352-360` |
| `--dry-run` | false | same, requires report | **differing** | C# MBC:588-589; `src/main.rs:355-357` |
| `--dry-run-only` | false | same, requires report; remaining forbidden | **differing** | C# MBC:591-592; `src/main.rs:361-363,920-923` |
| `--errors-only` | false | none | **missing** | C# MBC:594-595; `src/main.rs:287-364` |
| `--ignore-errors` | false | none | **missing** | C# MBC:597-598; `src/main.rs:287-364` |
| `--use-file-selection` | false; absent select means all | same | **matched** | C# MBC:600-601,3145-3203; `src/main.rs:312-317,989-1004` |
| `--output text\|json` | `text` | `--json` NDJSON events or `--validation-report` one report | **differing** — schemas and stream model differ | C# MBC:603-604,3005-3010,3219-3266; `src/main.rs:340-357,1072-1085,1122-1159` |
| none | — | `--phase all\|base\|remaining`, default all | **differing** — Rust-only phased wrapper | `src/main.rs:289-291,366-379` |
| none | — | `--profile`, `--active-profile`, `--settings-dir` | **differing** — Rust-only profile path/selection wrapper | `src/main.rs:304-311,924-973` |
| none | — | repeatable `--option <GUID>` | **differing** — Rust-only explicit option selection | `src/main.rs:318-320,1005-1022` |
| none | — | `--allow-processes` | **differing** — Rust-only safety opt-in | `src/main.rs:321-323,1061-1063` |
| none | — | `--patcher`, `--patcher-engine`, scratch/budget controls | **differing** — Rust exposes engine preflight inputs during validation | `src/main.rs:324-339,933-947,1061-1070` |

## `install` flag table

Note (2026-10-05): the rows for `--no-checkpoint`, `--skip-validation`, `-y/--yes`,
`-d/--download`, `--concurrent`, the continuation flags, `--best-effort`,
`--nexus-api-key`, `--download-timeout-hours` and `--ignore-errors` were refreshed
from source at `f6d2ec5` and checked with the release binary (the refused flags exit
1 before anything runs). Line references in the other rows still predate later
slices.

| C# flag | C# default/shape | Rust flag | Status and reason | Referent |
|---|---|---|---|---|
| `-i/--input` | required | same unless profile supplies it | **differing** | C# MBC:610-611; `src/main.rs:298-306` |
| `-g/--game-dir` | required | `--game-dir`/`-g`, profile-capable | **differing** | C# MBC:613-614; `src/main.rs:300-306` |
| `-s/--source-dir` | input directory if absent | required unless profile supplies it; `-s` accepted | **differing** — default differs | C# MBC:616-617,3342-3347; `src/main.rs:302-306,962-973` |
| `--select` | list (several values after one flag; a repeated flag is refused) | repeatable broader selector | **differing**. 2026-10-05: `--select a b` is now read as in C#, as for `validate` ([select-values-contract.md](select-values-contract.md)) | C# MBC:619-620; `src/main.rs:315-317,989-997` |
| `--use-file-selection` | false; absent select means all | same | **matched** | C# MBC:622-623; `src/main.rs:312-317,989-1004` |
| `--no-checkpoint` | false | same | **differing** — keeps the whole-run recovery snapshot and drops only per-component history; conflicts with `--component-checkpoints` and `--managed` | C# MBC:625-626; `src/main.rs:690-692` |
| `--skip-validation` | false | hidden, refused by name before anything runs (exit 1) | **differing** — Rust preflight is mandatory | C# MBC:628-629; `src/main.rs:698-699,2998-3016` |
| `-y/--yes` | false | hidden, refused by name before anything runs (exit 1) | **differing** — Rust has no install confirmation prompt | C# MBC:631-632; `src/main.rs:706-707,2998-3016` |
| `-d/--download` | false | same | **differing** — same spelling; acquires the selected resources before validation and the snapshot through Rust's shared fetch pipeline, not compared invocation by invocation | C# MBC:634-635; `src/main.rs:675-677` |
| `--concurrent` | false | same, requires `-d` | **differing** — four download workers instead of one; refused without `-d` | C# MBC:637-638; `src/main.rs:680-682` |
| `--patcher-engine` | optional enum | same canonical Rust enum plus value aliases | **differing** — accepted values/spelling differ | C# MBC:640-641; `src/main.rs:331-333,382-398` |
| `--kpatcher-path` | optional | visible alias of `--patcher` | **differing** — shared destination and no backend-specific validation at parse time | C# MBC:643-644; `src/main.rs:324-330` |
| `--odypatcher-path` | optional | visible alias of `--patcher` | **differing** | C# MBC:646-647; `src/main.rs:324-330` |
| `--direct-markdown` | false | same | **differing** — Rust additionally conflicts with profiles/options/FOMOD and forbids phased use | C# MBC:649-650,3286-3305; `src/main.rs:292-297,910-923,975-1047` |
| `--continue-on-missing-sources` | false | hidden, refused by name before anything runs (exit 1) | **differing** — conflicts with binding fail-stop rule | C# MBC:652-653,3656-3663; `src/main.rs:700-701,2998-3016` |
| `--continue-on-mod-failure` | false | hidden, refused by name before anything runs (exit 1) | **differing** — conflicts with binding fail-stop rule | C# MBC:655-656,3665-3671; `src/main.rs:702-703,2998-3016` |
| `--best-effort` | false; enables both continuation flags and `-y`; partial failure can exit 0 | same, alias `--continue-on-error`; parsed, then refused before anything runs (exit 1) | **differing** — unsafe reference behavior cannot be silently copied | C# MBC:658-659,3359-3365,3656-3671; `src/main.rs:693-695,3106-3109`; `docs/parity/ACCEPTANCE.md:75` |
| `--nexus-api-key` | optional | same, alias of `--nexus-mods-api-key` | **matched** for name/shape | C# MBC:661-662; `src/main.rs:678-679` |
| `--download-timeout-hours` | 48 | same, default 48; `0` also means 48 | **matched** for name/default | C# MBC:664-665; `src/main.rs:683-685` |
| `--ignore-errors` | false | hidden, refused by name before anything runs (exit 1) | **differing** — dependency failures remain fail-closed | C# MBC:667-668; `src/main.rs:704-705,2998-3016` |
| `--managed` | false | `install --managed` (needs `--component-checkpoints` or a checkpoint resume) | **differing** | C# MBC:670-671; `src/main.rs:25-26,237-286` |
| `--no-managed` | false | same; classic install, the Rust CLI default | **differing** — 2026-10-05: accepted and refused beside `--managed` with C#'s message; the CLI never reads settings.json's managed toggle, so the flag changes nothing else ([no-managed-contract.md](no-managed-contract.md)) | C# MBC:673-674; `src/main.rs:711-713,3016-3023` |
| `--profile` | optional | same name, different meaning: stored paths/selections | **differing** | C# MBC:676-677; `src/main.rs:304-311,948-973` |
| none | — | `--phase`, `--active-profile`, `--settings-dir`, `--option` | **differing** — Rust-only orchestration | `src/main.rs:289-320` |
| none | — | `--allow-processes` | **differing** — Rust-only explicit authorization | `src/main.rs:321-323,1061-1063` |
| none | — | `--scratch-dir`, `--max-expanded-bytes` | **differing** — Rust-only archive safety controls | `src/main.rs:334-339,1069-1070` |
| none | — | `--json` | **differing** — Rust legacy Event NDJSON | `src/main.rs:340-342,1072-1085` |
| none | — | `--install-progress-json` | **differing** — Rust typed progress NDJSON | `src/main.rs:343-351,1095-1116`; `docs/parity/GUI-CLI.md:38-52` |
| none | — | `--validation-report`, `--dry-run`, `--full`, `--dry-run-only` | **differing** — parser accepts some meaningless install combinations; see findings | `src/main.rs:352-363,908-923,1072-1146` |

## Remaining C# verbs and flags

| C# verb/flag | Alias/default | Rust equivalent | Status and reason | Referent |
|---|---|---|---|---|
| `set-nexus-api-key <api-key>` | required positional | none | **missing** | C# MBC:680-687; `src/main.rs:21-236` |
| `set-nexus-api-key --skip-validation` | false | none | **missing** | C# MBC:686-687; `src/main.rs:21-236` |
| `install-python-deps --force` | false | none | **missing** | C# MBC:690-695; `src/main.rs:21-236` |
| `holopatcher --args` | `-a`, default empty raw string | none | **missing** | C# MBC:697-702; `src/main.rs:21-236` |
| `profile --action` | `-a`, required | same, default `list` | **differing** | C# MBC:707-708; `src/profile_cli.rs:20-22` |
| `profile --name` | `-n` | same long only | **differing** — alias missing | C# MBC:710-711; `src/profile_cli.rs:25-26` |
| `profile --from` | optional; `--name` fallback for clone/rename | same, no name fallback | **differing** | C# MBC:713-717,3725-3734; `src/profile_cli.rs:27-30,64-75` |
| `profile --to` | optional | same | **matched** | C# MBC:716-717; `src/profile_cli.rs:29-30` |
| `profile --settings-dir` | AppData/ModSync | platform config/ModSync | **differing** — platform default mechanism | C# MBC:719-720; `src/profile_cli.rs:23-24`; `src/profiles.rs:12-20` |
| `profile --json` | false; only list/show | accepted but ignored; JSON always | **differing** | C# MBC:722-723,3817-3907; `src/profile_cli.rs:39-40,111-115` |
| `profile capture/apply` | absent | Rust-only action values | **differing** — Rust-only extension | `src/profile_cli.rs:7-18,84-110` |
| `profile capture/apply --input` | absent | optional at parse time; required at runtime for both actions | **differing** — Rust-only extension | `src/profile_cli.rs:32-33,84-86,97-99` |
| `profile apply --output` | absent | optional at parse time; required at runtime for apply | **differing** — Rust-only extension | `src/profile_cli.rs:34-35,97-100` |
| `profile capture --game-dir` | absent | optional | **differing** — Rust-only extension | `src/profile_cli.rs:36-37,88-94` |
| `profile capture --source-dir` | absent | optional | **differing** — Rust-only extension | `src/profile_cli.rs:38,88-94` |
| `settings --action` | `-a`, required, list/get/set | none | **missing** | C# MBC:729-730; `src/main.rs:21-236` |
| `settings --key` | `-k` | none | **missing** | C# MBC:732-733; `src/main.rs:21-236` |
| `settings --value` | typed JSON/scalar; empty removes | none | **missing** | C# MBC:735-736; `src/main.rs:21-236` |
| `settings --settings-dir` | AppData/ModSync | none | **missing** | C# MBC:738-739; `src/main.rs:21-236` |
| `settings --json` | false | none | **missing** | C# MBC:741-742; `src/main.rs:21-236` |
| `settings --reveal-secrets` | false | none | **missing** | C# MBC:744-745; `src/main.rs:21-236` |

## Rust-only flag inventory

These rows prevent Rust's existing public surface from disappearing while C# aliases are added. They are **differing** because the pinned C# CLI has no equivalent verb-level row.

| Rust invocation | Flags/positionals and defaults | Status | Referent |
|---|---|---|---|
| `gui` / no subcommand | GUI is also the no-subcommand default; optional `--settings-dir`, `--input`, `--game-dir`, `--source-dir`, `--protocol`; `--allow-multiple-instances=false` | **differing** | `src/main.rs:29-45,417-432` |
| `fomod-inspect` | `ARCHIVE`; optional `--choices`, `--extracted-directory`, `--save-choices`, `--interactive`, `--scratch-dir`, `--max-expanded-bytes` | **differing** | `src/main.rs:46-65` |
| `list` | `INPUT` | **differing** | `src/main.rs:66-67` |
| `protocol-parse` | `URL` | **differing** | `src/main.rs:68-69` |
| `protocol-fetch` | `URL OUTPUT` | **differing** | `src/main.rs:70-71` |
| `protocol-open` | `URL` | **differing** | `src/main.rs:72-73` |
| `protocol-resolve` | `INPUT URL` | **differing** | `src/main.rs:74-75` |
| `protocol-register-linux` | `EXECUTABLE`; optional `--applications-dir` | **differing** | `src/main.rs:76-81` |
| `nxm-download` | `URL OUTPUT`; optional `--nexus-mods-api-key`, `--settings-dir` | **differing** | `src/main.rs:82-90` |
| `update-apply` | `INPUT VERSIONS OUTPUT`; optional `--summary` | **differing** | `src/main.rs:91-98` |
| `update-check` | `INPUT OUTPUT`; optional `--nexus-mods-api-key`, `--settings-dir` | **differing** | `src/main.rs:99-108` |
| `restore` | required `--game-dir`, `--source-dir` | **differing** | `src/main.rs:147-153` |
| `download-plan` | `INPUT`; required `--component`, `--url`; repeatable `--filename`; optional `--source-dir` | **differing** | `src/main.rs:179-190` |
| `download` | `URL OUTPUT`; optional `--sha256`, `--max-expanded-bytes` | **differing** | `src/main.rs:191-200` |
| `torrent-download` | `MAGNET OUTPUT`; required `--sha256`; repeatable `--peer`; optional `--max-expanded-bytes` | **differing** | `src/main.rs:201-214` |
| `torrent-seed` | `ARCHIVE`; repeatable `--tracker` | **differing** | `src/main.rs:215-221` |
| `download-batch` | `INPUT`; `--workers=4`, `--retries=1`; optional scratch/budget | **differing** | `src/main.rs:222-233` |
| `download-provider` | `URL` | **differing** | `src/main.rs:234-235` |
| `deployment status` | required game/staging/manifest directories | **differing** | `src/main.rs:243-252` |
| `deployment deploy` | required game/staging/manifest directories and component; optional name | **differing** | `src/main.rs:253-265` |
| `deployment uninstall` | required game/staging/manifest directories and component | **differing** | `src/main.rs:266-276` |
| `deployment purge` | required game/staging/manifest directories | **differing** | `src/main.rs:277-285` |

## GUI preload, handoff, and wrapper comparison

| Concern | Pinned C# | Rust | Status | Referent |
|---|---|---|---|---|
| Game preload | `--kotorPath=<value>`; case-insensitive key; existing path is applied and absent path warns | `gui --game-dir <path>` | **differing** | C# GUI args:48-87; `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/App.axaml.cs:77-88`; `src/main.rs:29-45`; `src/ui.rs:1617-1645` |
| Mod preload | `--modDirectory=<value>`; existing path is applied and absent path warns | `gui --source-dir <path>` | **differing** | C# GUI args:48-87; `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/App.axaml.cs:64-75`; `src/main.rs:29-45` |
| Guide preload | `--instructionFile=<value>`; an existing file auto-loads after the window opens | `gui --input <path>` populates display state; loading remains explicit | **differing** | C# GUI args:48-87; `/home/brunner56/.codex/worktrees/5813/ModSync/src/ModSync.GUI/MainWindow.axaml.cs:458-468`; `src/main.rs:29-45`; `src/ui.rs:1617-1645,1719-1752` |
| NXM | `--nxm=<url>` or bare `nxm://…` | `gui --protocol <url>`; bare URL hits JSON parser | **differing** | C# GUI args:88-107; `src/main.rs:39-45,403-415` |
| ModSync URL | `--modsync=<url>` or bare `modsync://…` | `gui --protocol <url>`; bare URL hits JSON parser | **differing** | C# GUI args:92-113; `src/main.rs:39-45,403-415` |
| Multiple instance | `--allow-multiple-instances` | same under `gui` | **differing** — Rust requires named verb | C# GUI args:98-102; `src/main.rs:42-45` |
| Unknown args | silently ignored by GUI parser | rejected/captured by Clap | **differing** | C# GUI args:59-114; `src/main.rs:12-24` |
| Repeated preload | last value wins | Clap normally rejects repeated scalar | **differing** | C# GUI args:59-96; `src/main.rs:29-45` |
| Secondary launch | NXM wins forwarding if both supplied; primary enqueues both | one `--protocol`; secondary Unix wrapper forwards it | **differing** | C# GUI args:37-45; `src/ModSync.GUI/Program.cs:23-64,90-100`; `src/ui.rs:3179-3204` |
| Linux handler | external packaging path | generated entry invokes `gui --protocol %u` | **differing** — Rust extension is internally consistent | `src/protocol_registration.rs:48-69`; `src/main.rs:618-632` |
| Windows handler | GUI receives raw URL | Rust registry command supplies raw `%1`, which parses/prints JSON rather than launching GUI | **differing**, defective wrapper | `src/protocol_registration.rs:19-37`; `src/main.rs:403-415` |

## JSON and stream contracts

| Contract | C# shape | Rust shape | Status | Referent |
|---|---|---|---|---|
| validation early error | pretty camelCase `{success:false,exitCode:1,errorCount:0,warningCount:0,passedCount:0,componentCount:0,error}`; stdout only; exit 1 | same keys/types/stream/exit; error text differs | **differing** — schema matched, diagnostic string did not | C# MBC:3005-3010,3033-3077; `src/validation.rs:107-145`; `src/main.rs:1148-1159`; `/tmp/modsync-cli-table-audit-20260927/runtime/*validate-missing*` |
| validation report | camelCase report, stages and optional dry-run; process exit is pipeline exit | same family of report through shared Rust formatter; failure normalized to process 1 | **differing** — success fixtures and non-1 pipeline exits still need direct comparison | `src/ModSync.Core/Services/Validation/ValidationPipelineJsonFormatter.cs:22-179`; `src/validation.rs:15-145`; C# MBC:3219-3240; `src/main.rs:1122-1159` |
| ordinary validation JSON | no NDJSON; `--output json` selects one report | `--json` emits compact Event NDJSON `{component,step,message}` | **differing** | C# MBC:603-604,3219-3225; `src/main.rs:340-342,1072-1085` |
| install progress | text/logger output; no typed stream flag | compact NDJSON `{"type":"installProgress","progress":{…}}` | **differing** — Rust extension | `src/main.rs:343-351,1095-1116`; `docs/parity/GUI-CLI.md:38-52` |
| profile list JSON | PascalCase array, but observed stdout begins with logger initialization | clean pretty JSON array; always JSON | **differing** — C# machine stream is polluted | C# MBC:3817-3837; `src/profile_cli.rs:53-57,111-115`; `/tmp/modsync-cli-table-audit-20260927/runtime/*profile-list*` |
| profile show JSON | serialized C# Profile; JSON only with flag | Rust `cli_value`, always JSON | **differing** | C# MBC:3895-3915; `src/profile_cli.rs:58-63,111-115`; `src/profiles.rs:43-89` |
| settings list | `{settingsDirectory,settingsFile,values}` | no command | **missing** | C# MBC:3861-3892; `src/main.rs:21-236` |
| settings get/set | `{key,value,settingsFile}` | no command | **missing** | C# MBC:3985-4043; `src/main.rs:21-236` |
| conflicts | no standalone contract | pretty Rust conflict report | **differing** — Rust extension | `src/main.rs:1087-1095`; `src/conflicts.rs:6-27` |
| download batch | no standalone contract | prints full result array before returning aggregate failure | **differing** — JSON may accompany exit 1/stderr | `src/main.rs:870-895`; `src/download_queue.rs:14-35` |

## Shared pipeline wrappers

Rust `validate`, `conflicts`, and `install` do use one dispatch closure for profile/path resolution, selection, direct-Markdown preflight, FOMOD choices, phase preparation, settings and output adapters (`src/main.rs:897-1163`). Ordinary and observed installation converge on the same engine installation body (`src/engine.rs:786-861`). That architectural parity is real, but it does not make the invocation contracts above matched.

C# has distinct handler methods but routes conversion/merge into shared loading, dependency, download and FOMOD services; install invokes full validation by default before its installation service (`C# MBC:1815-2793,3014-3696`). Its `best-effort` path converts missing-source or completed-with-failures results to exit 0 (`C# MBC:3359-3365,3656-3671`). The project binding rule says guide order must stop on failure and never backfill (`AGENTS.md:11-14`); therefore this behavior must remain an explicit unresolved compatibility decision, not be copied by an adapter.

Rust-specific wrapper defects found during this audit:

1. `convert --output named.json` does not use the named output's extension for default format selection; only positional output does (`src/main.rs:780-789`).
2. `--validation-report` parses on all three install-family verbs. In all phase, install then suppresses legacy events without printing a report; conflicts ignores the report meaning and prints its own report (`src/main.rs:908-923,1072-1146`).
3. `validate --install-progress-json` is rejected only after parsing, not by Clap (`src/main.rs:343-351,897-907`).
4. The Windows raw protocol wrapper enters `ProtocolHandoff`, prints parsed JSON and exits rather than opening/forwarding the GUI (`src/protocol_registration.rs:19-37`; `src/main.rs:403-415`).
5. `profile --json` is accepted but deliberately discarded because JSON is unconditional (`src/profile_cli.rs:39-40,111-115`).

Pinned C# defects/boundaries that are evidence, not parity targets:

1. `convert --merge` loses several parsed flags at its internal adapter boundary (`C# MBC:1905-1943`).
2. `set-nexus-api-key` logs the first up-to-ten secret characters (`C# MBC:4070-4079`); Rust must not copy this.
3. `best-effort` and continuation flags permit partial installation and exit 0 (`C# MBC:3359-3365,3656-3671`), conflicting with the binding fail-stop rule (`AGENTS.md:11-14`; `docs/parity/ACCEPTANCE.md:75`).
4. GUI preload accepts only `key=value`, trims quotes after option recognition, ignores unknowns, and overwrites repeated values (`C# GUI args:59-114`).

## Probe receipt

No build or test ran. The container could not execute either artifact: `dotnet` was absent and the Rust binary required host GLIBC 2.43. The same immutable files were then invoked through the documented `host-spawn` boundary. C# DLL SHA-256 is `89d9b2361aee4364f95151b41db59a4133122a6ad41bc2703e232cf10a4f5a6b`, matching `docs/parity/validation-reference/receipt.json:13-18`; Rust binary SHA-256 is `daea394f52f47e2d3ad2b8477d0263f9f34af09da197ea211417b704c50df405` (binary provenance is not asserted by the hash alone).

Evidence root: `/tmp/modsync-cli-table-audit-20260927/`; checksums: `/tmp/modsync-cli-table-audit-20260927/SHA256SUMS`.

Observed gates:

| Probe | C# | Rust | Evidence |
|---|---|---|---|
| root and per-verb help | useful stdout, exit 1 | useful stdout, exit 0 | `csharp/*.stdout`, `csharp/*.exit`, `rust/*.stdout`, `rust/*.exit` |
| unknown `bogus` | parser help/error on stdout, exit 1 | protocol-parser error on stderr, exit 1 | `{csharp,rust}/bogus.*` |
| version | version plus logger line, exit 1 | `modsync 0.1.0`, exit 0 | `runtime/*version*` |
| isolated profile list JSON | logger line then `[]`, exit 0 | `[]`, exit 0 | `runtime/*profile-list*` |
| missing-input structured validation | same key/type envelope, distinct error text, stdout only, exit 1 | same | `runtime/*validate-missing*` |

The probes used only isolated directories below `/tmp/modsync-cli-table-audit-20260927/fixtures`; no game/mod tree or external patcher/program was invoked. Help, version, profile list, and missing-input validation do not establish successful conversion, validation, download, install, GUI, or full-build equivalence (`src/main.rs:401-1163`; C# MBC:748-787).

## Claim blocks

claim: The Rust CLI does not yet have full C# F26 verb/flag parity.
referent: The verb and per-flag tables above; C# declarations at MBC:364-746 versus Rust declarations at `src/main.rs:12-364` and `src/profile_cli.rs:7-41`.
checked: Pinned-source enumeration plus host help probes under `/tmp/modsync-cli-table-audit-20260927/`.
verdict: holds
confidence: high; only a different F26 definition that excludes declared C# verbs/options would change it.

claim: Rust validate/install share a real library pipeline, but shared calls alone do not prove CLI contract parity.
referent: `src/main.rs:897-1163`, `src/engine.rs:786-861`, and the differing validate/install rows above.
checked: Static dispatch trace and missing-input JSON probe.
verdict: holds
confidence: high; a verb-by-verb successful differential corpus could narrow individual differences, not the current missing flags.

claim: C# and Rust early structured-validation error envelopes match structurally.
referent: `/tmp/modsync-cli-table-audit-20260927/runtime/{csharp,rust}-validate-missing.stdout` and formatter definitions at `ValidationPipelineJsonFormatter.cs:22-179` / `src/validation.rs:107-145`.
checked: Both prebuilt binaries against the same nonexistent input under the disposable fixture.
verdict: holds
confidence: high for this early-error case; success, staged failure, warning, and dry-run reports remain unproven.

claim: The C# help/version and profile JSON streams should be copied byte-for-byte.
referent: C# help/version exit 1 and logger-prefixed profile JSON in `/tmp/modsync-cli-table-audit-20260927/`.
checked: Direct immutable-binary probes.
verdict: does not hold
confidence: high; they are observed reference behaviors, but copying erroneous exits or contaminating machine JSON would reduce contract quality and is not required by feature parity.

claim: C# best-effort installation can be added as an ordinary compatibility alias without changing safety semantics.
referent: C# MBC:3359-3365,3656-3671 versus `AGENTS.md:11-14` and `docs/parity/ACCEPTANCE.md:75`.
checked: Static control-flow comparison.
verdict: does not hold
confidence: high; only an explicit user ruling that overrides the fail-stop constraint would change it.
