# roose 🪿

A desktop goose for Hyprland, written in Rust. It is **r**ust + g**oose**.

The goose:

- waddles around your screen (a static sprite that bobs and flips, no animation frames)
- chases your cursor, grabs it in its beak and runs away with it
- drags in a VSCode window from the edge of the screen with a funny quote in it
- drags in memes shown with [imv](https://sr.ht/~exec64/imv/)
- is click-through, so it never blocks your clicks

## Install

```sh
cargo install --force --git https://github.com/Rutger505/roose
```

Runtime dependencies: Hyprland, `code` (VSCode) and `imv`. roose refuses to start outside Hyprland. Any other editor or viewer works through `--editor` and `--viewer`.

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
--editor <cmd>     command for notes [default: "code --new-window --ozone-platform=wayland"]
--viewer <cmd>     command for memes [default: imv]
--no-steal         never steal the cursor
--no-notes         never drag in notes
--no-memes         never drag in memes
```

To start it with Hyprland, add `exec-once = roose` to `hyprland.conf`.

## Why it's cheap to run many geese

Each goose is its own small process, and it's built so that running many of them stays cheap:

- **Rendered once.** The sprite is drawn once per output scale into shared memory, one copy per facing direction. Walking never redraws: each frame is just `wl_subsurface.set_position` plus a commit.
- **No re-arrange storm.** The obvious way to move a layer-shell surface is to change its margins. On every margin change Hyprland calls `arrangeLayersForMonitor`, which re-arranges *every* layer surface on the monitor and sends each one a `configure`. With N geese that is O(N²) wakeups. roose keeps its layer surface as a static, transparent 1×1 anchor and moves the goose as a subsurface instead. Hyprland tracks layer subsurfaces itself and damages the old and new spot, and nothing gets re-arranged.
- **Sleeps when idle.** The main loop is timer-driven. A standing goose sleeps until its next move, with no frame loop.
- **Talks to Hyprland directly.** It writes to Hyprland's IPC socket instead of spawning `hyprctl` for every cursor or window move.
- **Small.** No GPU context and no toolkit, just `wl_shm` and `tiny-skia`, with an LTO'd, stripped release binary.

### Benchmark

`scripts/bench.sh [instances] [seconds]` starts N geese on your current Hyprland session and reports their combined memory and CPU.

Run it with 1, 10 and 50 geese: CPU should grow linearly, and memory per goose should drop as you add more, because the code and libraries are shared between processes.

## Development

```sh
cargo test
cargo clippy --all-targets
scripts/bench.sh 10 30
```

- `src/brain.rs`: the goose's behaviour state machine. It's pure logic and unit-tested against a fake desktop.
- `src/desktop.rs`: the Hyprland side (cursor, spawning and dragging windows).
- `src/hypr.rs`: a minimal Hyprland IPC client.
- `src/sprite.rs`: the built-in goose, drawn with tiny-skia, or a PNG you provide.
- `src/main.rs`: Wayland wiring (layer-shell anchor + subsurface).
