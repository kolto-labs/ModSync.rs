#!/bin/sh
# Probe 8: legacy flags, old (c28f4fc) vs new binary. Each run gets fresh copies of the dirs.
P=/tmp/selection-probes
F=${F:-$P/fixtures/synthetic.toml}
O=${O:-$P/out-legacy}
rm -rf "$O" $P/scratch; mkdir -p "$O"
run() { # name, gamefixture, cmd, args...
  name=$1; gf=$2; shift 2
  for which in base new; do
    w=$P/scratch/$name-$which; mkdir -p $w/src; cp -r $P/fixtures/games/$gf $w/game
    "$P/bin/modsync-$which" "$@" -i $F --game-dir $w/game --source-dir $w/src >"$O/$name.$which.stdout" 2>"$O/$name.$which.stderr"
    code=$?
    # scrub the per-run scratch path so outputs compare
    sed -i "s#$w#<W>#g" "$O/$name.$which.stdout" "$O/$name.$which.stderr"
    printf '%s\t%s\t%s\t%s\n' "$name" "$which" "$code" "$*" >>"$O/argv.tsv"
  done
}
run l1-validate-default   k2aspyr validate --json
run l2-validate-selwarn   k2aspyr validate --json --select "Alpha Core" --select "bogus:x" --select ""
run l3-unknown-option     k2aspyr validate --json --option Missing
run l4-unknown-component  k2aspyr validate --json --select Missing
run l5-install-default    k2aspyr install --json
run l6-validate-option    k2aspyr validate --json --select "Delta: Colon Mod" --option 00000000-0000-0000-0000-000000000012
run l7-conflicts-select   k2aspyr conflicts --select "Beta Addon"
run l8-validate-k1-dev    k1      validate --json
run l9-validate-one      k2aspyr validate --json --select "Alpha Core"
run l10-install-one      k2aspyr install --json --select "Alpha Core" --select "Epsilon"
run l11-validate-opt     k2aspyr validate --json --select "Delta: Colon Mod" --option 00000000-0000-0000-0000-000000000012
run l12-validate-closure k2aspyr validate --json --select "Beta Addon"
cd "$O"
for f in *.base.stdout; do n=${f%.base.stdout}
  so=$(cmp -s $n.base.stdout $n.new.stdout && echo same || echo DIFF)
  se=$(cmp -s $n.base.stderr $n.new.stderr && echo same || echo DIFF)
  echo "$n stdout=$so stderr=$se"
done > compare.txt
cat compare.txt
