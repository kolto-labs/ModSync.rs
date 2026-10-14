# External action source and argument implementation

Source implementation dated 2026-10-02; behavioral acceptance remains deferred.

Execute and Run resolve every authored Source through the existing contained VFS
matching logic before spawning any child. Authored source order is preserved;
wildcards use stable matching order; overlapping matches run once. Missing or
unsafe later sources reject the whole instruction before its first subprocess.
Cancellation/failure stops before any later program. Patcher retains exactly one
Source so its selected configuration, staging and expected log root remain bound
to one invocation.

Arguments are tokenized before root substitution with CRT-style
double-quote/backslash rules and literal single quotes on every host (until
2026-10-04 Unix used shlex; see
[execute-arguments-contract.md](execute-arguments-contract.md)). No shell is invoked. Supported placeholders are
modDirectory, kotorDirectory and gameDirectory, case-insensitively. Each resulting
argument remains one OS string even when a root has spaces or non-UTF-8 bytes.
Unknown/unclosed markers, malformed quote text and NUL input fail before spawn.
External programs still require explicit allow-processes trust; they are not
sandboxed by this argument expansion.

Reference source: pinned C# Instruction.ExecuteProgramAsync resolves source arrays
and applies UtilityHelper.ReplaceCustomVariables to Arguments before execution.
This closes the source-derived single-source and missing-placeholder gaps. It does
not claim a measured cross-platform quoting corpus, child-process-tree termination,
or complete instruction/path parity. Existing child-only cancellation remains a
separate controlled process-tree acceptance item. No test/harness/fixture/runtime
execution or CI was performed for this slice.
