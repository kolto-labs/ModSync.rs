#!/usr/bin/env bash
# C# <-> Rust managed-deployment differential. Runs a built C# ModSync.Core.dll
# (no compilation) and the Rust CLI against disposable trees under /tmp/csprobe.
# Needs: dotnet runtime 10 on PATH (run on the host), CS_DLL, B (Rust binary).
# Exits 1 when any expected differential result is not observed, 2 on setup errors.
# The DLL is identified by hash only; this script cannot prove which source built it.
set -u
CS_DLL=${CS_DLL:-/run/media/brunner56/MyBook/Workspaces/ModSync/src/ModSync.GUI/bin/Debug/net10.0/ModSync.Core.dll}
B=${B:-/home/brunner56/.cache/modsync-rs-target-dep/debug/modsync}
[ -f "$CS_DLL" ] && [ -x "$B" ] && command -v dotnet >/dev/null || { echo "need CS_DLL, B and dotnet" >&2; exit 2; }
echo "C# dll: $CS_DLL sha256=$(sha256sum "$CS_DLL" | cut -d' ' -f1)"
echo "rust:   $B sha256=$(sha256sum "$B" | cut -d' ' -f1)"
fail=0
expect(){ if [ "$2" = "$3" ]; then echo "  PASS $1"; else echo "  FAIL $1: got '$2', want '$3'"; fail=1; fi; }
P=/tmp/csprobe; rm -rf $P; mkdir -p $P/game/Override $P/mods/ModA/Override $P/cfg $P/home
printf 'fake exe\n' > $P/game/swkotor.exe; printf ORIG > $P/game/Override/x.2da
printf CSNEW > $P/mods/ModA/Override/x.2da; printf ADDED > $P/mods/ModA/Override/added.tga
cat > $P/build.toml <<'TOML'
[metadata]
fileFormatVersion = "2.0"

[[thisMod]]
Guid = "aaaaaaaa-0000-0000-0000-00000000000a"
Name = "Diff Mod A"
IsSelected = true

[[thisMod.Instructions]]
Guid = "aaaaaaaa-0000-0000-0000-0000000000a1"
Action = "Copy"
Source = ["<<modDirectory>>\\ModA\\Override\\*"]
Destination = "<<kotorDirectory>>\\Override"
Overwrite = true
TOML
CFG=$P/cfg/ModSync; MR=$CFG/profiles/One/deployment
$B profile --action create --settings-dir $CFG --name One >/dev/null
cs(){ rm -rf $P/game/.modsync; XDG_CONFIG_HOME=$P/cfg HOME=$P/home dotnet "$CS_DLL" install -i $P/build.toml -g $P/game -s $P/mods --managed --profile One -y --skip-validation --no-checkpoint 2>&1 | grep -E "\[Deployment\]|Manifests written"; }
rs(){ $B deployment "$@" --game-dir $P/game --settings-dir $CFG --profile One; }

echo "D1 C# writes, Rust reads and uninstalls"
cs; cat $MR/manifests/*.json
expect "C# wrote one manifest" "$(ls $MR/manifests | wc -l)" 1
expect "Rust status reads it" "$(rs status | python3 -c 'import json,sys;print(len(json.load(sys.stdin)))')" 1
rs uninstall --component AAAAAAAA-0000-0000-0000-00000000000A >$P/d1.json
expect "Rust uninstall restored the original" "$(cat $P/game/Override/x.2da)" ORIG
expect "Rust uninstall removed the added file" "$(test -e $P/game/Override/added.tga && echo present || echo gone)" gone

echo "D2 Rust deploys RustB, C# deploys A over it, Rust purges"
mkdir -p $CFG/profiles/One/staging/bbbbbbbb-0000-0000-0000-00000000000b/Override
printf RUSTB > $CFG/profiles/One/staging/bbbbbbbb-0000-0000-0000-00000000000b/Override/x.2da
rs deploy --component bbbbbbbb-0000-0000-0000-00000000000b --name RustB >/dev/null
rm -rf $CFG/profiles/One/staging/aaaaaaaa-0000-0000-0000-00000000000a
out=$(cs); echo "$out"
expect "C# read the Rust manifest (conflict names RustB)" "$(echo "$out" | grep -c "already deployed by component 'RustB'")" 1
expect "Rust purge removed both" "$(rs purge | python3 -c 'import json,sys;print(json.load(sys.stdin)["purged"])')" 2
expect "mixed purge restored the original" "$(cat $P/game/Override/x.2da)" ORIG

echo "D3 C# redeploys the same component twice"
rm -rf $CFG/profiles/One/staging/*
for i in 1 2; do cs; echo "  run $i: live=$(cat $P/game/Override/x.2da) backup=$(cat $MR/backups/aaaaaaaa-0000-0000-0000-00000000000a/Override/x.2da)"; done
rs purge >/dev/null
expect "C# redeploy overwrote its own backup (known C# defect, Rust refuses this)" "$(cat $P/game/Override/x.2da)" CSNEW
echo "TOTAL fail=$fail"
exit $fail
