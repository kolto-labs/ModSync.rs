# CLI phases evidence archive

Implementation/source bindings and artifact hashes are in `manifest.json`.
Log files preserve captured commands and output content with trailing whitespace
removed and EOF blank lines reduced to one final newline. They are normalized
copies, not byte-exact originals. Source, binary, guide, manifest, and receipt hash
fields are unchanged. Original raw logs and task review diffs remain in
`.superpowers/sdd/cli-phases-plan/` through final review. Git history provides the
exact reviewed source diffs; they are not duplicated in this durable archive.
Review/report status wording is historical; see `../cli-phases-integration.md` for
the later independent PASS decisions and current whole-package review status.
