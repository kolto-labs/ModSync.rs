#!/bin/sh
# usage: drive.sh <root> : launches app, enables trust, validates, confirms install
R=$1
(setsid /tmp/modsync-2b-native/launch.sh $R & echo $! > $R/app.pid)
sleep 8
export DISPLAY=:93
c() { xdotool mousemove $1 $2 click 1; sleep ${3:-0.8}; }
c 62 34 1.5; c 62 115 1.5; c 23 142 1.5; c 262 34 1.5
for i in 1 2 3 4 5; do c 1206 728 1.5; done
c 95 303 4
c 1206 767 1
c 1206 678 0.8
/tmp/modsync-2b-native/shot.sh $R/06-confirm.png
c 504 428 0.1
