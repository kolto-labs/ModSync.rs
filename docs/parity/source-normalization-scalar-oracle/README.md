# Authored scalar Source reference oracle

Sixteen production C# conversion calls against pinned `d31ccbd3` DLL (hash in receipt),
with directly authored JSON/TOML/YAML/XML inputs. No intermediate Rust conversion
changed scalar shape. Each call ran in a read-only-host bubblewrap sandbox with
network isolated and only disposable fixture/HOME/XDG/TMPDIR writable.

Relative scalar paths normalize in all four formats. Empty and whitespace scalars
fail conversion (exit1) in all four. Numeric42 in JSON/TOML/YAML is accepted but
outputs an empty Source array; XML's untyped text42 is a different shape and becomes
a mods-relative path. See projection.json for exact results. Rust's approved
normalization plan deliberately preserves existing malformed-value errors rather
than copying this silent loss. This is conversion evidence only, not installation
validation, a Rust fix, or whole-format parity.

Raw inputs, exact argv/exits/stdout/stderr/output objects and driver are retained.

**Rust follow-up (after `3911e74`).** The Rust XML reader used to turn the empty and
whitespace XML scalars into an empty list, and the Markdown metadata reader did the same
for `Source = ""` / `Source: ''`. Both now keep the scalar as `[""]`, which Rust refuses
in the dry-run report, `validate` and `install`, as it already did for JSON/TOML/YAML.
A live rerun of this method on XML and Markdown fixtures, with the C# Markdown result
(the metadata block is dropped, not refused), is in
[scalar-empty-source-proof](../scalar-empty-source-proof/README.md); the contract is
[scalar-empty-source-contract.md](../scalar-empty-source-contract.md). The `probe.py`
here is the original driver with its original absolute paths and is kept as recorded;
only this README line in `SHA256SUMS` was refreshed for the note.
