# Claude desktop companion

A small pixel-art Claude that lives on your Linux desktop. It stays on top of every window, has a transparent background and no frame, and gets on with its own little life: it walks along the bottom of the screen, climbs up the sides, hangs from the top, sits, thinks, naps, and reacts when you play with it. While you're working it keeps to quiet things. While you're away it may come and play with your mouse pointer, and it hands the pointer back the instant you return.

## Install (once)

```bash
./companion-install
```

The installer:

1. Detects your distribution and desktop session.
2. Checks for the system libraries and build tools the companion needs. If any are missing, it shows the exact `sudo` package command and asks before running it. It never changes anything else on your system.
3. Checks for Rust 1.85 or newer. If Rust is missing, it offers the official installer (rustup). That installs into your home folder, needs no `sudo`, and leaves your shell profile alone.
4. Builds the app in release mode. The first build downloads its Rust libraries and takes about a minute.
5. Makes the scripts executable and checks that the built app starts and draws all its animations.

Run it again any time. Finished steps are skipped, and an unchanged build is skipped in under a second. Add `--yes` to accept the install questions automatically, or `--gnome-extension` to also set up the optional GNOME extension (see below).

## Start and stop

```bash
./companion
```

- **Stop:** press `Ctrl+C` in that terminal, or close the terminal.
- **Restart:** run `./companion` again.
- **Run it without keeping a terminal open:** `nohup ./companion >/dev/null 2>&1 &`. Stop it later with `pkill claude-mascot`.

`./companion` works from any folder (for example `~/claude-companion/companion`) and through a symlink. Nothing in the project uses fixed paths, so you can move the folder. The sprites are built into the app, so it needs no other files at runtime.

## Rebuild after changing the code

```bash
./companion-install
```

The same command rebuilds only what changed. If you start `./companion` after editing the code without rebuilding, it reminds you and runs the previous build.

## Update or reinstall dependencies

- **Rust:** `rustup update stable`, then `./companion-install`.
- **Rust libraries:** they are pinned in `Cargo.lock`, so builds are reproducible. To move to newer compatible versions, run `cargo update`, then `./companion-install`.
- **System libraries:** your distribution's normal updates cover them. If one was removed, `./companion-install` notices and offers to put it back.
- **A completely fresh build:** `rm -rf target && ./companion-install`.

## Playing with it

| You | Claude |
|---|---|
| Drag it | Dangles from the pointer, swinging with your movement |
| Throw it (drag fast and let go) | Flies, bounces off the screen edges, tumbles if thrown hard, and lands dizzy |
| Click | Happy hop. If it's asleep, it wakes up surprised first |
| Double-click | Waves |
| Rest the pointer on it for 1.5 s | Blushes and floats hearts (petting) |
| Right-click | Switches to the next activity and holds it until your next left click |

It is only clickable on the character itself. The transparent area around it lets clicks through to the windows below.

### What it does on its own

It has energy that drains while it's awake and refills when it naps, a need for attention that grows when nobody plays with it, and a slightly different curiosity and playfulness on each launch. From those it picks what to do next, rarely the same thing twice in a row.

- **While you're working** (the mouse is moving, and you haven't played with it in the last minute): it idles, looks around, sits, stretches, thinks, types on a tiny laptop, and takes short slow strolls.
- **While you're playing with it:** it chases and follows the pointer, pouncing when it can reach it.
- **When the pointer has been resting for a while:** it walks over to inspect it.
- **When it's tired, or after 3 minutes with no interaction:** it falls asleep.

### Climbing the screen edges

Now and then, more often when it's lively and you're not busy, it sets off on an outing:

1. It walks to a side edge and goes round the corner onto it.
2. It climbs up and down it, stopping now and then to cling and look around.
3. It may go round the top corner to hang from the top edge, shimmying along hand over hand or idly swinging.
4. Every step is picked at random as it goes. Eventually it climbs back down to the floor, pushes off into the room with a leap, lets go and drops, or loses its grip, wobbles, slips and falls.
5. Physics takes over in the air: momentum, gravity, a bounce, a squash on landing. A hard landing leaves it dizzy for a moment.

Click it while it's up on an edge and it gets startled off. Drag it off and throw it like any other time. Turn outings off with `climbing = false`.

The window stays 100×100 the whole time and simply follows Claude around the screen. Going round corners, Claude tucks into a ball while the middle of its body swings round the corner, so it never jumps.

### Pointer games while you're away

After you have been away from keyboard **and** mouse for `afk_after` seconds (default 120), it may start a game every so often (randomized around `mischief_every`, default 90 s):

1. It walks over, or sneaks up crouched and squinting, to the pointer.
2. It reels the pointer down into its paw if it's high up, or snatches it with a pounce.
3. It carries it proudly, winds up and throws it. The pointer flies with momentum and gravity and bounces off the screen edges.
4. It chases it down and bats it along the bottom of the screen a few times, then goes back to its business.

Safeguards:

- **You always win:** any key press or mouse movement cancels the game at once. It lets go, looks a little caught out, and stops touching the pointer.
- **It never clicks, types or drags anything.** It only moves the pointer, and only while you're away.
- **The screen still locks:** moving the pointer counts as activity to the desktop, so games stop after `mischief_for` seconds away (default 600). Your screen then blanks and locks as usual.
- **You can turn it off:** set `cursor_mischief = false` (see Settings).

## Seeing the pointer on GNOME Wayland (optional extension)

On a Wayland session, apps can't see where the mouse pointer is, and can't move it. The mascot still works without the extension: it roams, naps, and reacts to hovering, clicks, drags and throws. It just can't follow the pointer around the screen or play pointer games.

To enable those, install the small companion GNOME Shell extension from `gnome-extension/`:

```bash
./companion-install --gnome-extension
```

Then **log out and back in once** (GNOME only discovers new extensions at login), and run:

```bash
gnome-extensions enable claude-mascot-cursor@claude.local
```

The extension:
- reports where the pointer is and how long since your last keyboard or mouse input (not counting its own moves);
- moves the pointer only when the mascot asks, and only after 10 seconds without any input from you, never while the screen is locked;
- never clicks or types.

Remove it with `gnome-extensions uninstall claude-mascot-cursor@claude.local`.

On an X11 session nothing extra is needed. The mascot reads the pointer and your idle time from the X server (XScreenSaver extension) and moves the pointer with XWarpPointer.

## Settings

Settings live in `~/.config/claude-mascot/config.toml`, one `key = value` per line. Print every setting with its default and explanation:

```bash
./companion --print-config
```

Save that as your starting file:

```bash
mkdir -p ~/.config/claude-mascot && ./companion --print-config > ~/.config/claude-mascot/config.toml
```

| Setting | Default | What it does |
|---|---|---|
| `size` | 100 | Window size in pixels, rounded to a multiple of 25 (100 = 4×4 screen pixels per art pixel) |
| `roam` | true | Walk and run around on its own |
| `climbing` | true | Outings up the side edges and along the top edge |
| `monitor` | 0 | Which monitor it lives on: 0 = primary, 1, 2, … = monitors in system order |
| `cursor_play` | true | Inspect, follow and chase the pointer |
| `eye_tracking` | true | Eyes follow the pointer |
| `activity` | 1.0 | Higher = shorter idle and sitting spells |
| `curiosity`, `playfulness` | 0.6 | Personality, 0 to 1 |
| `walk_speed`, `run_speed` | 45, 210 | Pixels per second |
| `gravity`, `bounce` | 2200, 0.42 | Throw physics |
| `sleep_after` | 180 | Seconds without interaction before it naps |
| `cursor_mischief` | true | Pointer games while you're away |
| `afk_after` | 120 | Seconds without keyboard or mouse input before you count as away (minimum 10) |
| `mischief_every` | 90 | Average seconds between pointer games |
| `mischief_for` | 600 | Stop pointer games after this long away, so the screen can lock |
| `fps` | 60 | Frame rate while moving (30 when still, 12 asleep) |

Restart the mascot after editing the file. A mistyped line is reported in the terminal and skipped.

## Troubleshooting

| Problem | What to do |
|---|---|
| `companion: Claude is not built yet` | Run `./companion-install`. |
| `missing system libraries: …` | Run `./companion-install`. It names the packages and the `sudo` command. |
| `no desktop display found (DISPLAY is not set)` | Start it from a terminal inside your desktop session, not over SSH or a text console. On Wayland, Xwayland must be available (GNOME, KDE and most others start it automatically). |
| `cannot connect to X11/Xwayland` | As above. Check `echo $DISPLAY` prints something like `:0`. |
| The installer can't install packages | Run the `sudo` command it shows yourself. On Debian or Ubuntu, `sudo apt-get update` first may help. Then run `./companion-install` again. |
| The build fails while downloading | The first build needs internet access to fetch Rust libraries. Rerun once you're online. |
| `Rust … is older than 1.85` | Run `rustup update stable`, or let the installer do it. |
| It doesn't follow the pointer, or never plays pointer games (GNOME Wayland) | Install the extension: `./companion-install --gnome-extension`, then log out and back in. Without it, everything else still works. |
| It sits too still for your taste | Raise `activity` in the settings file, or right-click it to pick an activity. |
| Something looks wrong | `MASCOT_DEBUG=1 ./companion` logs what it is doing. |

## For developers

```bash
cargo test --release     # unit tests: physics, behavior, animation, rendering, settings
./companion-install      # rebuild
```

## How it works

Each frame, four layers work together:

1. **The mind** (`src/behavior.rs`) decides what Claude is doing and returns an intent: how fast to walk, whether to jump, where to look, what to do with the pointer.
2. **The physics** (`src/physics.rs`) carries the intent out in fixed 1/120 s steps, so motion is the same at any frame rate or under load.
   - The body is either airborne or attached to one of four surfaces: the floor, the left or right edge, or the top edge (hanging).
   - Rounding a corner swings the middle of the body around the corner point, so the change of direction never jumps. Letting go of an edge keeps the middle of the body where it is.
   - It also covers momentum, gravity, bouncing off the screen edges, and springs for leaning, wobbling and squash. Jumps start with a short crouch.
   - The pointer, during games, is a little physics toy too.
3. **The animation** (`src/anim.rs`) turns activity plus body motion into a pose. Expressions are continuous values (eye openness, eye curve, mouth, blush). The walk cycle is driven by distance travelled, so feet never slide. Every value eases from frame to frame, and turning around slides the face across instead of flipping.
4. **The pixel renderer** (`src/render.rs`) turns the pose into pixel art on a 25×25 canvas:
   - Squash and stretch pick a hand-drawn body frame.
   - On a side edge the whole character is turned a quarter turn, feet against the edge. Hanging, it is drawn high in the window with its paws at the top.
   - Leaning shears rows by a pixel or two (from the feet when standing, from the paws when hanging, like a pendulum).
   - Tumbling turns the sprite in exact quarter turns.
   - Expressions pick eye and mouth sprites.

   The canvas is enlarged by a whole number with nearest-neighbour sampling: no anti-aliasing, no smoothing, every art pixel a crisp square.

The window itself (`src/main.rs`) follows the body around the screen. The cursor feed and pointer moves are in `src/cursor.rs`, and the X11 specifics are in `src/x11.rs`:
- always-on-top, all workspaces, never steals focus;
- click-through input shape;
- pointer position, idle time and warping.

### Why X11 on a Wayland desktop?

GNOME's Wayland side gives ordinary apps no way to stay on top or place their own window. The mascot therefore runs as an X11 window through Xwayland, which GNOME treats the way it should.

## Editing the pixel art

Every sprite is a text file in `assets/pixel/`, one character per pixel:

| Character | Color | Character | Color |
|---|---|---|---|
| `.` | Transparent | `m` | Inside of mouth |
| `o` | Outline and eyes | `p` | Blush and tongue |
| `#` | Body | `r` | Heart red |
| `h` | Highlight (light comes from the top left) | `y` | Gold |
| `s` | Shade | `b` | Sleepy blue |
| `w` | White | `g`, `d` | Light and dark gray |
| `c` | Spark cream | | |

The palette is `PALETTE` in `src/sprite.rs`, and all rows in a file must be the same width.

- **Character parts** (`body*`, `foot`, `paw*`, `eye_*`, `mouth_*`, `tuft_spark`, `laptop`) carry their own outline.
- **Effects** (`heart`, `spark`, `z`, `exclaim`, `question`, `bubble`, `puff`) are drawn without one. The renderer adds a matching 1-pixel outline.

To add a sprite, save `assets/pixel/<name>.txt` and add `"<name>"` to `SOURCES` in `src/sprite.rs`.

To add an activity (for example `Dance`):

1. Add it to `enum Act` in `src/anim.rs` and to `name()`. Add it to `Act::CYCLE` if right-click should be able to pin it.
2. Add an arm in `target()` that sets pose values as functions of `t` (seconds into the activity), for example:
   - `p.eye_curve = 1.0` for ^ ^ eyes;
   - `p.arm_r = 2.0` for a raised paw;
   - pushing `fx(FxKind::Heart, x, y, scale, alpha)` for an effect.

   The renderer picks the matching pixel sprites.
3. Let the mind choose it: add a row to the `options` table in `choose_next()` in `src/behavior.rs` (activity, weight, shortest and longest duration).
4. Rebuild, then right-click the mascot (or watch it) to see it.

To see every animation as PNG strips at the window size, run:

```bash
./companion --export-frames frames
```

`MASCOT_DEBUG=1 ./companion` logs every change of activity, landings and grabs to the terminal.

## Known limits

- It lives on one monitor at a time (choose which with `monitor`), and its world is that monitor's edges. On Wayland it can't know where your windows are, so it doesn't climb on them.
- On GNOME Wayland, following the pointer and pointer games need the companion extension.
- I tested the extension's code paths with a stand-in for GNOME Shell, not inside a real GNOME Shell session; that needs a logout and login. If its own pointer moves were ever mistaken for yours, the safe thing happens: games stop.
- There are no keyboard shortcuts, because the window never takes focus.
