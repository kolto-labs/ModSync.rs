#!/bin/sh
# Selection CLI probes (Task 2). Read-only against fixtures; outputs under out/.
P=/tmp/selection-probes
B=$P/bin/modsync-new
F=$P/fixtures/synthetic.toml
G=$P/fixtures/games
O=$P/out
rm -rf "$O"; mkdir -p "$O"
n=0
probe() { # name, then argv after the binary
  name=$1; shift
  "$B" "$@" >"$O/$name.stdout" 2>"$O/$name.stderr"; code=$?
  printf '%s\t%s\t%s\n' "$name" "$code" "$*" >>"$O/argv.tsv"
}
probe p1a-tier2           selection -i $F --game-dir $G/k2aspyr --select "tier:2 - Recommended" --json
probe p1b-closure         selection -i $F --game-dir $G/k2aspyr --select "Beta Addon" --json
probe p2-deselect-dep     selection -i $F --game-dir $G/k2aspyr --select "Beta Addon" --deselect "Alpha Core" --json
probe p3-nested-option    selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --option 00000000-0000-0000-0000-000000000014 --json
probe p4a-legacy-default  selection -i $F --game-dir $G/k2legacy --json
probe p4b-k1-default      selection -i $F --game-dir $G/k1 --json
probe p4c-unknown-default selection -i $F --game-dir $G/unknown --json
probe p4d-refuse-direct   selection -i $F --game-dir $G/k2legacy --select "Aspyr Only Mod" --json
probe p4e-refuse-closure  selection -i $F --game-dir $G/k2legacy --select "Needs Aspyr" --json
probe p5a-add-category    selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --add-category PATCH --json
probe p5b-reset-category  selection -i $F --game-dir $G/k2aspyr --select category:patch --json
probe p6-candidates       selection -i $F --game-dir $G/k2aspyr --json
probe p7a-mutual          selection -i $F --game-dir $G/k2aspyr --select "Mutual One" --select "Mutual Two" --json
probe p7b-default-quiet   selection -i $F --game-dir $G/k2aspyr --json
probe p9-argv-order-a     selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --deselect "Beta Addon" --select-through-tier "2 - Recommended" --json
probe p9-argv-order-b     selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --select-through-tier "2 - Recommended" --deselect "Beta Addon" --json
probe p10-search-spoiler  selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --select-visible-search harmless --spoiler-free
probe p10-search-plain    selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --select-visible-search harmless
probe p11-numeric-tier    selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --select-through-tier 2 --json
probe p12-bare-optional   selection -i $F --game-dir $G/k2aspyr --select "Alpha Core" --select-through-tier Optional --json
probe p13-unknown-comp    selection -i $F --select Missing
probe p13-unknown-option  selection -i $F --option Missing
probe p14-direct-md-conflict validate --direct-markdown --deselect x -i $F --game-dir $G/k1 --source-dir $P
probe p15-text            selection -i $F --game-dir $G/k2aspyr --select "Beta Addon"
probe p16-default-plus-flag-legacy selection -i $F --game-dir $G/k2legacy --deselect "Beta Addon" --json
probe p17-option-refused  selection -i $P/fixtures/synthetic-aspyr-option.toml --game-dir $G/k2legacy --select "Alpha Core" --option 00000000-0000-0000-0000-000000000015 --json
probe p18-option-refused-default selection -i $P/fixtures/synthetic-aspyr-option.toml --game-dir $G/k2legacy --option 00000000-0000-0000-0000-000000000015 --json
probe p19-option-dep-legacy selection -i $P/fixtures/synthetic-aspyr-option-dep.toml --game-dir $G/k2legacy --use-file-selection --json
probe p19-option-dep-aspyr  selection -i $P/fixtures/synthetic-aspyr-option-dep.toml --game-dir $G/k2aspyr --use-file-selection --json
