#!/bin/sh
# E2: K2 real cancel. Stop Install and the modal confirm are clicked back to back.
R=/tmp/modsync-2b-native/r2/k2-cancel; . /tmp/modsync-2b-native/r2/lib.sh
$B/make_fixture.sh $R ok /Styles/Kotor2Style.axaml; : > $T; log "fixture $R ok KOTOR2"; manifest before
launch; trust_validate_confirm
log "install start = previous click"; sleep 3; shot 10-active
c 990 678 0.4 "Stop Install (footer)"; shot 11-stop-confirm
c 390 427 0.2 "Stop install (modal confirm; cancellation token set now)"
sleep 1.5; shot 20-cancelled; manifest after-cancel
xdotool mousemove 470 450; for i in 1 2 3 4 5 6 7 8 9 10; do xdotool click 5; sleep 0.05; done; log "scroll page down"; sleep 0.8
shot 21-scrolled
