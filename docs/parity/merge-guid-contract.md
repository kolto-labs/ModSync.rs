# Merge GUID values and authored spellings

The shared .NET character parser now supplies known GUID comparison to the merge
adapter. This changes comparisons and explicit merge results, not guide loading,
persisted IDs or existing checkpoint/receipt hashes.

## Identity decisions

When both component GUIDs parse, `same_guid` compares their UUID values. If
either input does not parse, the prior trimmed/braced ASCII comparison remains.
This fallback is deliberately separate from union keys: it can compare
an unsupported spelling with a known spelling, so it is not a universal identity
equivalence relation suitable for a set.

Known empty GUIDs use UUID nil detection. Opaque values retain the prior blank
predicate. Matched option GUID adoption now recognizes known nil forms, keeping
the source's authored spelling when a nonempty source replaces an empty target.
The existing component GUID preference policy is unchanged.

These decisions follow pinned C# `d31ccbd3`
`ComponentMergeService.cs` dictionary lookup at347/425, ordering equality at580,
component empty/preference decisions at603–619 and option empty decisions at866–871.

## Dependency/order unions

With an empty source field, the target is untouched. With a nonempty source,
the adapter deduplicates the target in original order, retaining the first
authored spelling, then appends previously unseen source values in source order.
Known GUIDs use UUID value keys; opaque identifiers use the union's existing
ASCII case-insensitive policy. A discriminated set prevents accidental opaque
versus known key collisions. Ordered output is independent of set sorting.

This follows source field `HashSet<Guid>` construction/addition at753–796 and
option union decisions at896/908. The pinned
[.NET 9 HashSet implementation](https://raw.githubusercontent.com/dotnet/runtime/v9.0.0/src/libraries/System.Private.CoreLib/src/System/Collections/Generic/HashSet.cs)
retains insertion order for this construction/addition path without removal.
Retaining authored spellings is the Rust string-model extension; emitted IDs
are not silently canonicalized. Work uses one parser call per union entry and
ordered-set insertion rather than repeatedly reparsing all prior entries.

## Evidence boundary

The character grammar remains defined in the profile GUID parsing contract.
Guide GUID repair, required nonzero intake, strict identity classification and
explicit identity migration remain separate. Native/merge-corpus/full F20
acceptance is not established by source review or production compilation.
No tests, harnesses, filesystem probes or runtime calls belong to this slice.
