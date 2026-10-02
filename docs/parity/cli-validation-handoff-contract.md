# CLI validation flags and bare protocol handoff

## Source boundary

This slice fixes two retained CLI adapter gaps identified in the
[CLI table audit](cli-table-audit.md) and [implementation plan](cli-table-plan.md).
It adds source implementation and independent specification/quality review;
invocation, desktop, transport, and platform acceptance remain separate.

## Validation-only flags

The shared install-family dispatch refuses `--validation-report`, `--output`,
`--errors-only`, `--full`, `--dry-run`, and `--dry-run-only` on `install` and
`conflicts`, in every phase. A single predicate gates these fields before profile,
guide, path, phase, acquisition, or engine preparation. Parser-owned conflicts
and required arguments may still fail before dispatch. Existing CLI session
logging and cancellation registration occur before this guard.

The former All-phase `install --validation-report` combination suppressed events
without producing a validation report. It now returns a diagnostic on stderr and
exit 1 through the normal command failure path. It does not invent a validation
JSON document for an installation or conflict command. Existing validation text,
JSON, event, setup-error and cancellation handling is unchanged. Installation
`--json`/`--install-progress-json` and conflict JSON retain their distinct meaning.

The pinned reference declares report/full/dry-run/error filtering in
`ValidateOptions`, rather than `InstallOptions`:
`ModBuildConverter.cs:570–678` at C# `d31ccbd3bfcddb4705917eafa6483715c166de3b`.
The Rust refusal closes an adapter ambiguity without bypassing existing safety
checks or changing report schema.

## Bare protocol route

Exactly one bare `nxm://` or `modsync://` token, optionally following the existing
leading `-v`, `--verbose`, or `--plaintext` global switches, normalizes into
`gui --protocol=VALUE` before Clap parsing and CLI session initialization. Scheme
recognition is case insensitive. The existing compatibility trim/quote policy
is applied once; that same resulting payload is qualified and forwarded.

The additive pure `single_instance::validate_protocol_request` refuses the
internal activation sentinel and reuses the delivery admission rules: 1–8,192
UTF-8 bytes and the shared protocol parser. The CLI maps all qualification errors
to a generic diagnostic without the URL or underlying parser details. No native
transport, filesystem, DNS, fetching, settings, or desktop work occurs during
qualification. Successful normalization uses one equals-form option so the
payload cannot become another option.

The normal GUI path decides cold launch versus authenticated single-instance
forwarding using the current package channel. A headless build reaches its
existing desktop-disabled error without printing parsed-link JSON or starting a
CLI session. Explicit `protocol-parse` remains the diagnostic JSON command.

The unrestricted external-subcommand branch is removed. A recognized bare
protocol token with additional arguments is refused before Clap with a generic
diagnostic that does not echo the URL (stderr, exit 1). Invalid single-URL
requests also use the redacted command error (stderr, exit 1). Unknown
non-protocol verbs retain ordinary Clap errors (stderr, exit 2). This retains
the intentional distinction from the reference parser's help/error exit
conventions. Successful GUI/forwarding paths retain their own existing exit
behavior.

Existing explicit `gui` and legacy-switch preload handling remain in their prior
paths; bare-first protocol requests use the refusal above. Duplicate-target
refusal, unknown legacy arguments, and the reference's
two independent URL slots are outside this bounded change. Current registered
Windows/Linux commands already invoke `gui --protocol`; they need no edits.
The reference GUI scheme/trim and single-instance behavior are anchored in
`CLIArguments.cs:48–115` and `Program.cs:23–100` at the same pinned C# commit.

## Evidence limit

Only production source was changed. No test source, fixture, harness, runtime,
native API probe, or CI execution belongs to this slice. This is not whole F26
or native desktop acceptance.
