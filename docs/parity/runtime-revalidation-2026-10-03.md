# Runtime revalidation of `main`, 2026-10-03

Source work between 2026-09-28 and 2026-10-02 landed without any runtime execution
(the user had deferred testing until implementation was complete). This record
re-runs the retained production CLI proofs against `main` `73e8819`, corrects what
they exposed, and adds a fresh C#/Rust differential on the public guide pin. No Rust
tests or custom Rust harnesses were created, changed or run (AGENTS.md freeze).

Environment: Linux x86_64 cloud container, Rust 1.95.0, every network fixture inside
`unshare -Urn` (loopback only, no routes). C# reference `d31ccbd3` built with the
apt .NET 10 SDK (`global.json` roll-forward relaxed locally to `latestMajor`, run with
`DOTNET_ROLL_FORWARD=Major`). Guides: mod-builds `336b36d9`, K1/K2 `full.md` hashes
matching [the cloud bootstrap](cloud/README.md).

## Retained proof suites

| Suite | `main` before fixes | After fixes and expectation updates |
| --- | --- | --- |
| `fomod-s1-proof/probe.py` | 1 fail | 339/339 |
| `fomod-s1-proof/inherited_controller.py` | 8 fail | 41/41 |
| `fomod-s2a-proof/probe.py` | 14 fail | 224/224 |
| `fomod-s2b-proof/probe.py` | crash | 254/254 |
| `nexus-h1a-proof/probe.py` | 4 fail | 138/138 |
| `fetch-shared-url-proof/probe.py` | 35 fail | 382/382 |
| `deployment-csharp-differential.sh` (C# DLL + Rust) | not run | `TOTAL fail=0` |

Every failure was classified by reading source and per-request fixture logs.

**Regressions, fixed in source:**

1. **OpenSSL instead of rustls.** `f68dcfc` enabled the MEGA crate's `reqwest`
   feature, which turns on reqwest `default-tls`; reqwest then picks native TLS for
   every `ClientBuilder`. Confirmed at runtime: a wrong extra CA produced OpenSSL's
   `certificate verify failed` before the fix and rustls' `invalid peer certificate`
   after it. All nine builders now call `use_rustls_tls()`.
2. **camelCase prompt status.** `fomod.rs` `write_prompt_status` created
   `handlerMetadata`, while loading canonicalizes to C#'s `HandlerMetadata`. A failed
   or no-op `--record` run therefore still rewrote the guide. It now writes the
   canonical key unless an entry has only the authored camelCase key.
3. **FOMOD choices diagnostics.** `f68dcfc` collapsed `Reading`/`Parsing FOMOD
   choices` into `Loading FOMOD choices`. Restored in `ChoicesFile::load`, so
   validate/install gain the same context.
4. **Rust YAML/XML drafted guides did not reload.** Found by the differential below:
   `DraftReview.RequiresReview must be a boolean`. YAML now reads untagged scalars as
   strings, and XML drops empty lists. The Rust-written review record is retyped
   before validation (exact `true`/`false`, digit-only counts, missing lists).
5. **YAML typed fields read as text.** `WidescreenOnly: true` in YAML arrived as the
   string `"true"`; desktop/install code reads it with `as_bool`, so a widescreen-only
   mod was treated as ordinary. YAML now types `WidescreenOnly`/`AspyrExclusive` (as
   XML did) and resource `FileSize`/`ContentLength`/`SchemaVersion`/`UpdateAvailable`.

Adjacent correction: a different-URL follower (shared destination) never contacts
its own URL, but H4 metadata collection still probed it, so `--record` could store
headers describing bytes that were never downloaded. It is now skipped.

**Stale expectations, updated in the probes** (each deliberate and documented):
dated `settings/Logs/kotormodsync_*.log` mirrors C# NLog; post-download H4 provider
metadata adds one Nexus `files/{id}.json` call or a direct `HEAD` plus `Range: bytes=0-0`
GET (`COMPATIBILITY.md` "Provider metadata observations"); FOMOD fallbacks are counted
report warnings ([visible fallbacks](fomod-visible-fallbacks.md)); DeadlyStream is no
longer manual, so the shared-manual case uses GameFront; fixtures author the canonical
`HandlerMetadata` key, and FOMOD S2a fixtures no longer author renames.

All security checks in the Nexus suite passed unchanged: refused cross-host,
cross-port, alias and downgrade redirects make zero destination requests, the API key
never leaves the configured origin, and archive hosts never receive it.

**Not rerun here:** `origin-loopback-proof` needs IPv6 (`::1` origin, librqbit binds
`[::]:0`), which this kernel lacks. Its CA, redirect, proxy, cancel and key-isolation
cases passed in a patched scratch run except the TLS-text check that exposed item 1.
The GUI (Xvfb) step-2/3 probes were not run in this pass.

## C#/Rust differential on the public guides

Scripts are in [`runtime-revalidation-2026-10-03/`](runtime-revalidation-2026-10-03/);
raw outputs stay outside this repository. Both tools ran
`convert -i content/k{1,2}/full.md -f json` with and without `--parse-directions`.

- **Shape:** K1 186 roots (187 nodes drafted), K2 145 (148 drafted) in both tools,
  same Name/Author at every ordinal position.
- **Instructions and resources:** after GUID mapping by position and removing
  serializer omissions (empty fields, C# `FileSize=0`/`UpdateAvailable=false`
  defaults), the only instruction difference is C#'s action-dependent omission of
  `Overwrite` on Patcher/Extract/CleanList (K1 11, K2 9), which the model defaults
  to true in both. Delete `Overwrite=false` now matches.
- **Rust-only fields,** unchanged from the earlier baseline: `NameFieldContent` on
  every root, `UnresolvedMasters` on two roots per guide, `DraftReview` on 68 K1 and
  62 K2 roots.
- **Cross-format interchange:** each tool's drafted guide was exported to TOML, YAML
  and XML and reloaded by both tools. All twelve cross loads succeed. Rust's own YAML
  and XML reloads failed before fix 4 and pass after it. Remaining differences occur
  in both tools: `\` Source separators become `/`, a comma inside a `Language` value
  splits it into two entries, and C# TOML omits `Destination` on Delete/Patcher.
- **Validation:** the C# reference validate verb throws
  `'path' cannot be null or whitespace` (`ComponentValidation.cs:340`,
  `GetCaseSensitivePath` on an empty Destination) for both guides, even with empty
  game/mod roots. Rust reports per-component results. No report comparison is
  possible on these guides until that reference defect is avoided.

## Limits

These runs prove the named CLI contracts on generic fixtures and the conversion
equivalence above. They do not prove installs of real mods, live providers, native
GUI behavior, Windows/macOS runtime or the F29 three-lane full build, which still
need legitimate archives and a game baseline.
