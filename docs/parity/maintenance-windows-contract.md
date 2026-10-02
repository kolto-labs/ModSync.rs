# Windows directory maintenance source contract

This document describes the production source implementation of
`windows-owner-ntfs-process-interruption-v1`. Windows GNU production library/CLI cross-compilation and linking passed;
execution, interruption recovery and external process acceptance remain
unverified. There is no OS crash or power-loss guarantee. The existing
[maintenance transaction contract](maintenance-contract.md) supplies review,
root locks, immutable intent, rollback, cancellation, fences and explicit recovery.

## Admission and authority

The Windows profile qualifies canonical local fixed NTFS paths through the
existing checkpoint persistence qualifier. UNC, device/provider paths, other
filesystems, missing or linked ancestors, reparse points, pending deletion,
special objects and multiply linked files are refused.

Every captured native object includes the complete volume serial and 128-bit
file ID from `GetFileInformationByHandleEx(FileIdInfo)`. The complete pair is
compared; the older device/inode fields are deterministic projections used for
existing intermediate-name derivation, not substitutes for native identity.
[Microsoft FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)

The effective thread token is used when present, otherwise the process token.
Its user SID must equal the object's owner SID. A duplicated impersonation token
is used for access qualification; no privileges are enabled. Existing DACLs must
already grant readable files and usable directories, file attribute repair and
the specific rename rights when applicable. Failed access checks, reported
privileges used, foreign ownership, and unsupported object/conditional/custom
ACE types are explicit setup errors. Only ordinary allow/deny DACL entries are
admitted. No ownership or tree DACL repair is attempted.
[Microsoft AccessCheck](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-accesscheck)

`GetSecurityInfo` captures owner/group SIDs, DACL entries and relevant owner/group
defaulting and DACL control state from the pinned handle. A bounded deterministic
SHA-256 fingerprint binds that state before and after mutation. Only the owner
SID and fingerprint are serialized; raw DACLs and token data are not included in
product output. SACL inspection, audit state equivalence and privilege acquisition
are outside this profile. Concurrent external editing remains unsupported.
[Microsoft GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo)

Native `FileStreamInfo` queries use an 8-byte-aligned fixed 64 KiB buffer. Files
must have exactly the ordinary unnamed `::$DATA` stream of the observed file
length; directories must have no data stream or an empty unnamed stream. Named
streams, multiple streams, oversized/malformed replies and unsupported query
results are refused. This admission applies to roots, inventory objects and
private metadata, including mutation readback. The inventory continues to hash
only admitted default file data.
[Microsoft FILE_STREAM_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_stream_info)

## Preserved attributes and permission repair

Only readonly, directory, hidden, system, archive and not-content-indexed native
attributes are admitted. Compression, encryption, sparse/offline/cloud/reparse,
temporary storage and unknown attribute profiles are refused. `NORMAL` is
normalized and is used alone when the resulting file has no other attributes.

Windows inventory `mode` is `0` or `1`, representing readonly separately from
immutable attributes. `OwnerWritable` clears readonly on files only. Directory
readonly is preserved because it does not control directory write access;
already usable directory DACLs are required. A denying DACL is not repaired.
[Microsoft file attribute contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileattributesw)

The setter uses a pinned `FILE_WRITE_ATTRIBUTES` handle and
`SetFileInformationByHandle(FileBasicInfo)`, retains every admitted immutable
flag, leaves timestamp fields unchanged, flushes through a data-writable handle
and verifies native identity/security/attributes and readonly state afterward.
Rollback opens its data-writable flush handle before restoring readonly; it does
not attempt to reopen data-write access after readonly has been set.
[Microsoft FILE_BASIC_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_basic_info),
[Microsoft FileBasicInformation timestamp semantics](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fscc/16023025-8a78-492f-8b96-c873b042ac50)

Readonly files that actually require a names change are refused during
`LowercaseNames` preview. The diagnostic directs the operator to run
`OwnerWritable` and create a new lowercase preview. A names transaction never
silently clears readonly and never uses an ignore-readonly rename flag.

## Exclusive rename and private publication

Both rename legs use `SetFileInformationByHandle(FileRenameInfo)` on a pinned
source with DELETE access and a pinned directory handle as `RootDirectory`.
Parent handles request list/add-file/add-directory/traverse rights consistent
with the unchanged qualified DACL. Relative destination components are bounded
UTF-16 names. `ReplaceIfExists` is false for tree operations and new metadata
publication. No copy, merge or overwrite fallback exists.
[Microsoft FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info),
[Microsoft SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)

Private intent, fence and journal files are created with native `CREATE_NEW` and
a protected DACL granting only the current owner full file access, supplied at
creation. This changes security only on newly owned metadata files. Archive is
set initially so normal journal writes cannot change its recorded immutable
archive state. Temporary writers are flushed and closed before publication;
native source identity/security is checked again. Publication must remain on the
same qualified local NTFS volume. Replacement is limited to verified core-owned
terminal metadata; the tree rename path always remains exclusive.

Existing Windows journals are opened with full read/write access so interrupted
tail truncation can succeed. Since that overrides Rust's append-only access mask,
each append explicitly seeks to EOF after identity/length/mtime verification and
requires the observed offset to equal the recorded length. Flush and close
ordering retains the existing durable journal/fence protocol.

## Serialization compatibility and proof boundary

`Identity.windows` is optional and omitted on Unix. Windows records contain the
full native ID, owner SID, security fingerprint and immutable attributes;
readers validate their shape and their consistency with entry type and mode.
Linux omission preserves legacy serialized identity bytes and digest input.
The frozen original Linux capability remains an exact allowlist. Stable native
profile IDs and independently validated optional casing descriptors govern new
writers, so presentation text changes do not invalidate profiled intents.

The shared pinned Unicode casing contract is described separately in
[maintenance Unicode admission](maintenance-unicode-contract.md). Native filesystem
collision checks and exclusive rename still govern publication; a mapped name
never authorizes replacing another object.

The profile states process-interruption source design and flushed file data.
Windows namespace boundary checks do not claim directory fsync. Source review
does not establish live platform behavior, interruption recovery, OS crash
recovery or power-loss durability. Cross-compilation/link evidence is recorded
separately in [the production gate receipt](production-gates-2026-10-02.md).
