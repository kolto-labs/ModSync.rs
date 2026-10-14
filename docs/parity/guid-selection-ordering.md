# GUID value comparison in selection and ordering

This production source slice uses the shared .NET 9 character GUID parser for
selection and dependency ordering. A private ephemeral identity contains either
a parsed UUID value or an ASCII-folded opaque legacy spelling. It does not rewrite
stored guide IDs, selected ID receipts, deployment/checkpoint identities or output
strings. Required GUID admission and nil policy remain at their existing boundaries.

Component/option lookup, dependency/restriction relationships, CLI GUID selectors,
selection diff assignments and restriction-pair deduplication use the same value
comparison. Component-name selector precedence and selection cascade order remain
unchanged. Legacy opaque identities keep ASCII comparison; this is separate from
the model's instruction equivalence rule, whose opaque fallback remains exact.

Selection traversal records the direction of a visit separately from the identity.
A synthetic opaque identity beginning with a dash cannot collide with the old
unchecked-visit string marker. Components and options retain separate visit sets.

`Guide::ordered` uses ephemeral identities for its top-level dictionary, selected
set, predecessors and completed nodes. It rejects duplicate canonical top-level
identities before building the graph, matching the pinned reference's typed GUID
dictionary requirement. Missing-reference errors retain the authored reference
spelling; stable guide order still chooses among currently runnable components.
The instruction-equivalence/Choose deduplication GUID helper now uses the same
character parser, including reference numeric and whitespace forms.

Primary reference: pinned `ComponentSelectionService.cs` typed GUID relationship
lookups and `DependencyResolverService.cs:58` top-level GUID dictionary. The parser
contract remains documented separately in `guid.rs`.

Engine selected/completed comparisons are covered in the continuation below.
Other consumers keep their separate reconciliation scope. This slice does not
expand opaque legacy IDs into valid GUIDs, and does not close F04/F10. Source
review, production compilation and native/behavioral acceptance remain separate.

## Execution and validation continuation

The engine now uses typed GUID identity for private selected/completed sets,
checkpoint resume-plan projection, the checkpoint coordinator's selected set, verified-prefix comparison, remaining filters,
component and instruction conditions, immediate Choose option lookup and current
conflict attribution. Choose option lookup has since moved to the C# text rule
(`Instruction.GetChosenOptions`): see [choose-option-match-contract.md](choose-option-match-contract.md). Original checkpoint definitions and recovery receipts are
not rewritten; their hash/provenance admission remains unchanged. Only admitted
resume definitions are projected into the engine's private membership keys.
Declared-conflict and basic-validation lookup use the same comparison, while the
public selected-ID and report APIs keep their existing string shapes. Conflict
presentation and adjacent writer collapse compare GUID values without rewriting
report IDs. Filesystem path casing is a separate contract and remains unchanged.

No engine run, resume, validation runtime or checkpoint mutation has established
acceptance for these source changes yet.
