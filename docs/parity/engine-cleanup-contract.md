# Engine cleanup source contract

This slice implements `DelDuplicate` and `CleanList` against the pinned C#
`Instruction.cs` and `ModComponent.cs` at
`d31ccbd3bfcddb4705917eafa6483715c166de3b`. It adds bounded, cancellable planning
before deletion. Source review and production compilation are separate evidence;
this document does not claim runtime, native filesystem, or full parity acceptance.

## DelDuplicate

`Instruction.DeleteDuplicateFile` (lines 556–667) counts compatible extensions
by filename stem, then examines **all** immediate files in the destination.
`ModComponent.cs:1594` always requests case-insensitive comparison, independently
of the configured filesystem path policy. Rust uses its retained .NET 9 ordinal
keys for both stems and extension comparison.

Blank Source entries are ignored using the pinned .NET whitespace set. Nonblank
entries retain their authored whitespace. An empty resulting list uses `.dds`,
`.tpc`, `.tga`. The requested deletion extension is Arguments, including an empty
string. A candidate is deleted when its nonempty stem has more than one compatible
file and its extension equals Arguments. The requested extension need not appear
in the compatible list. **Empty Arguments can select extensionless candidates**;
there is no empty-argument early return. Counts are not decremented during planning.

The qualified basename helper follows last-dot extension/stem semantics: a
trailing dot has an empty extension; a leading-dot-only basename has an empty
stem and cannot be deleted. This is not a claim to implement arbitrary
platform-specific path parsing. Existing root/path admission still governs names.
Non-Unicode candidate filenames are explicitly refused rather than compared
through lossy filename conversion.

Destination existence is checked first. Candidate scanning, copied keys, planning
and publication honor the shared cancellation token and work bound. There are at
most 100,000 immediate candidates. Every planned target is root-qualified before
any deletion, and execution checks cancellation before each existing delete call.

## CleanList matching and planning

The first matched source is used, preserving existing source resolution.
`ModComponent.cs:1673–1778` supplies these selection rules:

1. Blank names fail; `Mandatory Deletions` is an ordinal-insensitive prefix.
2. Tokenization uses invariant simple lowercase, then ASCII letters/digits,
   retained stop words and the reference's final-`s` singularization.
3. An empty target token set fails **before** exact/substring quick checks.
4. Ordinal-insensitive exact/substring matching checks selected names.
5. Otherwise require at least two shared tokens and 60% of the smaller token set.

Selected names retain original spelling in all ordinary, dry-run and checkpoint
engine captures. The shared pinned Unicode helpers provide simple lowercase and
ordinal keys, without Rust full lowercase expansions or context-sensitive casing.
Prepared selected keys/tokens and repeated comparison work are admitted under the
same bound as rows and paths. Name/path trimming and blank detection use the
explicit .NET 9 `Char.IsWhiteSpace` set, including NEL and NBSP, without NUL trimming.

Rows split on CR or LF. Blank rows are skipped; comma-separated fields retain the
reference's unquoted CSV behavior. Nonblank rows without a comma produce a generic
warning, without reproducing the row. Validation receives a warning issue and
installation emits a warning event. Empty filename fields are skipped. Missing
files remain harmless. Quoting, escaping and alternate CSV grammars are not added.

Decode and qualify the complete deletion plan before mutation. Retain the existing
relative-path parser, virtual-file lookup and root checks. Deduplicate selected
physical targets; at most 100,000 deletions are planned. Cancellation, I/O, unsafe
paths or admission failures abort before deletion; a later execution failure uses
the engine's existing stop/recovery behavior. C# may continue after deletion
failure; this slice retains Rust's guide-order failure policy.

## ReadAllText encoding profile

The pinned provider calls `File.ReadAllText`, which defaults to nonthrowing UTF-8
and uses BOM-aware `StreamReader.ReadToEnd`. Neither the reference nor this slice
uses a 4 MiB reference file limit. [File.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/IO/File.cs),
[StreamReader.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/IO/StreamReader.cs).

The pure cleanup decoder accepts default UTF-8 and one leading UTF-8, UTF-16LE,
UTF-16BE, UTF-32LE or UTF-32BE BOM. Four initial bytes distinguish UTF-16LE from
UTF-32LE. A later BOM remains ordinary text; unrecognized/partial BOMs follow the
selected default decoder.

UTF-8 replaces each maximal ill-formed subsequence with U+FFFD, including one
replacement for an incomplete final valid prefix. UTF-16 replaces each unpaired
surrogate and an odd final byte separately: a pending high surrogate plus an odd
byte produces two replacements. UTF-32 replaces an invalid scalar or surrogate
word with one replacement, and a final one-to-three-byte partial word with one
replacement. Pending state is flushed before the final row. Authored U+FFFD is
preserved. The guide-specific replacement-to-underscore policy is never used.
[UTF8Encoding.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Text/UTF8Encoding.cs),
[Rune.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Text/Rune.cs),
[UnicodeEncoding.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Text/UnicodeEncoding.cs),
[UTF32Encoding.cs](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Text/UTF32Encoding.cs).

## Explicit Rust admission policy

These are independent implementation limits, not limits claimed for C#:

- 8 MiB raw input, with 8 KiB read buffers and bounded decoder carry.
- 8 MiB decoded UTF-8 bytes, including replacement expansion and delimiters.
- 256 KiB per decoded line or individual preprocessing field.
- 100,000 CR/LF row boundaries/final records; CRLF consumes two boundaries.
- 2 million cumulative work units: decoder steps, preprocessing text bytes,
  token visits, repeated comparisons, inventory visits and planned-path work.

Cancellation is checked around each input read and throughout decoding, matching,
planning and deletion. Generic reader errors propagate before mutation. The work
limit can reject an input below a byte/row limit; satisfying one limit does not
bypass the others. The bounded whole-file row/target plan protects against partial
cleanup caused by late malformed or excessive input.
