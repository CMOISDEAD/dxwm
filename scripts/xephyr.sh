#!/usr/bin/env sh

# Run the release build of dxwm inside a nested Xephyr server.
#
# usage (from the repo root):
#   make run-xephyr
#   SCREEN_SIZE=1600x900 XDISPLAY=:8 APP=alacritty sh scripts/xephyr.sh
#
# XDISPLAY must not be the display of the host session.
# Logs go to xephyr.log in the repo root.
CUR_DIR="$(dirname $(readlink -f $0))"
SCREEN_SIZE=${SCREEN_SIZE:-1200x900}
XDISPLAY=${XDISPLAY:-:7}
APP=${APP:-st}

touch $CUR_DIR/../xephyr.log

Xephyr +extension RANDR -screen ${SCREEN_SIZE} ${XDISPLAY} -ac &
XEPHYR_PID=$!

sleep 1
env DISPLAY=${XDISPLAY} "$CUR_DIR/../target/release/dxwm" >>"$CUR_DIR/../xephyr.log" 2>&1 &
WM_PID=$!

trap "kill $XEPHYR_PID && kill $WM_PID && rm $CUR_DIR/../xephyr.log" SIGINT SIGTERM exit

env DISPLAY=${XDISPLAY} ${APP} &

tail -f $CUR_DIR/../xephyr.log

wait $WM_PID
kill $XEPHYR_PID
