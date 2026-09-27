#!/bin/sh
# Real-corpus sanity check: CLI selection vs an independent Python oracle.
P=/tmp/selection-probes
T=/run/media/brunner56/MyBook/Workspaces/ModSync/mod-builds/TOMLs
O=$P/out-corpus
rm -rf "$O"; mkdir -p "$O"
sha256sum $T/KOTOR1_Full.toml $T/KOTOR2_Full.toml > "$O/inputs.sha256"
for g in 1 2; do
  f=$T/KOTOR${g}_Full.toml
  gd=$P/fixtures/games/k1; [ $g = 2 ] && gd=$P/fixtures/games/k2aspyr
  for mode in default tier2 tier1-add-bugfix; do
    case $mode in
      default) set -- ;;
      tier2) set -- --select "tier:2 - Recommended" ;;
      tier1-add-bugfix) set -- --select "tier:1 - Essential" --add-category bugfix ;;
    esac
    name=k$g-$mode
    $P/bin/modsync-new selection -i $f --game-dir $gd --json "$@" >"$O/$name.stdout" 2>"$O/$name.stderr"
    echo "$name	$?	selection -i $f --game-dir $gd --json $*" >>"$O/argv.tsv"
    python3 $P/corpus_oracle.py $f $mode >"$O/$name.oracle.json"
  done
done
