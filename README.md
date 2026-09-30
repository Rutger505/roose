# roose 🪿

A desktop goose for Wayland, written in Rust. Built for Hyprland. It is **r**ust + g**oose**.

The goose:

- waddles around your screen (a static sprite that bobs and flips, no animation frames)
- chases your cursor, grabs it in its beak and runs away with it
- drags in a VSCode window from the edge of the screen with a funny quote in it
- drags in memes shown with [imv](https://sr.ht/~exec64/imv/)
- is click-through, so it never blocks your clicks

## Install

```sh
cargo install --git https://github.com/Rutger505/roose
```

Runtime dependencies: Hyprland, `code` (VSCode) and `imv`. Any other editor or viewer works through `--editor` and `--viewer`.

## Usage

```sh
roose                # one goose
roose & roose & roose  # a flock
pkill roose          # peace
```

Put images in `~/Pictures/roose` (or pass `--memes <dir>`) and the goose will bring them to you. Without any, it brings a picture of itself.

```
--size <px>        goose width in logical pixels [default: 80]
--speed <factor>   movement speed multiplier [default: 1.0]
--fps <n>          ticks per second while moving [default: 30]
--cooldown <s>     minimum seconds between fetched windows [default: 45]
--sprite <png>     use your own goose image (should face right)
--memes <dir>      folder with images to drag in [default: ~/Pictures/roose]
--editor <cmd>     command for notes [default: "code --new-window"]
--viewer <cmd>     command for memes [default: imv]
--no-steal         never steal the cursor
--no-notes         never drag in notes
--no-memes         never drag in memes
```

To start it with Hyprland, add `exec-once = roose` to `hyprland.conf`.

On compositors other than Hyprland (sway, river, ...) the goose still walks around, but it can't touch your cursor or windows. That needs Hyprland's IPC.

## Why it's cheap to run many geese

Each goose is its own small process, and it's built so that running many of them stays cheap:

- **Rendered once.** The sprite is drawn once per output scale into shared memory, one copy per facing direction. Walking never redraws: each frame is just `wl_subsurface.set_position` plus a commit.
- **No re-arrange storm.** The obvious way to move a layer-shell surface is to change its margins. That makes the compositor re-arrange *every* layer surface on the monitor and send each one a `configure`. With N geese that is O(N²) wakeups: in testing, one goose received 570 configures for its own 40 commits. roose keeps its layer surface as a static, transparent 1×1 anchor and moves the goose as a subsurface instead, so it gets 2 configures in total.
- **Sleeps when idle.** The main loop is timer-driven. A standing goose sleeps until its next move, with no frame loop.
- **Talks to Hyprland directly.** It writes to Hyprland's IPC socket instead of spawning `hyprctl` for every cursor or window move.
- **Small.** No GPU context and no toolkit, just `wl_shm` and `tiny-skia`, with an LTO'd, stripped release binary.

### Benchmark

`scripts/bench.sh [instances] [seconds]` starts N geese on your current session and reports their combined memory and CPU. `HEADLESS=1` runs them in a throwaway headless sway instead.

Headless sway, 4-core VM, 30s per run, geese wandering (release build):

- 1 goose: 1.5 MiB PSS, 0.10% of one core
- 10 geese: 4.8 MiB PSS in total (0.5 MiB each), 1.10% of one core
- 50 geese: 16.7 MiB PSS in total (0.3 MiB each), 5.27% of one core

CPU grows linearly, about 0.1% per goose. Memory per goose *drops* as you add more, because the code and libraries are shared between processes. With the margin-based approach, 25 geese used 19.5% of a core. With subsurfaces they use 1.5%.

## Development

```sh
cargo test
cargo clippy --all-targets
HEADLESS=1 scripts/bench.sh 10 30
```

- `src/brain.rs`: the goose's behaviour state machine. It's pure logic and unit-tested against a fake desktop.
- `src/desktop.rs`: the Hyprland side (cursor, spawning and dragging windows).
- `src/hypr.rs`: a minimal Hyprland IPC client.
- `src/sprite.rs`: the built-in goose, drawn with tiny-skia, or a PNG you provide.
- `src/main.rs`: Wayland wiring (layer-shell anchor + subsurface).
