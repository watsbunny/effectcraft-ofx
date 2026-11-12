# OpenFX plug-ins

EffectCraft runs [OpenFX](https://openfx.readthedocs.io/) (OFX 1.4) image-effect plug-ins as an
**external effect helper**: `effectcraft-helper-ofx`, a separate program that loads the plug-ins
and renders their frames. EffectCraft talks to it over the helper protocol
([external-helpers.md](external-helpers.md)). Every OFX plug-in it finds becomes an ordinary
effect: it appears in Effects & Presets under the category the plug-in declares, its parameters
show up in Effect Controls with keyframes and expressions, and `effect.apply` takes its id (the
OFX identifier, lower-cased, e.g. `org.effectcraft.test.invert`).

The containment matters: OFX plug-ins are native libraries. A plug-in that crashes or hangs takes
down the helper, not EffectCraft. EffectCraft shows the error on the effect, leaves the frame as
it was and starts the helper again for the next render (see [ADR 0001](adr/0001-external-effect-helpers.md)).

## Installing the helper

The helper is part of this workspace:

```sh
cargo build --release -p effectcraft -p effectcraft-helper-ofx
```

Put `effectcraft-helper-ofx` (`effectcraft-helper-ofx.exe` on Windows) in one of the folders
EffectCraft searches for helpers:

| Where | Notes |
|---|---|
| `helpers/` next to `effectcraft` | What a package should ship. |
| `helpers/` in the [settings folder](preferences.md) | No administrator rights. |
| A folder in `EFFECTCRAFT_HELPER_PATH` | A path list; entries can be programs or folders. |

Helpers run native code, so EffectCraft does not start them unless asked:

- **Effect ▸ Scan for Effect Helpers** (`effect.helpers.scan`) starts them for this session.
- Set `EFFECTCRAFT_HELPERS=1` to start installed helpers at every launch.

## Where plug-ins are looked for

The helper scans these folders, system folders first, at start-up:

| System | Folders |
|---|---|
| Windows | `%CommonProgramFiles%\OFX\Plugins`, `C:\Program Files\Common Files\OFX\Plugins` |
| macOS | `/Library/OFX/Plugins`, `~/Library/OFX/Plugins` |
| Linux | `/usr/OFX/Plugins`, `/usr/local/OFX/Plugins`, `${XDG_DATA_HOME:-~/.local/share}/OFX/Plugins` |
| Every system | `OFX` in the settings folder, `OFX` next to the helper, `OFX` next to EffectCraft, and every folder in `OFX_PLUGIN_PATH` |

The settings folder's `OFX` (Windows `%APPDATA%\EffectCraft\OFX`) is the place for plug-ins copied
by hand: it needs no administrator rights, installers and uninstallers never touch it, and portable
mode moves it with the settings. A bundle must have a folder for this platform:

```
Something.ofx.bundle/
  Contents/
    Win64/          Windows (64-bit)       Something.ofx
    MacOS/          macOS                  Something.ofx
    Linux-x86-64/   Linux (x86_64)         Something.ofx
```

**Loading one by hand:** Effect ▸ Load Plug-ins Through Helper…
(`effect.helpers.load {"path": "…"}`) takes an `.ofx` file, an `.ofx.bundle` folder or a folder of
bundles. The effect lasts until EffectCraft quits; copy the bundle into a searched folder to keep
it.

`effect.helpers.list` shows the helper and how many effects it offers; `effect.plugins.list` lists
the effects themselves with their `source` (`helper: …`) and last render error.

## Settings and files

The helper keeps its files in EffectCraft's settings folder (passed as
`EFFECTCRAFT_HELPER_STATE_DIR`):

| File | What it is |
|---|---|
| `ofx_cache.json` | The scan cache: what each plug-in file contains, so a big suite does not have to be loaded and described at every start. Safe to delete; it is rebuilt. |
| `ofx_blocklist.txt` | Plug-in files that crashed the helper while loading; a plug-in with 2 strikes in a row is skipped. |
| `ofx_render_crashes.txt` | Plug-ins that crashed the helper while rendering; 2 in a row and it is skipped. |
| `ofx_settings.json` | Optional: `{"host_name": "…"}` and `{"scan_cache": false}`. |

`EFFECTCRAFT_NO_OFX=1` makes the helper skip its scan (useful to check whether a plug-in is the
cause of a problem). `EFFECTCRAFT_OFX_HOST_NAME` overrides the host name for one run.

To force a rescan after changing a plug-in's licence or a support file:

1. Quit EffectCraft.
2. Delete `ofx_cache.json` from the settings folder.
3. Start EffectCraft again (or run Effect ▸ Scan for Effect Helpers).

To bring a skipped plug-in back, update or reinstall it, or delete its line from
`ofx_blocklist.txt` / `ofx_render_crashes.txt`. `effect.helpers.scan {"retry": true}` also clears
EffectCraft's own helper block list (`helper_blocklist.txt`).

## What works

| Plug-in | Status |
|---|---|
| RE:Vision **ReelSmart Motion Blur**, **Twixtor** | Works (tested with RSMB 7 and Twixtor 8 on Windows in the in-process host this crate came from). |
| Boris FX **Sapphire** (about 290 effects) | Works (tested with Sapphire 2026.5 on Windows). |
| Other OFX image effects | Expected to work when they have a CPU render path. |
| GPU-only OFX effects | Skipped: the host renders on the CPU. |

All frames are premultiplied RGBA `f32`. Plug-ins that declare byte, short, RGB or alpha-only
clips are converted. Only image-effect plug-ins are hosted; OFX generators, transitions, general
effects and GPU renderers are not.

## What to expect

- **Watermark or red X:** the plug-in runs as a trial. Activate the **OFX** licence with the
  vendor's tool (an After Effects licence usually does not cover OFX).
- **No on-screen handles:** OFX overlays and buttons that open vendor windows are not shown.
- **Curves are not editable:** curve parameters are not saved with the project and start from the
  plug-in's default every time.
- **Slow:** everything renders on the CPU. Work at Half or Quarter resolution while editing.
- **A plug-in is native code.** Install only plug-ins you trust; it runs in the helper with your
  user's access to the computer.

## How it is built

| Part | Where |
|---|---|
| OFX host (C ABI, host suites, parameters, clips, instances, bundle discovery) | `crates/ofx` (`effectcraft-ofx`) |
| Helper binary: scans, offers effects, serves frames | `apps/effectcraft-helper-ofx` |
| Protocol and the app side | `crates/helper-host` ([external-helpers.md](external-helpers.md)) |

The OFX host needs `unsafe` Rust to call the C interface, so `crates/ofx` is the only crate that
does not take the workspace's `unsafe_code = "forbid"`. Every host callback is wrapped in
`catch_unwind`, null-checks its pointers and validates handles by magic number. The helper binary
and the app are safe Rust.

## Testing

`crates/ofx` has the host's own tests and needs a test plug-in bundle (the fork's
`examples/ofx/ec-test-ofx`). Build that library, lay it out as
`ec-test-ofx.ofx.bundle/Contents/<platform>/ec-test-ofx.ofx`, and point `EC_TEST_OFX_BUNDLE` at
the bundle folder:

```sh
cargo build --release --manifest-path path/to/ec-test-ofx/Cargo.toml
EC_TEST_OFX_BUNDLE=/path/to/ec-test-ofx.ofx.bundle cargo test -p effectcraft-ofx
```

Without the variable, the tests that need the bundle are skipped.
