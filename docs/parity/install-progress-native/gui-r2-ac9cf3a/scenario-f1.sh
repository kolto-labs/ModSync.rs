#!/bin/sh
# E4: fail on G1, repoint folders at G2 (its own pending snapshot), then Restore.
R=/tmp/modsync-2b-native/r2/k1-f1; . /tmp/modsync-2b-native/r2/lib.sh
G2=/tmp/modsync-2b-native/r2/k1-f1-g2
$B/make_fixture.sh $G2 fail /Styles/KotorStyle.axaml
$B/make_fixture.sh $R fail /Styles/KotorStyle.axaml; : > $T; log "fixture G1=$R fail KOTOR1; G2=$G2 fail"
(cd $G2 && find game mods -type f | sort | xargs sha256sum) > $R/g2-before.sha256
log "G2 CLI: $B/modsync install --input $G2/guide.json --game-dir $G2/game --source-dir $G2/mods --use-file-selection --allow-processes"
$B/modsync install --input $G2/guide.json --game-dir $G2/game --source-dir $G2/mods --use-file-selection --allow-processes > $R/g2-cli.out 2>&1
log "G2 CLI exit $?"
(cd $G2 && find game mods -type f | sort | xargs sha256sum) > $R/g2-after-fail.sha256
ls -a $G2 | grep modsync >> $T; ls $G2/.modsync-*/ 2>/dev/null | head >> $T
manifest before
launch; trust_validate_confirm
sleep 14; shot 20-g1-failed; manifest after-failure
