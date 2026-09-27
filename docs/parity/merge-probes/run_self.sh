#!/bin/sh
# Self-merge of shipped builds: C# (prebuilt DLL, isolated HOME) vs Rust after2; plus Rust convert as content reference.
D=/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0
for g in KOTOR1_Full KOTOR2_Full; do
T=/run/media/brunner56/MyBook/Workspaces/ModSync/mod-builds/TOMLs/$g.toml
O=/tmp/f20-probe/self/$g
timeout 300 env HOME=/tmp/f20-probe/home XDG_CONFIG_HOME=/tmp/f20-probe/home/.config XDG_DATA_HOME=/tmp/f20-probe/home/.local/share DOTNET_CLI_HOME=/tmp/f20-probe/home dotnet $D/ModSync.Core.dll merge -e $T -n $T -f json -o $O.cs.json --plaintext > $O.cs.log 2>&1; echo "$g cs=$?"
timeout 300 env HOME=/tmp/f20-probe/home /tmp/f20-probe/modsync-after2 merge $T $T $O.rs.json > $O.rs.log 2>&1; echo "$g rs=$?"
timeout 300 env HOME=/tmp/f20-probe/home /tmp/f20-probe/modsync-after2 convert $T $O.conv.json > $O.conv.log 2>&1; echo "$g conv=$?"
done
