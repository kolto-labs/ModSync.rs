#!/bin/sh
# Review #2: utility Restore after a path edit restores G1, not G2.
R=/tmp/modsync-2b-native/r2/k1-review2; . /tmp/modsync-2b-native/r2/lib.sh
G2=/tmp/modsync-2b-native/r2/k1-review2-g2
$B/make_fixture.sh $G2 fail /Styles/KotorStyle.axaml
$B/make_fixture.sh $R fail /Styles/KotorStyle.axaml; : > $T; log "fixture G1=$R fail KOTOR1; G2=$G2 fail"
(cd $G2 && find game mods -type f | sort | xargs sha256sum) > $R/g2-before.sha256
log "G2 CLI: $B/modsync install --input $G2/guide.json --game-dir $G2/game --source-dir $G2/mods --use-file-selection --allow-processes"
$B/modsync install --input $G2/guide.json --game-dir $G2/game --source-dir $G2/mods --use-file-selection --allow-processes > $R/g2-cli.out 2>&1
log "G2 CLI exit $?"
(cd $G2 && find game mods -type f | sort | xargs sha256sum) > $R/g2-after-fail.sha256
manifest before
launch; trust_validate_confirm
sleep 14; shot 20-g1-failed; manifest after-failure
c 62 34 1.5 "Guide and settings"; xdotool windowfocus --sync $(win); log windowfocus
c 370 226 0.5 "Game directory field"; xdotool key --window $(win) ctrl+a; xdotool type --window $(win) --delay 5 "$G2/game"; log "typed game dir G2"
c 370 247 0.5 "Mod directory field"; xdotool key --window $(win) ctrl+a; xdotool type --window $(win) --delay 5 "$G2/mods"; log "typed mod dir G2"
sleep 0.5; shot 30-settings-g2
c 62 115 1.5 "Validation tools"; shot 31-utility-page
