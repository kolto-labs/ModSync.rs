#!/bin/sh
# Task 3 CLI contract probes; clean git-archive build of ac9cf3a, --no-default-features.
E=$(cd "$(dirname "$0")" && pwd); MK=/tmp/modsync-2b-native/make_fixture.sh
run() { name=$1; shift; "$E/modsync" "$@" > "$E/$name.stdout" 2> "$E/$name.stderr"; echo $? > "$E/$name.exit"; }
man() { (cd "$1" && find game mods -type f | sort | xargs sha256sum; find . -maxdepth 1 -name '.modsync-*' | sort) > "$2"; }
sh $MK $E/ok ok Light; man $E/ok $E/ok.before.manifest
run typed-ok install --input $E/ok/guide.json --game-dir $E/ok/game --source-dir $E/ok/mods --use-file-selection --allow-processes --install-progress-json
man $E/ok $E/ok.after.manifest
sh $MK $E/fail fail Light; man $E/fail $E/fail.before.manifest
run typed-fail install --input $E/fail/guide.json --game-dir $E/fail/game --source-dir $E/fail/mods --use-file-selection --allow-processes --install-progress-json
man $E/fail $E/fail.after.manifest; ls -a $E/fail/.modsync-* > $E/fail.recovery.ls 2>&1
sh $MK $E/pre ok Light; rm $E/pre/mods/beta.txt; man $E/pre $E/pre.before.manifest
run typed-preflight install --input $E/pre/guide.json --game-dir $E/pre/game --source-dir $E/pre/mods --use-file-selection --allow-processes --install-progress-json
man $E/pre $E/pre.after.manifest
sh $MK $E/leg ok Light
run legacy-json install --input $E/leg/guide.json --game-dir $E/leg/game --source-dir $E/leg/mods --use-file-selection --allow-processes --json
run conflict-json install --input $E/leg/guide.json --game-dir $E/leg/game --source-dir $E/leg/mods --use-file-selection --install-progress-json --json
run conflict-report install --input $E/leg/guide.json --game-dir $E/leg/game --source-dir $E/leg/mods --use-file-selection --install-progress-json --validation-report
run help install --help
