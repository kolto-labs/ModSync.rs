#!/bin/sh
R=/tmp/modsync-2b-native/r2/light-review9; . /tmp/modsync-2b-native/r2/lib.sh
log "recovery: launch clicks were not delivered (validation failed without trust); redo from the Validate page"
c 62 34 1.5 "Guide and settings"; c 62 115 1.5 "Validation tools"; c 23 142 1.5 "Trust external programs"
xdotool windowfocus --sync $(win); log windowfocus
c 450 206 0.5 "FOMOD choices file field"; xdotool key --window $(win) ctrl+a; xdotool type --window $(win) --delay 5 "$R/choices.json"; log "typed choices path"
sleep 0.5; shot 02b-trust-choices; c 262 34 1.5 "Return to wizard"
c 95 303 4 "Run Validation"; c 1206 767 1.2 "Next"; c 1206 678 1 "Next"; shot 06b-confirm
c 504 428 3 "Create snapshot and install"; shot 10-active
touch $R/choices.json; log "touch $R/choices.json (changes ctime, so the input stamp changes)"
sleep 20; shot 20-stale-success; manifest after
