#!/bin/sh
D=/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0
for k in p1_order p8_instr p4_domain; do d=/tmp/f20-probe/cases/$k
env HOME=/tmp/f20-probe/home XDG_CONFIG_HOME=/tmp/f20-probe/home/.config XDG_DATA_HOME=/tmp/f20-probe/home/.local/share DOTNET_CLI_HOME=/tmp/f20-probe/home \
  dotnet $D/ModSync.Core.dll merge -e $d/existing.json -n $d/incoming.json -f toml -o $d/cs.toml --plaintext > $d/cs-toml.log 2>&1; echo "$k cs=$?"
/tmp/f20-probe/modsync-after merge $d/existing.json $d/incoming.json $d/rs_after.toml > $d/rs-toml.log 2>&1; echo "$k rs=$?"
done
