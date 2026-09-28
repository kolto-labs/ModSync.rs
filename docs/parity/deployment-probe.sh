#!/usr/bin/env bash
# Disposable deployment probes against the real `modsync deployment` CLI.
# Usage: B=<path to modsync binary> bash docs/parity/deployment-probe.sh
# Writes only under /tmp/depprobe/run. Exits 1 if any probe fails, 2 on setup errors.
set -u
B=${B:-/home/brunner56/.cache/modsync-rs-target-dep/debug/modsync}
[ -x "$B" ] || { echo "binary not found: $B" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 2; }
bin_mtime_epoch=$(stat -c %Y "$B")
echo "binary: $B sha256=$(sha256sum "$B" | cut -d' ' -f1) mtime=$(stat -c %y "$B")"
# Bind the probe run to the commit the binary was actually built from, not
# just the worktree's current HEAD: HEAD plus a clean `src/` tree is only
# trustworthy provenance when the binary is newer than that commit.
src_head=$(git -C "$(dirname "$0")" rev-parse HEAD 2>/dev/null)
src_head_epoch=$(git -C "$(dirname "$0")" log -1 --format=%ct HEAD 2>/dev/null)
src_dirty=""
git -C "$(dirname "$0")" diff --quiet HEAD -- ../../src 2>/dev/null || src_dirty=" +uncommitted src changes"
staleness=""
if [ -z "$src_dirty" ] && [ -n "$src_head_epoch" ] && [ "$bin_mtime_epoch" -lt "$src_head_epoch" ]; then
  staleness=" +WARNING binary mtime predates HEAD commit; it was not necessarily built from it"
fi
echo "source: ${src_head}${src_dirty}${staleness}"
R=${PROBE_ROOT:-/tmp/depprobe/run}; mkdir -p "$R"
[ -z "$(ls -A "$R")" ] || { echo "probe root must be empty: $R" >&2; exit 2; }
A=aaaaaaaa-0000-0000-0000-000000000001; Bc=bbbbbbbb-0000-0000-0000-000000000002; C=cccccccc-0000-0000-0000-000000000003
pass=0; fail=0
ok(){ if eval "$2"; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1"; fail=$((fail+1)); fi; }
man(){ # summarize manifests in $1
  python3 - "$1" <<'PY'
import json,sys,os,glob
d=sys.argv[1]+"/manifests"
out=[]
for f in sorted(glob.glob(d+"/*.json")):
    try: m=json.load(open(f))
    except Exception as e: out.append(os.path.basename(f)+":CORRUPT"); continue
    out.append("%s[%s]"%(m["ComponentGuid"][:8],",".join("%s%s%s"%(e["RelativePath"],"+bk" if e["OverwroteExisting"] else "","/hl" if e["DeploymentMethod"]==0 else "/cp") for e in m["Entries"])))
print(" ".join(out) or "(none)")
PY
}
new(){ W=$R/$1; G=$W/game; S=$W/staging; M=$W/managed; mkdir -p $G $S $M; }
dep(){ $B deployment "$@" --game-dir $G --staging-dir $S --manifest-dir $M; }
stage(){ mkdir -p "$(dirname "$S/$1/$2")"; printf '%s' "$3" > "$S/$1/$2"; }
tree(){ (cd $1 && find . -type f | sort | while read f; do printf '%s=%s ' "$f" "$(cat "$f")"; done); echo; }

echo "P1 deploy into empty target"; new p1
stage $A Override/a.tga A1; echo "  before: $(man $M)"
dep deploy --component $A --name ModA >$W/out 2>$W/err; echo "  after:  $(man $M)"
ok "file deployed" '[ "$(cat $G/Override/a.tga)" = A1 ]'
ok "hardlinked (same inode)" '[ "$(stat -c %i $G/Override/a.tga)" = "$(stat -c %i $S/$A/Override/a.tga)" ]'
ok "DeployedUtc 7 fractional digits" 'grep -qE "\"DeployedUtc\": \"[0-9T:-]+\.[0-9]{7}Z\"" $M/manifests/$A.json'
dep uninstall --component $A >$W/out2; echo "  uninstall: $(man $M)"
ok "removed and pruned" '[ ! -e $G/Override ]'

echo "P2 deploy over pre-existing file, uninstall restores"; new p2
mkdir -p $G/Override; printf ORIG > $G/Override/a.tga; stage $A Override/a.tga NEW
dep deploy --component $A >/dev/null; echo "  after deploy: $(man $M) backup=$(cat $M/backups/$A/Override/a.tga)"
ok "new bytes live" '[ "$(cat $G/Override/a.tga)" = NEW ]'
dep uninstall --component $A >$W/out; echo "  after uninstall: $(man $M)"
ok "original restored" '[ "$(cat $G/Override/a.tga)" = ORIG ]'
ok "backup tree removed (nested empty dirs pruned)" '[ ! -e $M/backups/$A ]'

echo "P3 same component deployed twice"; new p3
mkdir -p $G/Override; printf ORIG > $G/Override/a.tga; stage $A Override/a.tga NEW
dep deploy --component $A >/dev/null; dep deploy --component $A >$W/out 2>$W/err; rc=$?
echo "  second deploy rc=$rc: $(tail -1 $W/err)"; echo "  manifests: $(man $M) backup=$(cat $M/backups/$A/Override/a.tga)"
ok "second deploy refused" '[ $rc -ne 0 ]'
ok "original backup intact" '[ "$(cat $M/backups/$A/Override/a.tga)" = ORIG ]'
dep uninstall --component $A >/dev/null
ok "original back after uninstall" '[ "$(cat $G/Override/a.tga)" = ORIG ]'

echo "P4 user-modified deployed file is preserved"; new p4
mkdir -p $G/Override; printf ORIG > $G/Override/a.tga; stage $A Override/a.tga NEW; stage $A Override/b.tga B
dep deploy --component $A >/dev/null; echo "  before: $(man $M)"
rm $G/Override/a.tga; printf USER > $G/Override/a.tga   # replace (not edit-in-place) so the staged hardlink stays NEW
dep uninstall --component $A >$W/out 2>$W/err; echo "  after: $(man $M)  report: $(python3 -c 'import json,sys;r=json.load(open(sys.argv[1]));print({k:r[k] for k in ("deleted","restored","skipped")})' $W/out)"
ok "user file kept" '[ "$(cat $G/Override/a.tga)" = USER ]'
ok "b removed" '[ ! -e $G/Override/b.tga ]'
ok "original backup retained" '[ "$(cat $M/backups/$A/Override/a.tga)" = ORIG ]'
ok "skipped reported" 'grep -q "Override/a.tga" <(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))[\"skipped\"])" $W/out)'

echo "P5 purge order across three components"; new p5
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da
stage $A Override/x.2da A; stage $Bc Override/x.2da B; stage $C Override/x.2da C; stage $C Override/c.txt C
for c in $A $Bc $C; do dep deploy --component $c >/dev/null 2>>$W/err; sleep 0.05; done
echo "  before: $(man $M)"; echo "  conflict warnings: $(grep -c Conflict $W/err)"
dep purge >$W/out; echo "  after: $(man $M) order=$(python3 -c 'import json,sys;print([c["component"][:8] for c in json.load(open(sys.argv[1]))["components"]])' $W/out)"
ok "purge newest first" '[ "$(python3 -c "import json,sys;print(\"\".join(c[\"component\"][0] for c in json.load(open(sys.argv[1]))[\"components\"]))" $W/out)" = cba ]'
ok "original restored after purge" '[ "$(cat $G/Override/x.2da)" = ORIG ] && [ ! -e $G/Override/c.txt ]'
ok "two conflict warnings" '[ "$(grep -c Conflict $W/err)" = 2 ]'

echo "P6 out-of-order uninstall hands the original backup to the later owner"; new p6
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da; stage $A Override/x.2da A; stage $Bc Override/x.2da B
dep deploy --component $A >/dev/null; sleep 0.05; dep deploy --component $Bc >/dev/null 2>&1
echo "  before: $(man $M) Bbackup=$(cat $M/backups/$Bc/Override/x.2da)"
dep uninstall --component $A >$W/out 2>&1; echo "  after A uninstall: $(man $M) Bbackup=$(cat $M/backups/$Bc/Override/x.2da) transferred=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["transferred"])' $W/out 2>/dev/null)"
ok "B still live" '[ "$(cat $G/Override/x.2da)" = B ]'
ok "B backup now original" '[ "$(cat $M/backups/$Bc/Override/x.2da)" = ORIG ]'
dep uninstall --component $Bc >/dev/null
ok "original restored after B uninstall" '[ "$(cat $G/Override/x.2da)" = ORIG ]'
ok "no orphan backups" '[ -z "$(find $M/backups -type f)" ]'

echo "P7 bracketed / trailing-dot filenames"; new p7
stage $A 'Movies/intro[1].bik' M; stage $A 'Override/odd.' D
dep deploy --component $A >$W/out 2>$W/err; rc=$?; echo "  rc=$rc after: $(man $M)"
ok "deployed bracket name" '[ $rc -eq 0 ] && [ "$(cat "$G/Movies/intro[1].bik")" = M ]'
dep uninstall --component $A >/dev/null; ok "removed" '[ -z "$(find $G -type f)" ]'

echo "P8 symlinked Override inside game dir is refused, nothing changed"; new p8
mkdir -p $W/outside; ln -s $W/outside $G/Override; stage $A Movies/m.bik M; stage $A Override/a.tga X
dep deploy --component $A >$W/out 2>$W/err; rc=$?; echo "  rc=$rc: $(tail -1 $W/err)"; echo "  after: $(man $M)"
ok "refused" '[ $rc -ne 0 ]'
ok "nothing written outside" '[ -z "$(ls -A $W/outside)" ]'
ok "no partial deploy" '[ ! -e $G/Movies ] && [ -z "$(ls $M/manifests)" ]'

echo "P9 staged symlink refused"; new p9
mkdir -p $S/$A; ln -s /etc/hostname $S/$A/link.txt
dep deploy --component $A >$W/out 2>$W/err; rc=$?; echo "  rc=$rc: $(tail -1 $W/err)"
ok "refused" '[ $rc -ne 0 ] && [ -z "$(ls -A $G)" ]'

echo "P10 mid-deploy failure rolls back"; new p10
mkdir -p $G/Override; printf ORIG > $G/Override/a.tga; stage $A Override/a.tga NEW; stage $A Override/z/b.tga B
mkdir -p $G/Override/z; chmod 555 $G/Override/z   # unwritable directory: link/copy of z/b.tga fails
dep deploy --component $A >$W/out 2>$W/err; rc=$?; chmod 755 $G/Override/z; echo "  rc=$rc: $(tail -1 $W/err | cut -c1-140)"
echo "  after: $(man $M)"
ok "failed" '[ $rc -ne 0 ]'
ok "original back in place" '[ "$(cat $G/Override/a.tga)" = ORIG ]'
ok "no manifest, no backups" '[ -z "$(ls $M/manifests)" ] && [ -z "$(find $M/backups -type f)" ]'

echo "P11 case-aware destination"; new p11
mkdir -p $G/Override; printf ORIG > $G/Override/A.TGA; stage $A override/a.tga NEW
dep deploy --component $A >/dev/null; echo "  after: $(man $M) dirs=$(ls $G)"
ok "reused Override/A.TGA" '[ "$(ls $G)" = Override ] && [ "$(cat $G/Override/A.TGA)" = NEW ]'
dep uninstall --component $A >/dev/null; ok "restored" '[ "$(cat $G/Override/A.TGA)" = ORIG ]'

echo "P12 corrupt manifest during status/purge"; new p12
stage $A Override/a.tga A; dep deploy --component $A >/dev/null; printf '{not json' > $M/manifests/dddddddd-0000-0000-0000-000000000004.json
echo "  before: $(man $M)"
dep status >$W/st 2>$W/err; rc1=$?; dep purge >$W/out 2>>$W/err; rc2=$?
echo "  status rc=$rc1 purge rc=$rc2 warnings=$(grep -c 'Failed to read' $W/err) after: $(man $M)"
ok "status and purge succeed" '[ $rc1 -eq 0 ] && [ $rc2 -eq 0 ]'
ok "good component purged, corrupt kept" '[ ! -e $G/Override/a.tga ] && [ -e $M/manifests/dddddddd-0000-0000-0000-000000000004.json ]'

echo "P13 GUID casing normalized"; new p13
stage $A Override/a.tga A; dep deploy --component "{${A^^}}" >/dev/null
ok "manifest file lowercase" '[ -e $M/manifests/$A.json ]'
dep uninstall --component "${A^^}" >$W/out; ok "uppercase uninstall finds it" 'grep -q "\"removed\": true" $W/out'

echo "P14 two profiles into one game dir, switch with purge"; new p14
CFG=$W/cfg; mkdir -p $CFG
$B profile --help >/dev/null 2>&1
$B profile --action create --settings-dir $CFG --name One >/dev/null 2>$W/perr
$B profile --action create --settings-dir $CFG --name Two >/dev/null 2>>$W/perr
ls $CFG/profiles
P1S=$CFG/profiles/One/staging; P2S=$CFG/profiles/Two/staging; mkdir -p $P1S/$A/Override $P2S/$Bc/Override
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da; printf ONE > $P1S/$A/Override/x.2da; printf TWO > $P2S/$Bc/Override/x.2da
$B deployment switch --game-dir $G --settings-dir $CFG --to One >/dev/null
$B deployment deploy --game-dir $G --settings-dir $CFG --component $A >/dev/null
echo "  One: $(man $CFG/profiles/One/deployment) live=$(cat $G/Override/x.2da)"
$B deployment switch --game-dir $G --settings-dir $CFG --to Two --purge-current >$W/sw; echo "  switch: $(python3 -c 'import json,sys;r=json.load(open(sys.argv[1]));print(r["from"],"->",r["to"],"purged",len(r["purged"]),"staged",r["stagedNotDeployed"])' $W/sw)"
ok "One purged, original live" '[ "$(cat $G/Override/x.2da)" = ORIG ] && [ -z "$(ls $CFG/profiles/One/deployment/manifests)" ]'
$B deployment deploy --game-dir $G --settings-dir $CFG --component $Bc >/dev/null
echo "  Two (active): $(man $CFG/profiles/Two/deployment) live=$(cat $G/Override/x.2da)"
ok "active profile resolves roots" '[ "$(cat $G/Override/x.2da)" = TWO ]'
$B deployment purge --game-dir $G --settings-dir $CFG --profile Two >/dev/null; ok "purge via --profile" '[ "$(cat $G/Override/x.2da)" = ORIG ]'

echo "P15 patcher provenance (snapshot/record)"; new p15
mkdir -p $G/Override; printf DIALOG0 > $G/dialog.tlk; printf KEEP > $G/Override/keep.2da
dep snapshot --out $W/before.json >/dev/null; cp -a $G $W/pre   # pre-patcher tree, like the recovery snapshot
printf DIALOG1 > $G/dialog.tlk; printf NEWFILE > $G/Override/new.2da   # "patcher" writes
dep record --component $A --name Patched --before $W/before.json --backup-source $W/pre >$W/out 2>$W/err
echo "  after record: $(man $M) backup=$(cat $M/backups/$A/dialog.tlk)"
ok "both recorded, dialog backed up" 'python3 -c "import json,sys;r=json.load(open(sys.argv[1]))[\"provenance\"];sys.exit(0 if sorted(r[\"recorded\"])==[\"Override/new.2da\",\"dialog.tlk\"] and r[\"backedUp\"]==[\"dialog.tlk\"] else 1)" $W/out'
dep uninstall --component $A >/dev/null
ok "uninstall restores pre-patcher dialog.tlk, removes new file" '[ "$(cat $G/dialog.tlk)" = DIALOG0 ] && [ ! -e $G/Override/new.2da ] && [ "$(cat $G/Override/keep.2da)" = KEEP ]'
printf DIALOG2 > $G/dialog.tlk; dep snapshot --out $W/b2.json >/dev/null; printf DIALOG3 > $G/dialog.tlk
dep record --component $C --before $W/b2.json >$W/out 2>$W/err
ok "modified pre-existing file without backup source is skipped" 'python3 -c "import json,sys;r=json.load(open(sys.argv[1]))[\"provenance\"];sys.exit(0 if r[\"skipped\"]==[\"dialog.tlk\"] and not r[\"recorded\"] else 1)" $W/out'

echo "P16 legacy uppercase manifest (written by the pre-normalization CLI)"; new p16
U=${A^^}; mkdir -p $G/Override $S/$U/Override $M/manifests $M/backups/$U/Override
printf ORIG > $M/backups/$U/Override/a.tga; printf NEW > $S/$U/Override/a.tga; ln $S/$U/Override/a.tga $G/Override/a.tga
H=$(sha256sum $S/$U/Override/a.tga | cut -d' ' -f1)
printf '{"ComponentGuid":"%s","DeployedUtc":"2026-09-27T10:00:00.0000000Z","Entries":[{"RelativePath":"Override/a.tga","SourceHash":"%s","Size":3,"DeploymentMethod":0,"OverwroteExisting":true,"BackupRelativePath":"Override/a.tga"}]}' $U $H > $M/manifests/$U.json
echo "  before: $(man $M)"
dep deploy --component $U >$W/out 2>$W/err; rc=$?; echo "  redeploy rc=$rc: $(tail -1 $W/err | cut -c1-90)"
ok "redeploy over legacy manifest refused" '[ $rc -ne 0 ] && [ "$(ls $M/manifests)" = "$U.json" ]'
dep purge >$W/out 2>$W/err; echo "  after purge: $(man $M) live=$(cat $G/Override/a.tga)"
ok "purge finds legacy manifest and restores" '[ "$(cat $G/Override/a.tga)" = ORIG ] && [ -z "$(ls $M/manifests)" ] && [ ! -e $M/backups/$U ]'

echo "P17 switch without --purge-current is refused while the current profile is live"; new p17
CFG=$W/cfg; $B profile --action create --settings-dir $CFG --name One >/dev/null; $B profile --action create --settings-dir $CFG --name Two >/dev/null
mkdir -p $CFG/profiles/One/staging/$A; printf X > $CFG/profiles/One/staging/$A/x.txt
$B deployment switch --game-dir $G --settings-dir $CFG --to One >/dev/null
$B deployment deploy --game-dir $G --settings-dir $CFG --component $A >/dev/null
$B deployment switch --game-dir $G --settings-dir $CFG --to Two >$W/out 2>$W/err; rc=$?; echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "refused, One still active, files in place" '[ $rc -ne 0 ] && grep -q "\"activeProfileName\": \"One\"" $CFG/settings.json && [ -e $G/x.txt ]'

echo "P18 a second profile cannot deploy into a game dir another profile still owns"; new p18
CFG=$W/cfg; $B profile --action create --settings-dir $CFG --name One >/dev/null; $B profile --action create --settings-dir $CFG --name Two >/dev/null
mkdir -p $G/Override $CFG/profiles/One/staging/$A/Override $CFG/profiles/Two/staging/$Bc/Override
printf ORIG > $G/Override/x.2da; printf ONE > $CFG/profiles/One/staging/$A/Override/x.2da; printf TWO > $CFG/profiles/Two/staging/$Bc/Override/x.2da
$B deployment deploy --game-dir $G --settings-dir $CFG --profile One --component $A >/dev/null
$B deployment deploy --game-dir $G --settings-dir $CFG --profile Two --component $Bc >$W/out 2>$W/err; rc=$?; echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "Two refused, ONE live" '[ $rc -ne 0 ] && [ "$(cat $G/Override/x.2da)" = ONE ]'
$B deployment purge --game-dir $G --settings-dir $CFG --profile One >/dev/null
$B deployment deploy --game-dir $G --settings-dir $CFG --profile Two --component $Bc >/dev/null; rc=$?
$B deployment purge --game-dir $G --settings-dir $CFG --profile Two >/dev/null
ok "after One is purged Two deploys, and purge gives ORIG" '[ $rc -eq 0 ] && [ "$(cat $G/Override/x.2da)" = ORIG ]'

echo "P19 case-colliding game directories are refused even with an exact spelling"; new p19
mkdir -p $G/Override $G/override; stage $A Override/a.tga A
dep deploy --component $A >$W/out 2>$W/err; rc=$?; echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "refused, nothing deployed" '[ $rc -ne 0 ] && [ -z "$(find $G -type f)" ] && [ -z "$(ls $M/manifests)" ]'

echo "P20 patcher changes a file the component already deployed"; new p20
printf ORIG > $G/dialog.tlk; stage $A dialog.tlk STAGED
dep deploy --component $A >/dev/null; dep snapshot --out $W/before.json >/dev/null; cp -a $G $W/pre
rm $G/dialog.tlk; printf PATCHED > $G/dialog.tlk      # patcher replaces the file
dep record --component $A --before $W/before.json --backup-source $W/pre >$W/out 2>$W/err
echo "  after record: $(man $M) staged=$(cat $S/$A/dialog.tlk)"
ok "owned path updated, staging kept in step" 'python3 -c "import json,sys;r=json.load(open(sys.argv[1]))[\"provenance\"];sys.exit(0 if r[\"updated\"]==[\"dialog.tlk\"] else 1)" $W/out && [ "$(cat $S/$A/dialog.tlk)" = PATCHED ]'
dep uninstall --component $A >/dev/null
ok "uninstall removes patcher output and restores ORIG" '[ "$(cat $G/dialog.tlk)" = ORIG ] && [ -z "$(ls $M/manifests)" ]'

echo "P21 provenance on an older component keeps ownership order"; new p21
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da; stage $A Override/x.2da A; stage $Bc Override/x.2da B
dep deploy --component $A >/dev/null; sleep 0.05; dep deploy --component $Bc >/dev/null 2>&1
before=$(grep -h DeployedUtc $M/manifests/$A.json)
dep snapshot --out $W/before.json >/dev/null; printf NEW > $G/Override/new.2da
dep record --component $A --before $W/before.json >/dev/null 2>&1
ok "record_live left A's DeployedUtc unchanged" '[ "$(grep -h DeployedUtc $M/manifests/$A.json)" = "$before" ]'
dep uninstall --component $A >$W/out 2>&1; dep uninstall --component $Bc >/dev/null
ok "handoff still works: final bytes ORIG, no orphan backups" '[ "$(cat $G/Override/x.2da)" = ORIG ] && [ -z "$(find $M/backups -type f)" ]'

echo "P22 deployment refuses while another ModSync process holds the game lock"; new p22
stage $A a.txt A
LOCK=$(dirname $G)/.modsync-lock-$(printf %s "$(realpath $G)" | sha256sum | cut -c1-16)
flock "$LOCK" sleep 3 & sleep 0.5
dep deploy --component $A >$W/out 2>$W/err; rc=$?; wait
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "refused while locked, nothing deployed" '[ $rc -ne 0 ] && [ ! -e $G/a.txt ]'
dep deploy --component $A >/dev/null; ok "deploys once the lock is free" '[ -e $G/a.txt ]'

echo "P23 a pre-patcher copy that does not match the snapshot is not used as a backup"; new p23
printf DIALOG0 > $G/dialog.tlk; dep snapshot --out $W/before.json >/dev/null
mkdir -p $W/pre; printf WRONG > $W/pre/dialog.tlk; printf DIALOG1 > $G/dialog.tlk
dep record --component $A --before $W/before.json --backup-source $W/pre >$W/out 2>$W/err; rc=$?
ok "skipped, no backup left behind" '[ $rc -eq 0 ] && python3 -c "import json,sys;r=json.load(open(sys.argv[1]))[\"provenance\"];sys.exit(0 if r[\"skipped\"]==[\"dialog.tlk\"] else 1)" $W/out && [ -z "$(find $M/backups -type f)" ]'

echo "P24 uninstall uses the recorded spelling after a case variant appears"; new p24
mkdir -p $G/Override; printf ORIG > $G/Override/a.tga; stage $A Override/a.tga A
dep deploy --component $A >/dev/null; mkdir $G/override
dep uninstall --component $A >$W/out 2>$W/err; rc=$?
ok "original restored, manifest removed" '[ $rc -eq 0 ] && [ "$(cat $G/Override/a.tga)" = ORIG ] && [ -z "$(ls $M/manifests)" ] && [ -z "$(find $M/backups -type f)" ]'

echo "P25 provenance refuses a path a newer component owns, nothing mutated"; new p25
mkdir -p $G/Override; printf ORIGX > $G/Override/x.2da; printf ORIGY > $G/Override/y.2da
stage $A Override/x.2da A; stage $Bc Override/y.2da B
dep deploy --component $A >/dev/null; sleep 0.05; dep deploy --component $Bc >/dev/null
dep snapshot --out $W/before.json >/dev/null; cp -a $G $W/pre
rm $G/Override/y.2da; printf PATCHED > $G/Override/y.2da
before_a=$(cat $M/manifests/$A.json)
dep record --component $A --before $W/before.json --backup-source $W/pre >$W/out 2>$W/err; rc=$?
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"; echo "  after record: $(man $M)"
ok "refused (nonzero exit), not skip-with-warning" '[ $rc -ne 0 ] && grep -q "y.2da" $W/err'
ok "A's manifest untouched, not recorded, no backup made" '[ "$(cat $M/manifests/$A.json)" = "$before_a" ] && ! grep -q y.2da $M/manifests/$A.json && [ ! -e $M/backups/$A/Override/y.2da ]'

echo "P25b provenance refuses this component's own path changed before the patcher, with no prior copy given"; new p25b
printf DIALOG0 > $G/dialog.tlk; stage $A dialog.tlk STAGED
dep deploy --component $A >/dev/null   # dialog.tlk is now STAGED, owned by A (hardlinked to staging)
rm $G/dialog.tlk; printf TAMPERED > $G/dialog.tlk   # replace (not edit-in-place) so the staged hardlink keeps STAGED
before_a=$(cat $M/manifests/$A.json)
dep snapshot --out $W/before.json >/dev/null   # captures TAMPERED, not the owned STAGED hash
rm $G/dialog.tlk; printf PATCHED > $G/dialog.tlk   # "patcher" writes over the tampered bytes
dep record --component $A --before $W/before.json >$W/out 2>$W/err; rc=$?
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"; echo "  after record: $(man $M)"
ok "refused (nonzero exit), staging and manifest untouched" '[ $rc -ne 0 ] && [ "$(cat $M/manifests/$A.json)" = "$before_a" ] && [ "$(cat $S/$A/dialog.tlk)" = STAGED ]'

echo "P25c explicit --manifest-dir roots enforce the same game-wide exclusivity as profiles"; new p25c
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da
M2=$W/managed2; mkdir -p $M2
dep2(){ $B deployment "$@" --game-dir $G --staging-dir $S --manifest-dir $M2; }
stage $A Override/x.2da ONE
dep deploy --component $A >/dev/null; echo "  root1 live: $(cat $G/Override/x.2da)"
stage $Bc Override/x.2da TWO
dep2 deploy --component $Bc >$W/out 2>$W/err; rc=$?
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-150)"
ok "second explicit root refused while first still owns the game dir" '[ $rc -ne 0 ] && [ "$(cat $G/Override/x.2da)" = ONE ] && [ -z "$(ls $M2/manifests 2>/dev/null)" ]'
dep purge >/dev/null
dep2 deploy --component $Bc >$W/out2 2>$W/err2; rc2=$?
echo "  after root1 purge, root2 rc=$rc2: live=$(cat $G/Override/x.2da 2>/dev/null)"
ok "ownership transfers once the first root has no manifests left" '[ $rc2 -eq 0 ] && [ "$(cat $G/Override/x.2da)" = TWO ]'
dep2 uninstall --component $Bc >/dev/null

echo "P25d a marker-less legacy manifest root is refused against the wrong game directory"; new p25d
mkdir -p $G/Override $M/manifests; printf ORIG > $G/Override/a.tga
printf '{"ComponentGuid":"%s","DeployedUtc":"2026-09-27T10:00:00.0000000Z","Entries":[{"RelativePath":"Override/a.tga","SourceHash":"deadbeef","Size":3,"DeploymentMethod":0,"OverwroteExisting":false,"BackupRelativePath":null}]}' $A > $M/manifests/$A.json
echo "  before: $(man $M) (no game-directory marker, hash does not match this game dir)"
dep status >$W/out 2>$W/err; rc=$?
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-150)"
ok "status still succeeds (read-only, no ensure_own_game gate)" '[ $rc -eq 0 ]'
dep uninstall --component $A >$W/out2 2>$W/err2; rc2=$?
echo "  uninstall rc=$rc2: $(tail -1 $W/err2 | cut -c1-150)"
ok "marker-less root refused: no owned entry hash-matches this game dir" '[ $rc2 -ne 0 ] && [ -e $M/manifests/$A.json ] && [ "$(cat $G/Override/a.tga)" = ORIG ]'

echo "P26 a failed backup handoff leaves both backups and both manifests"; new p26
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da; stage $A Override/x.2da A; stage $Bc Override/x.2da B
dep deploy --component $A >/dev/null; sleep 0.05; dep deploy --component $Bc >/dev/null 2>&1
chmod 555 $M/backups/$Bc/Override
dep uninstall --component $A >$W/out 2>$W/err; rc=$?; chmod 755 $M/backups/$Bc/Override
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "refused; B's slot still holds A, A still holds ORIG" '[ $rc -ne 0 ] && [ "$(cat $M/backups/$Bc/Override/x.2da)" = A ] && [ "$(cat $M/backups/$A/Override/x.2da)" = ORIG ] && [ -e $M/manifests/$A.json ] && [ -e $M/manifests/$Bc.json ]'
dep uninstall --component $A >/dev/null; dep uninstall --component $Bc >/dev/null
ok "retry hands off; final bytes ORIG" '[ "$(cat $G/Override/x.2da)" = ORIG ] && [ -z "$(ls $M/manifests)" ] && [ -z "$(find $M/backups -type f)" ]'

echo "P27 a failed provenance record removes the backups it made"; new p27
mkdir -p $G/sub; printf A0 > $G/a.2da; printf X0 > $G/sub/x.2da
dep snapshot --out $W/before.json >/dev/null; cp -a $G $W/pre; printf A1 > $G/a.2da; printf X1 > $G/sub/x.2da
mkdir -p $M/backups/$A/sub; chmod 555 $M/backups/$A/sub
dep record --component $A --before $W/before.json --backup-source $W/pre >$W/out 2>$W/err; rc=$?
chmod -R u+w $M 2>/dev/null; echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "failed, no backup files, no manifest" '[ $rc -ne 0 ] && [ -z "$(find $M/backups -type f)" ] && [ ! -e $M/manifests/$A.json ]'
dep record --component $A --before $W/before.json --backup-source $W/pre >$W/out 2>$W/err; rc=$?
ok "retry records both with backups" '[ $rc -eq 0 ] && python3 -c "import json,sys;r=json.load(open(sys.argv[1]))[\"provenance\"];sys.exit(0 if sorted(r[\"backedUp\"])==[\"a.2da\",\"sub/x.2da\"] else 1)" $W/out'
dep uninstall --component $A >/dev/null
ok "uninstall restores both originals" '[ "$(cat $G/a.2da)" = A0 ] && [ "$(cat $G/sub/x.2da)" = X0 ]'

echo "P28 a manifest root refuses a different game directory"; new p28
stage $A a.txt A; dep deploy --component $A >/dev/null; mkdir -p $W/game2
$B deployment uninstall --component $A --game-dir $W/game2 --staging-dir $S --manifest-dir $M >$W/out 2>$W/err; rc=$?
echo "  rc=$rc: $(tail -1 $W/err | cut -c1-120)"
ok "refused, manifest and file kept" '[ $rc -ne 0 ] && [ -e $M/manifests/$A.json ] && [ -e $G/a.txt ] && [ "$(cat $M/game-directory)" = "$(realpath $G)" ]'

echo "P29 a root deployed by an older build (no owner-record support) adopts the record on its next command, closing the window for a second explicit root"; new p29
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da
OWNER=$(dirname $G)/.modsync-owner-$(printf %s "$(realpath $G)" | sha256sum | cut -c1-16)
OLDB=${OLDB:-/tmp/depprobe/modsync-nodefault-b1c60b1}
if [ -x "$OLDB" ]; then
  depold(){ $OLDB deployment "$@" --game-dir $G --staging-dir $S --manifest-dir $M; }
  stage $A Override/x.2da OLD
  depold deploy --component $A >$W/oldout 2>$W/olderr; rc=$?
  echo "  old-binary deploy rc=$rc: $(man $M)"
  ok "old binary deployed with a marker but no owner record" '[ $rc -eq 0 ] && [ -e $M/game-directory ] && [ ! -e "$OWNER" ]'
  dep uninstall --component $C >$W/out 2>$W/err   # bogus component: touches ensure_own_game only, adopts
  echo "  after fixed-binary touch: owner=$(cat "$OWNER" 2>/dev/null || echo MISSING)"
  ok "fixed binary adopted the owner record for this root" '[ -e "$OWNER" ] && [ "$(cat "$OWNER")" = "$(realpath $M)" ]'
  M2=$W/managed2; mkdir -p $M2
  dep2(){ $B deployment "$@" --game-dir $G --staging-dir $S --manifest-dir $M2; }
  stage $Bc Override/x.2da NEW
  dep2 deploy --component $Bc >$W/out2 2>$W/err2; rc2=$?
  echo "  second explicit root rc=$rc2: $(tail -1 $W/err2 | cut -c1-150)"
  ok "second root refused; original root's file untouched" '[ $rc2 -ne 0 ] && [ "$(cat $G/Override/x.2da)" = OLD ] && [ -z "$(ls $M2/manifests 2>/dev/null)" ]'
  dep uninstall --component $A >/dev/null
else
  echo "  FAIL: OLDB not found or not executable ($OLDB); legacy upgrade proof requires the old binary"
  fail=$((fail+1))
fi

echo "P30 a marker-less root with one matching and one mismatching entry refuses, and the untouched game dir stays untouched"; new p30
mkdir -p $G/Override $W/game2/Override
printf ORIG1 > $G/Override/a.tga; printf WRONGB > $G/Override/b.tga
printf ORIG1 > $W/game2/Override/a.tga; printf REALB > $W/game2/Override/b.tga
hA=$(sha256sum $W/game2/Override/a.tga | cut -d' ' -f1); hB=$(sha256sum $W/game2/Override/b.tga | cut -d' ' -f1)
mkdir -p $M/manifests
printf '{"ComponentGuid":"%s","DeployedUtc":"2026-09-27T10:00:00.0000000Z","Entries":[{"RelativePath":"Override/a.tga","SourceHash":"%s","Size":5,"DeploymentMethod":0,"OverwroteExisting":false,"BackupRelativePath":null},{"RelativePath":"Override/b.tga","SourceHash":"%s","Size":5,"DeploymentMethod":0,"OverwroteExisting":false,"BackupRelativePath":null}]}' $A $hA $hB > $M/manifests/$A.json
echo "  manifest entries: a.tga matches both dirs by coincidence, b.tga only matches game2"
dep uninstall --component $A >$W/out 2>$W/err; rc=$?
echo "  rc=$rc against the wrong dir: $(tail -1 $W/err | cut -c1-150)"
ok "refused, names the first mismatch, wrong dir untouched" '[ $rc -ne 0 ] && grep -q "b.tga" $W/err && [ "$(cat $G/Override/a.tga)" = ORIG1 ] && [ "$(cat $G/Override/b.tga)" = WRONGB ] && [ -e $M/manifests/$A.json ]'
ok "the real game dir was never touched" '[ "$(cat $W/game2/Override/a.tga)" = ORIG1 ] && [ "$(cat $W/game2/Override/b.tga)" = REALB ]'

echo "P31 the same marker-less root is accepted once every owned entry matches (the real game dir)"; new31_G=$W/game2
dep31(){ $B deployment "$@" --game-dir $new31_G --staging-dir $S --manifest-dir $M; }
dep31 uninstall --component $A >$W/out3 2>$W/err3; rc3=$?
echo "  rc=$rc3 against the real dir: $(man $M)"
ok "accepted: both entries matched, manifest consumed, files removed" '[ $rc3 -eq 0 ] && [ ! -e $W/game2/Override/a.tga ] && [ ! -e $W/game2/Override/b.tga ] && [ -z "$(ls $M/manifests)" ]'

echo "P32 a marker-less legacy root with overlapping deployments purges newest first"; new p32
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da
stage $A Override/x.2da FIRST; stage $Bc Override/x.2da SECOND
dep deploy --component $A >$W/deploy-a 2>$W/deploy-a.err; rc_a=$?
sleep 0.05
dep deploy --component $Bc >$W/deploy-b 2>$W/deploy-b.err; rc_b=$?
OWNER=$(dirname $G)/.modsync-owner-$(printf %s "$(realpath $G)" | sha256sum | cut -c1-16)
rm -f "$M/game-directory" "$OWNER"
dep purge >$W/out 2>$W/err; rc=$?
ok "overlapping legacy manifests accepted and original restored" '[ $rc_a -eq 0 ] && [ $rc_b -eq 0 ] && [ $rc -eq 0 ] && [ "$(cat $G/Override/x.2da)" = ORIG ] && [ -z "$(ls $M/manifests)" ] && [ -z "$(find $M/backups -type f)" ]'

echo "P33 a marker-less root that adopted ownership blocks a second explicit root"; new p33
printf ORIG > $G/x.txt; stage $A x.txt FIRST; stage $Bc x.txt SECOND
dep deploy --component $A >$W/deploy-a 2>$W/deploy-a.err; rc_a=$?
OWNER=$(dirname $G)/.modsync-owner-$(printf %s "$(realpath $G)" | sha256sum | cut -c1-16)
rm -f "$M/game-directory" "$OWNER"
dep uninstall --component $C >$W/touch 2>$W/touch.err; rc_touch=$?
M2=$W/managed2; mkdir -p "$M2"
$B deployment deploy --component $Bc --game-dir "$G" --staging-dir "$S" --manifest-dir "$M2" >$W/out 2>$W/err; rc=$?
ok "legacy claim adopted and second root refused without changing live bytes" '[ $rc_a -eq 0 ] && [ $rc_touch -eq 0 ] && [ -e "$OWNER" ] && [ $rc -ne 0 ] && [ "$(cat $G/x.txt)" = FIRST ] && [ -e $M/manifests/$A.json ] && [ -z "$(ls $M2/manifests)" ]'
dep purge >$W/purge 2>$W/purge.err; rc_purge=$?
ok "legacy owner still restores its original" '[ $rc_purge -eq 0 ] && [ "$(cat $G/x.txt)" = ORIG ]'

echo "P34 an empty root does not adopt ownership on a no-op or failed deploy"; new p34
OWNER=$(dirname $G)/.modsync-owner-$(printf %s "$(realpath $G)" | sha256sum | cut -c1-16)
dep uninstall --component $A >$W/noop 2>$W/noop.err; rc_noop=$?
ok "no-op uninstall leaves no owner record" '[ $rc_noop -eq 0 ] && [ ! -e "$OWNER" ]'
dep deploy --component $A >$W/missing 2>$W/missing.err; rc_missing=$?
ok "missing staging refuses without claiming ownership" '[ $rc_missing -ne 0 ] && [ ! -e "$OWNER" ]'
printf ORIG > $G/x.txt; stage $A x.txt FIRST
dep deploy --component $A >$W/deploy 2>$W/deploy.err; rc_deploy=$?
ok "first successful deployment retains its ownership warning" '[ $rc_deploy -eq 0 ] && grep -q "could not be verified" $W/deploy.err'

 echo "P35 mixed legacy path separators identify one live-owned path"; new p35
mkdir -p $G/Override; printf ORIG > $G/Override/x.2da
stage $A Override/x.2da FIRST; stage $Bc Override/x.2da SECOND
dep deploy --component $A >$W/deploy-a 2>$W/deploy-a.err
sleep 0.05
dep deploy --component $Bc >$W/deploy-b 2>$W/deploy-b.err
python3 - "$M/manifests/$A.json" <<'PYSEP'
import json,sys
p=sys.argv[1];m=json.load(open(p))
for entry in m['Entries']:
    entry['RelativePath']=entry['RelativePath'].replace('/',chr(92))
open(p,'w').write(json.dumps(m))
PYSEP
OWNER=$(dirname $G)/.modsync-owner-$(printf %s "$(realpath $G)" | sha256sum | cut -c1-16)
rm -f "$M/game-directory" "$OWNER"
dep purge >$W/out 2>$W/err; rc=$?
ok "mixed separator legacy overlap restores original" '[ $rc -eq 0 ] && [ "$(cat $G/Override/x.2da)" = ORIG ] && [ -z "$(ls $M/manifests)" ]'

echo; echo "TOTAL pass=$pass fail=$fail"
[ "$fail" -eq 0 ] || exit 1
