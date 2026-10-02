# Legacy Nexus key import

`Store::nexus_api_key` now checks its own `nexusmods.config` when the resolved
settings key is absent or blank. This implements the pinned CLI's legacy-key
source (`ModBuildConverter.cs:202–239`) before caller-owned environment fallback.
An explicit Store remains isolated: it never consults another global directory
for this file. Implicit stores use their resolved current configuration root;
existing implicit legacy-settings lookup precedence remains unchanged.

The file is read through the private storage reader with a 64 KiB limit,
ordinary-file/no-follow and owner checks. UTF-8 with optional BOM is supported;
non-UTF-8 and embedded control characters are rejected without echoing content.
Whitespace-only files supply no key. This intentionally does not import unsafe
or corrupt credentials merely because the reference uses an unbounded text read.

Import takes the shared settings lock and rereads settings. A nonblank key saved
in the meantime wins. Only case aliases of `nexusModsApiKey` are replaced; other
settings are preserved through the established private atomic writer. Corrupt
current settings are not overwritten. The legacy file is never deleted. No
network validation is performed by this compatibility migration.

Callers that use `nexus_api_key` receive this behavior. Pure typed-settings
projection remains separate and does not silently perform migration. A failed
write returns an error rather than claiming the key was migrated. There were no
tests, harnesses, runtime probes, native registrations, or builds in this slice.
