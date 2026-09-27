#!/bin/sh
# E3: Light, 600x400 Stop Install while active and Continue after success.
R=/tmp/modsync-2b-native/r2/light-small; . /tmp/modsync-2b-native/r2/lib.sh
$B/make_fixture.sh $R ok /Styles/LightStyle.axaml; : > $T; log "fixture $R ok Light"; manifest before
launch; trust_validate_confirm
sleep 2; size 600 400; shot 10-small-active-stop-install
sleep 18; shot 20-small-success-continue; manifest after
size 1280 800; shot 21-success-large
