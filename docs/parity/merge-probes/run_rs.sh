#!/bin/sh
# usage: run_rs.sh <binary> <tag> [baseline]  -> writes cases/*/<tag>.json
BIN=$1; TAG=$2; MODE=$3
for d in /tmp/f20-probe/cases/*/; do
  flags=$(cat $d/flags)
  if [ "$MODE" = baseline ]; then
    flags=$(echo "$flags" | sed 's/--prefer-existing-modlinks/--prefer-existing-resource-registry/')
    case "$flags" in *installation-method*) echo "$(basename $d) flag-missing"; rm -f $d/$TAG.json; continue;; esac
  fi
  env HOME=/tmp/f20-probe/home $BIN merge $d/existing.json $d/incoming.json $d/$TAG.json $flags > $d/$TAG.log 2>&1
  echo "$(basename $d) exit=$?"
done
