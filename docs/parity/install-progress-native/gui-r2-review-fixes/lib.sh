# Sourced by scenario scripts. Records every action with a wall-clock timestamp.
B=/tmp/modsync-2b-native
export DISPLAY=:93
T=${R}/transcript.txt
log() { echo "$(date +%H:%M:%S.%3N) $*" >> "$T"; }
c() { log "click $1,$2 ${4:-}"; xdotool mousemove $1 $2 click 1; sleep ${3:-1.2}; }
shot() { DISPLAY=:93 import -window root "$R/$1.png"; log "screenshot $1.png"; }
win() { xdotool search --name "ModSync.rs" | head -1; }
size() { xdotool windowsize $(win) $1 $2; log "windowsize $1x$2"; sleep 1.5; }
manifest() { (cd "$R" && find game mods -type f | sort | xargs sha256sum) > "$R/$1.sha256"; log "manifest $1.sha256"; }
launch() {
  log "launch: $B/launch.sh $R  (modsync gui --settings-dir $R/settings --input $R/guide.json --game-dir $R/game --source-dir $R/mods --allow-multiple-instances)"
  (setsid $B/launch.sh $R & echo $! > $R/app.pid); i=0; while [ -z "$(win)" ] && [ $i -lt 60 ]; do sleep 0.5; i=$((i+1)); done; sleep 4; log "app pgid $(cat $R/app.pid) window $(win)"
}
trust_validate_confirm() {
  # The first pointer click after launch is not delivered without a window manager.
  c 700 12 1 "warm-up click on title bar text"
  c 62 34 1.5 "Guide and settings"; c 62 115 1.5 "Validation tools"; c 23 142 1.5 "Trust external programs"
  shot 02-trust; c 262 34 1.5 "Return to wizard"
  for i in 1 2 3 4 5; do c 1206 728 1.5 "Next"; done
  c 95 303 4 "Run Validation"; c 1206 767 1.2 "Next"; c 1206 678 1 "Next"; shot 06-confirm
  c 504 428 0.1 "Create snapshot and install"
}
trust_from_validate() {
  c 62 34 1.5 "Guide and settings"; c 62 115 1.5 "Validation tools"; c 23 142 1.5 "Trust external programs"
  shot 02b-trust; c 262 34 1.5 "Return to wizard"
  c 95 303 4 "Run Validation"; c 1206 767 1.2 "Next"; c 1206 678 1 "Next"; shot 06b-confirm
  c 504 428 0.1 "Create snapshot and install"
}
