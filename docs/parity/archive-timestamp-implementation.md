# Archive modification-time contract

Source implementation 2026-10-02; integrated source review and production
build/strict-Clippy checks in both feature modes passed at the T3 boundary.
Runtime corpus and native filesystem
acceptance are deferred by the user's testing policy. The table below records
decoder units and conversion rules from the locked library sources, not measured
cross-language output.

The pinned C# real filesystem extractor reads `IArchiveEntry.LastModifiedTime`
and calls `File.SetLastWriteTime` after copying file contents. Explicit directory
entries are skipped. Rust's archive entries now carry an optional modification
time separately from their bytes and permissions. Engine scratch content carries
that value through virtual Copy/Move/Rename. Real publication captures source
modification time, performs the existing atomic byte copy, then applies the time
to the actual output path before readonly permissions, including managed staging.

| Format | Source | Conversion and absence |
| --- | --- | --- |
| ZIP NTFS extra | `zip 2.4.2` `ExtraField::Ntfs::mtime` | UTC FILETIME, 100 ns since 1601; preferred over other ZIP clocks |
| ZIP extended timestamp | `ExtraField::ExtendedTimestamp::mod_time` | Unsigned Unix seconds; preferred over DOS when no NTFS field exists |
| ZIP DOS fallback | `ZipFile::last_modified` | Local wall clock with 2-second resolution; later instant for ambiguous fall-back time, explicit error for nonexistent local time; decoder absence remains absent |
| 7z and embedded 7z SFX | `sevenz-rust2 0.22.2` presence flag and `NtTime` | UTC FILETIME in 100 ns units; no timestamp when the presence flag is false |
| TAR and tar.gz member | `tar 0.4.46` header `mtime` | Unix seconds; member time wins over gzip-container time |
| Plain GZip | `flate2 1.1.10` first parsed header | Unix seconds; zero means absent; concatenated members produce one output with the first header's time |
| RAR | `unrar 0.5.8` extraction result | Preserve decoder-written temporary-file modification time when header `file_time` is nonzero; no separately invented timezone conversion |

FILETIME/Unix conversions use checked `SystemTime` arithmetic. Timezone-less DOS
data is not silently labeled UTC. The explicit DST ambiguity rule is a Rust
policy requiring comparison with the pinned reference; it is not a claim of
identical historical timezone behavior. Unsupported or unrepresentable times
remain actionable errors rather than fabricated timestamps. RAR decoder timestamp
behavior and native filesystem rounding require runtime evidence.

The metadata field does not authorize writing during virtual validation.
Timestamp-setting failures during real publication must follow the same pending
recovery and stop-on-error path as byte/permission failures. Archive integrity,
path containment, streaming limits and decoder selection remain unchanged. No
empty-directory, encrypted/multipart or executable-SFX support is implied.

Remaining acceptance includes known UTC timestamps, ZIP extension precedence and
DOS/DST cases, absent times, RAR output, multi-member gzip, nested extraction,
follow-up Copy/Move/Rename, managed staging, native precision, and failures during
publication. No filesystem tests, fixtures or runtime probes were created or run.
Checkpoint/legacy restore timestamp retention requires separate reconciliation;
this slice does not establish restoration of filesystem metadata beyond the
checkpoint schema's existing mode contract.
