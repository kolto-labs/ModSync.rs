#!/bin/sh
# usage: make_fixture.sh <root> <mode: ok|fail> <theme>
set -e
R=$1; MODE=$2; THEME=$3
rm -rf "$R"; mkdir -p "$R/game/Override" "$R/mods" "$R/settings" "$R/home" "$R/xdg/config" "$R/xdg/data" "$R/xdg/cache" "$R/xdg/runtime"
chmod 700 "$R/xdg/runtime"
: > "$R/game/swkotor.exe"; : > "$R/game/dialog.tlk"
for n in alpha beta gamma; do printf "%s fixture bytes\n" $n > "$R/mods/$n.txt"; done
printf '#!/bin/sh\nsleep 6\nexit 0\n' > "$R/mods/slow.sh"
printf '#!/bin/sh\nsleep 3\necho "fixture failure" >&2\nexit 3\n' > "$R/mods/fail.sh"
chmod +x "$R/mods/slow.sh" "$R/mods/fail.sh"
WIDE=""
LONG=$(for i in 1 2 3 4 5 6 7 8 9 10 11 12; do printf "Long direction sentence %s keeps going so the text must wrap and clip at sixty pixels. " $i; done)
[ "$4" = wide ] && { printf "wide fixture bytes\n" > "$R/mods/wide.txt"; WIDE=',
 {"Guid":"w4","Name":"Wide Fix","IsSelected":true,"WidescreenOnly":true,"Directions":"Widescreen directions (GUID w4).","Instructions":[
   {"Action":"Copy","Source":["<<modDirectory>>/wide.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true},
   {"Action":"Run","Source":["<<modDirectory>>/slow.sh"]}]}'; }
SECOND=slow.sh; [ "$MODE" = fail ] && SECOND=fail.sh
cat > "$R/guide.json" <<J
{"thisMod":[
 {"Guid":"a1","Name":"Alpha Textures","IsSelected":true,"Directions":"Alpha directions: copy the alpha file, then run the slow step. $LONG","Instructions":[
   {"Action":"Copy","Source":["<<modDirectory>>/alpha.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true},
   {"Action":"Run","Source":["<<modDirectory>>/slow.sh"]}]},
 {"Guid":"b2","Name":"Beta Patch","IsSelected":true,"Directions":"First Beta Patch directions (GUID b2).","Instructions":[
   {"Action":"Copy","Source":["<<modDirectory>>/beta.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true},
   {"Action":"Run","Source":["<<modDirectory>>/$SECOND"]}]},
 {"Guid":"c3","Name":"Beta Patch","IsSelected":true,"Directions":"Second Beta Patch directions (GUID c3), same display name.","Instructions":[
   {"Action":"Copy","Source":["<<modDirectory>>/gamma.txt"],"Destination":"<<gameDirectory>>/Override","Overwrite":true},
   {"Action":"Run","Source":["<<modDirectory>>/slow.sh"]}]}$WIDE
]}
J
printf '{"theme":"%s"}\n' "$THEME" > "$R/settings/settings.json"
