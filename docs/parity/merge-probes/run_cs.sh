#!/bin/sh
# Run pinned C# merge on every case, isolated HOME.
D=/tmp/modsync-validation-reference-20260927-5813/artifacts/bin/ModSync.Core/debug_net9.0
for d in /tmp/f20-probe/cases/*/; do
  flags=$(cat $d/flags)
  env HOME=/tmp/f20-probe/home XDG_CONFIG_HOME=/tmp/f20-probe/home/.config XDG_DATA_HOME=/tmp/f20-probe/home/.local/share DOTNET_CLI_HOME=/tmp/f20-probe/home \
    dotnet $D/ModSync.Core.dll merge -e $d/existing.json -n $d/incoming.json -f json -o $d/cs.json --plaintext $flags > $d/cs.log 2>&1
  echo "$(basename $d) exit=$?" 
done
