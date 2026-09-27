#!/bin/sh
# E2: K2 cancel with a timestamped transcript, plus a real close request.
R=/tmp/modsync-2b-native/r2/k2-cancel; . /tmp/modsync-2b-native/r2/lib.sh
$B/make_fixture.sh $R ok /Styles/Kotor2Style.axaml; : > $T; log "fixture $R ok KOTOR2"; manifest before
launch; trust_validate_confirm
sleep 3; shot 10-active
log "send WM_DELETE_WINDOW to $(win) via $B/wm_delete.py"; python3 $B/wm_delete.py $(win) >> $T 2>&1
sleep 0.8; shot 11-close-attempt
