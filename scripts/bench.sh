#!/usr/bin/env bash
# Usage: scripts/bench.sh [instances] [seconds]
# Runs N geese on the current Wayland session and reports their combined RAM and CPU usage.
# Set HEADLESS=1 to spin up a throwaway headless sway instead (needs sway + pixman renderer).
set -eu
count=${1:-10}
seconds=${2:-30}
root=$(cd "$(dirname "$0")/.." && pwd)
bin=$root/target/release/roose
cargo build --release --quiet --manifest-path "$root/Cargo.toml"

cleanup() { kill "${pids[@]}" ${sway_pid:-} 2>/dev/null || true; }
trap cleanup EXIT

if [ "${HEADLESS:-0}" = 1 ]; then
  unset HYPRLAND_INSTANCE_SIGNATURE
  before=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' || true)
  printf 'output HEADLESS-1 resolution 1920x1080\n' >"$root/target/bench-sway.conf"
  WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$root/target/bench-sway.conf" >/dev/null 2>&1 &
  sway_pid=$!
  for _ in $(seq 50); do
    socket=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | grep -vxF "$before" | tail -1 || true)
    [ -n "$socket" ] && break
    sleep 0.1
  done
  export WAYLAND_DISPLAY=$socket
fi

pids=()
for _ in $(seq "$count"); do
  "$bin" --no-notes --no-memes --no-steal &
  pids+=($!)
done

cpu_ticks() { awk '{ print $14 + $15 }' "/proc/$1/stat"; }
start=()
for pid in "${pids[@]}"; do start+=("$(cpu_ticks "$pid")"); done
sleep "$seconds"

hz=$(getconf CLK_TCK)
ticks=0
rss=0
pss=0
for i in "${!pids[@]}"; do
  pid=${pids[$i]}
  ticks=$((ticks + $(cpu_ticks "$pid") - ${start[$i]}))
  rss=$((rss + $(awk '/VmRSS/ { print $2 }' "/proc/$pid/status")))
  pss=$((pss + $(awk '/^Pss:/ { print $2 }' "/proc/$pid/smaps_rollup")))
done

awk -v n="$count" -v s="$seconds" -v t="$ticks" -v hz="$hz" -v rss="$rss" -v pss="$pss" 'BEGIN {
  cpu = 100 * t / hz / s
  printf "geese:           %d\n", n
  printf "total RSS:       %.1f MiB (%.1f MiB each)\n", rss / 1024, rss / 1024 / n
  printf "total PSS:       %.1f MiB (%.1f MiB each, shared libraries split fairly)\n", pss / 1024, pss / 1024 / n
  printf "total CPU:       %.2f%% of one core (%.3f%% each)\n", cpu, cpu / n
}'
