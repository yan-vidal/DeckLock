#!/usr/bin/env bash
# Started by run.sh inside a private Xvfb and D-Bus session:
#   scenario.sh <out-dir> <none|xfwm4|xfwm4-comp> <bin-dir>
set -u
OUT=$1
WM=$2
BIN=$3
mkdir -p "$OUT" && cd "$OUT" || exit 1
unset WAYLAND_DISPLAY WAYLAND_SOCKET
export GDK_BACKEND=x11 GTK_A11Y=none NO_AT_BRIDGE=1
[[ -n ${DISPLAY:-} && -z ${WAYLAND_DISPLAY:-} && $XDG_RUNTIME_DIR == */x11-probe/runtime.* ]] || { echo "refusing: run through run.sh"; exit 1; }

wait_for() { # pattern file seconds
    local i
    for ((i = 0; i < $3 * 10; i++)); do
        grep -q "$1" "$2" 2>/dev/null && return 0
        sleep 0.1
    done
    echo "TIMEOUT waiting for '$1' in $2"
    return 1
}

case $WM in
    xfwm4) xfwm4 --compositor=off > wm.log 2>&1 & ;;
    xfwm4-comp) xfwm4 --compositor=on --vblank=off > wm.log 2>&1 & ;;
esac
sleep 2
echo "== wm=$WM"
"$BIN/intruder" cm

"$BIN/lock" > lock.log 2>&1 &
LOCK=$!
wait_for 'grabs-held\|grab-gave-up' lock.log 10
wait_for 'LOCK report' lock.log 5
XID=$(grep -o 'xid=0x[0-9a-f]*' lock.log | head -1 | cut -d= -f2)
grep -E 'realized|grab|report' lock.log
xwd -root -silent | magick xwd:- locked.png 2>/dev/null

echo "== another client tries to grab"
"$BIN/intruder" grab
echo "== stacking at rest"
"$BIN/intruder" stack "$XID"

echo "== typing a wrong password while a keylogger listens"
"$BIN/intruder" keylog 4 > keylog.log &
KL=$!
sleep 0.5
"$BIN/intruder" type hunter Return
wait $KL
cat keylog.log
sleep 0.3
grep 'entry-activate' lock.log

echo "== managed window mapped and raised"
"$BIN/intruder" window 3 &
W=$!
sleep 1.5
"$BIN/intruder" stack "$XID"
xwd -root -silent | magick xwd:- raised-managed.png 2>/dev/null
wait $W
echo "== override-redirect window mapped and raised every 500ms"
"$BIN/intruder" window 3 override &
W=$!
sleep 1.5
"$BIN/intruder" stack "$XID"
xwd -root -silent | magick xwd:- raised-override.png 2>/dev/null
wait $W
grep -c 'raised-on' lock.log | sed 's/^/raise-reactions=/'

echo "== mask after raises"
"$BIN/intruder" mask "$XID"
echo "== entry context menu (popover) opened and closed"
"$BIN/intruder" type F2
sleep 1
"$BIN/intruder" grab
xwd -root -silent | magick xwd:- popover.png 2>/dev/null
"$BIN/intruder" type Escape
sleep 0.7
"$BIN/intruder" grab

echo "== focus now"
"$BIN/intruder" stack "$XID"
echo "== correct password after popover"
"$BIN/intruder" type secret Return
sleep 1
grep -E 'entry-activate|unlocked|focus-|popover' lock.log
grep 'fps=' lock.log | sed -n '2p;$p'
kill $LOCK 2>/dev/null
wait $LOCK 2>/dev/null

echo "== process death"
"$BIN/lock" > death.log 2>&1 &
LOCK=$!
wait_for 'grabs-held\|grab-gave-up' death.log 10
kill -9 $LOCK
wait $LOCK 2>/dev/null
sleep 0.5
"$BIN/intruder" grab
