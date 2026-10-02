# Windows checkpoint implementation boundary

Read-only source audit on 2026-10-02, followed by a narrow recovery lock intake
correction. No native execution, filesystem tests, or Windows build was performed.

Checkpoint writing still refuses non-Unix platforms in `store::writer_supported`.
This is an implementation gap, not merely missing native acceptance. Do not remove
that gate or return success from a directory-sync stub. The current journal
protocol relies on flushed files and persistent directory creation, publication,
replacement and deletion ordering. The Windows tempfile implementation alone does
not establish that contract.

Remaining source work:

- A deliberate Windows persistence capability and publication protocol, including
  directory mutations and journal deletion. Receipts currently name Unix
  file-and-directory-fsync and must describe the actual platform capability.
- Restore preparation for owned read-only destination files, followed by the
  captured attributes; ACL failures remain errors.
- Legacy snapshot flushing through writable handles before restoring read-only
  attributes. The current read-only reopen cannot satisfy Windows flush access.
- Native interruption, sharing violation, reparse point and filesystem acceptance
  after the implementation is complete. No ACL/owner/alternate-stream equivalence
  follows from the existing portable mode schema.

The narrow recovery lock correction adds `FILE_FLAG_OPEN_REPARSE_POINT` on Windows
and checks the opened handle's reparse attribute before taking the exclusive lock.
The recovery directory is also required to be an ordinary directory, rejecting
Windows reparse attributes. Existing Unix no-follow/nonblocking lock flags remain.
This fixes leaf-lock admission; it does not turn path-based operations into a
transaction against an adversary replacing ancestors, and does not enable Windows
checkpoint writes or claim platform durability.

Official contracts consulted by the source reviewer:

- [Directory handles](https://learn.microsoft.com/en-us/windows/win32/fileio/obtaining-a-handle-to-a-directory) require backup semantics; this alone is not a directory persistence guarantee.
- [FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers) requires write access; whole-volume flush has different privilege requirements.
- [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) write-through does not by itself cover separate journal deletions and directory creation.
- [ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) documents its write-through flag as unsupported.
