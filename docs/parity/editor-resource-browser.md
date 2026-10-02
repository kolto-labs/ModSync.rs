# F21 resource tree and multiple-selection adapter

The native guide editor now projects the shared registered-resource inventory into
an expandable directory tree. File checkboxes retain multiple selections across
expanded archive folders and registered resources. Directory nodes group known
archive members; they do not invent extra filesystem files or infer extraction
locations. Registered inventory errors remain visible and disable those entries.

Projecting archive members requires an explicit absolute extraction directory.
Every chosen path goes through the existing shared `editor::variable_path`
validation on the owned background worker. The browser does not extract archives,
create folders, choose an extraction destination, or mutate either install root.
Native source browsing supports multiple file selection; folder browsing remains
one explicit source/destination folder.

The user reviews all projected variable paths before confirming them into the
separate instruction draft. Multiple sources append in deterministic order with
exact duplicate removal; a destination requires exactly one projected path.
Only the existing Apply component/option draft action submits the shared validated
transaction to change the guide. Failed projection publishes no partial selection.
Guide/root context changes, component changes, extraction-directory edits, and
resource selection changes invalidate projected values. Starting a refresh or
projection clears prior values so a failed replacement cannot masquerade as the
new result.

The presentation projection is bounded to 20000 archive members, 100000 tree nodes,
64 path segments and 4 MiB of member text. Selection and resulting draft source
lists are capped at 4096 entries; projected text is capped at 4 MiB. Overflows fail
explicitly without silently truncating a candidate. Tree construction and path
projection honor cancellation; inventory uses the existing shared archive budget.

This records source implementation only. No tests, native file pickers, archive
probes, builds or runtime validation were executed for this slice. Keyboard/focus,
platform picker and native rendering acceptance remain separate.
