#!/bin/sh
D=/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0
d=/tmp/f20-probe/cases/$1
env HOME=/tmp/f20-probe/home XDG_CONFIG_HOME=/tmp/f20-probe/home/.config XDG_DATA_HOME=/tmp/f20-probe/home/.local/share DOTNET_CLI_HOME=/tmp/f20-probe/home \
  dotnet $D/ModSync.Core.dll merge -e $d/existing.json -n $d/incoming.json -f json -o $d/cs.json --plaintext $(cat $d/flags) > $d/cs.log 2>&1
echo "$1 exit=$?"
