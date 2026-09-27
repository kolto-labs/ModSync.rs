#!/bin/sh
# Review #9: the input stamp changes mid-install (FOMOD choices file ctime), so the
# committed success is stale; the page must offer Restart wizard.
R=/tmp/modsync-2b-native/r2/light-review9; . /tmp/modsync-2b-native/r2/lib.sh
$B/make_fixture.sh $R ok /Styles/LightStyle.axaml; : > $T; log "fixture $R ok Light"
printf '{"version":1,"archives":[]}\n' > $R/choices.json; log "wrote empty version-1 FOMOD choices $R/choices.json"
manifest before
launch
c 700 12 1 "warm-up click on title bar text"
c 62 34 1.5 "Guide and settings"; c 62 115 1.5 "Validation tools"; c 23 142 1.5 "Trust external programs"
xdotool windowfocus --sync $(win); log windowfocus
c 450 206 0.5 "FOMOD choices file field"; xdotool type --window $(win) --delay 5 "$R/choices.json"; log "typed choices path"
sleep 0.5; shot 02-trust-choices; c 262 34 1.5 "Return to wizard"
for i in 1 2 3 4 5; do c 1206 728 1.5 "Next"; done
c 95 303 4 "Run Validation"; c 1206 767 1.2 "Next"; c 1206 678 1 "Next"; shot 06-confirm
c 504 428 3 "Create snapshot and install"; shot 10-active
touch $R/choices.json; log "touch $R/choices.json (changes ctime, so the input stamp changes)"
sleep 20; shot 20-stale-success; manifest after
