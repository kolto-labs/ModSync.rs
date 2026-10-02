# Root-aware external FOMOD choices

## Implemented source contract

`phase::prepare_with_roots` and `ChoicesFile::apply_to_remaining_with_roots`
add read-only physical ownership resolution for external choices. Existing pure
`phase::prepare` and rootless choice APIs keep their contracts. Rooted adapters
must supply canonical mod/game roots, the selected case policy, and their actual
cancellation token. No roots are fabricated for basic/rootless validation.

A source folder named `bundle.zip` may contain `inner.zip`. Its suffix does not
make it an archive file: the new resolver inventories ordinary paths and applies
the engine's PathPolicy glob matching plus literal-directory-prefix matching.
This also covers root-folder sources and nested descendants. A nonexistent
literal `inner.zip` source retains explicit ownership; ordinary non-archive-named folder/glob sources retain their prior lexical
potential ownership, even when absent or currently containing no matching file.
Earlier instructions may create a matching archive in the engine's virtual tree;
preparation cannot rule that out from the current disk inventory. A missing
archive-suffixed folder retains old exact-name behavior: future generated
archive-suffixed folders require embedded choices or registry identity evidence. Registry bindings remain authoritative, whether
or not the registered download exists. Two different physical files sharing a
choices basename within one component's Extract scope are rejected, because the
choices format cannot distinguish them. Rootless APIs retain their prior lexical
matching and cannot resolve this folder identity distinction.

The inventory rejects symlinks, special files, Windows reparse points, non-UTF-8
identity and case aliases under the configured matching policy. It excludes the
same private peer staging directories as the engine. It limits entries to
100,000, depth to 64, individual paths to 16 KiB, aggregate path text to 32 MiB,
and matching visits to 20 million. Cancellation is checked during scanning,
matching and before candidate publication. The walk and metadata inspection do
not read archive contents or create/modify mod or game paths. Concurrent external
filesystem changes are not locked by preparation; the execution gate still
inspects the current archive and validates current choices before installation.

Empty choice files and guides with no eligible top-level component return after
checking cancellation, without inventorying either filesystem root.

Attachment is a global authored external-choices metadata overlay, as in the
existing rootless API: it can update matching unselected components/options.
It does not authorize downloads or installation. Captured selected post-download
consumers use the separate `apply_bound_archive` authorization contract.

Attachment retains the existing component and nested-option ownership rules.
A retained completed component skips its entire nested subtree. Changes merge
per archive, preserving all other saved entries, and malformed relevant saved
choices remain errors. A separate candidate is published only after every
attachment succeeds. Reconstructed remaining-phase definitions retain the
existing receipt-verification contract; preparation does not certify completion.

## Missing registered archives

The engine FOMOD gate now emits a bounded filename-only warning when a registered
archive is absent and no prior prompt state exists. It does not include provider
URLs or resource metadata. Recorded prompt state still makes absence a hard
error. This follows pinned `FomodConfigurationGate.cs` lines 119–139. Generic
archive availability validation remains independent.

## Separate differences and verification boundary

Hard-dependency handling is unchanged: Rust selected ordering rejects an
unselected required dependency before this gate; the pinned gate expands hard
dependencies for inspection. This change does not silently select dependencies.
C# configured markers without usable Rust choices still require explicit
reconfiguration; generated C# instructions are not imported as choices.

This receipt describes implementation only. No tests, harnesses, archive probes,
runtime checks, builds or CI were run for this slice. Adapter wiring and
independent source review must be recorded separately by the owning agents.

Desktop Save Selection uses this rooted overlay in its owned worker when both
roots are supplied; missing roots retain rootless behavior, while supplied invalid
roots fail explicitly. It captures the current path policy and cancellation token.
