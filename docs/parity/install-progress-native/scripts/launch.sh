#!/bin/sh
# usage: launch.sh <root> ; runs on DISPLAY=:93 with private XDG/HOME/bus
R=$1
export DISPLAY=:93 HOME=$R/home XDG_CONFIG_HOME=$R/xdg/config XDG_DATA_HOME=$R/xdg/data XDG_CACHE_HOME=$R/xdg/cache XDG_RUNTIME_DIR=$R/xdg/runtime
unset WAYLAND_DISPLAY DBUS_SESSION_BUS_ADDRESS
cd $R
exec dbus-run-session -- /tmp/modsync-2b-native/modsync gui --settings-dir $R/settings --input $R/guide.json --game-dir $R/game --source-dir $R/mods --allow-multiple-instances > $R/app.log 2>&1
