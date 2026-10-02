# Unicode selection matching source correction

Pinned reference `ModBuildConverter.cs:1388–1472` uses OrdinalIgnoreCase for
category/tier selectors. `ModSelectionPage.axaml.cs:773–791` uses the same
comparison for category, tier and displayed name/description search.

The shared Rust selection library now uses the existing simple Unicode case
mapping from `fomod::fold_case` / `eq_ignore_case` for authored component names,
custom category/tier equality, and displayed search text. Both legacy CLI
selection and the current transaction dispatcher use the same comparison.
GUID lookup remains ASCII case-insensitive; numeric/known tier ordering and
spoiler-safe displayed text selection remain unchanged.

This corrects ASCII-only equality and full lowercase substring comparison in
these paths. The existing mapping deliberately avoids multi-character full-case
expansion; Unicode/runtime-version edge equivalence remains an acceptance item.
No tests, harnesses, CLI runtime or native UI were run for this correction.
