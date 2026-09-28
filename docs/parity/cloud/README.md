# Cloud bootstrap and production proof

Everything required for the immediate source review and generic FOMOD fixtures is
in this repository or obtainable from the pinned public repositories below. No
original workstation, cached executable, agent mailbox, game installation or
private archive directory is needed. Historical receipts retain old paths only
as provenance; do not execute commands copied from those paths.

## Fresh Linux agent

Clone the private `bodencrouch/ModSync.rs` repository with an account/app granted
read access. Use Rust 1.95.0 with rustfmt and clippy (the last measured toolchain),
Python 3.9+, and a C/C++ toolchain. Suggested Ubuntu packages:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev libwayland-dev libxkbcommon-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev libgl1-mesa-dev python3 openssl iproute2 util-linux git ca-certificates bubblewrap
rustup toolchain install 1.95.0 --profile minimal --component rustfmt --component clippy
rustup override set 1.95.0
python3 docs/parity/fomod-s1-proof/gates.py
```

This runs only production lib/bin build and Clippy in both feature modes, plus
formatting. **No Rust tests/custom Rust harnesses/all-targets until full parity.**
CI uses the same production-only gate policy. Existing test-source deficiencies
remain explicitly unverified; green production gates are not full acceptance.

The gate script defaults to `<clone>/target`, honors `CARGO_TARGET_DIR`, and writes
fresh evidence under `target/cloud-proof/gates`. `MODSYNC_PROOF_OUT` overrides its
output directory. It does not overwrite retained historical receipts.

Run actual Linux CLI/PTY fixtures after building, each in its own isolated network
namespace. Namespace support and /proc plus PTYs are required:

```sh
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-s1-proof/probe.py
FOMOD_PARENT_NETNS="$(readlink /proc/self/ns/net)" unshare -Urn python3 docs/parity/fomod-s1-proof/inherited_controller.py
```

Both scripts generate all generic archives, TLS certificates, guides and settings.
They derive repository location from their own file, use `CARGO_TARGET_DIR` or
`target`, and accept `MODSYNC_BINARY` for an explicitly built production binary.
Fresh outputs default to `target/cloud-proof/fomod` and `inherited` respectively;
`MODSYNC_PROOF_OUT` overrides either. Failed comparisons fail the process. Do not
set Python optimization flags that disable assertions. If user/network namespaces
are denied, report the runtime-proof limitation and continue source review;
**never run network fixtures in the host namespace**. No GUI session is required.

## Public C# reference and guides

Fetch exact commits, not default branches:

```sh
mkdir -p ../modsync-reference
cd ../modsync-reference
git init csharp
git -C csharp remote add origin https://github.com/bodencrouch/ModOrchestration.git
git -C csharp fetch --depth=1 origin d31ccbd3bfcddb4705917eafa6483715c166de3b
git -C csharp checkout --detach FETCH_HEAD
git init guides
git -C guides remote add origin https://github.com/KOTOR-Community-Portal/mod-builds.git
git -C guides fetch --depth=1 origin 336b36d911b879b0254da5d08a6cce72c28607e5
git -C guides checkout --detach FETCH_HEAD
```

Both commits were verified accessible through GitHub when publishing this handoff.
For source review no .NET build is needed. For C# production CLI comparisons,
install a .NET 9 SDK (the reference global.json requests9.0.100/latestFeature), then:

```sh
cd csharp
dotnet build src/ModSync.Core/ModSync.Core.csproj -c Debug -f net9.0 -o ../csharp-bin -p:UseSharedCompilation=false
dotnet ../csharp-bin/ModSync.Core.dll --help
```

The core csproj has no project-reference/submodule prerequisite. Build output is
new cloud evidence, not expected to match an old machine-specific DLL hash.
Reference GUI/backend/full-install proof has additional platform/resource needs;
source inspection and generic Rust fixtures do not require them. Never call
validation parity proved when the reference fails a missing-patcher prerequisite.

Canonical guides are `content/k1/full.md` and `content/k2/full.md`. At this public
pin their SHA256 values are:

- K1: `fc81840edb64da1684697dc89d72c9235c6afaa43d4d1e0b1989e4f206ba1cd2`
- K2: `6e3336a7c731807c5f6f2e4bef8065ab8b3ee47377594b25f7ef17da1568f8f1`

The old full-markdown-baseline K2 receipt used a locally modified guide with a
different hash. That exact raw guide was not published. Treat that receipt as
historical; create a fresh bilateral baseline from the public pin. This is not a
hidden dependency: no next task requires the old private raw outputs. Keep real
published names/raw guide outputs outside this Rust repository.

## Remaining external acceptance

No licensed game baseline, mod archive cache, native Windows/macOS runtime, live
account credential or desktop recording is bundled. F29 must obtain legitimate
inputs on the execution host and create new isolated C#/Rust/manual lanes; it
cannot depend on the former workstation. Missing external inputs block that
specific acceptance, not implementation or current quality review. Old proof
scripts outside the three ported entry points may retain historical paths; adapt
them to fresh clones/binaries when their slice is next exercised. All needed
source revisions are in Git history and public reference pins above. Do not claim
those old scripts are already portable or historical binary hashes reproducible.
