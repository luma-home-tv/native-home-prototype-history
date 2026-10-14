# Native Home

A separate native Home application built on PlxNative's shared UI libraries, in
an optional example/client rather than in the Plex application. The selected
prior art and compatibility boundaries are recorded in `prior-art.json` and
`design.json`. Derived source remains GPL-3.0-or-later; no LG system component is
replaced.

The shell composes the existing card grid, declarative forms, focus engine,
double-buffered hit map and press machine. Its four main areas are Home,
Applications, Inputs and Controls. Controls lead to Picture, Sound, Network,
Accessibility, General and Panel Care. A small shared `Art::Texture` variant lets
the standard cards borrow local app icons without a Plex server. Existing server
art paths retain their previous behaviour.

Application identities and focus keys survive catalogue refreshes. Hidden
services, store placeholders and unregistered targets are excluded. A long OK
press toggles a favourite and does not also launch the app on release. Moving
focus cancels the armed gesture. Back returns through Controls or Home, and exits
from Home. Empty catalogues and intentionally empty favourites keep the rail
usable. Preferences have a serializable schema; a persistent store has **not**
yet been connected, so this preview starts with default favourites each launch.

`collect-snapshot.py` reads registered apps/launch points, bounded icon files,
HDMI status and picture/audio values over SSH. It never installs, launches or
changes anything on the TV. Its output is a private build artifact. The renderer
labels this data as a captured snapshot, not live state. HDMI plug, signal and
active state are distinct. Settings include their input/dynamic-range/mode
context; protected, unavailable or stale rows cannot activate.

The shell emits typed requests for applications, HDMI inputs and LG Settings,
but the preview **records them without executing them**. It has no setting-write
action, arbitrary service caller, privileged broker, Home interception or boot
hook. Unsupported control sections stay with LG Settings. Comprehensive settings
coverage, live refresh, persistent preferences, a restricted executor and device
performance remain incomplete.

## Build and verification

`build-client.py test` runs the separate client's behaviour checks.
`build-client.py host --bin home-preview` builds the native desktop shell;
run it with a snapshot asset directory and a new capture directory. Add
`--interactive` for a visible, bounded 30-second session. Fonts use the shared
app-resource environment; use an isolated `PLXNATIVE_APP_DIR` and
`PLXNATIVE_RUNTIME_DIR` on the host.

`build-client.py arm --bin home-preview` rebuilds Rust's standard library for
ARMv7 softfp using `NATIVE_HOME_WEBOS_SDK` and `NATIVE_HOME_BUILD_DIR`. It compiles
the SDL adapter against the target headers: webOS adds fields to SDL events, so
Rust does not reinterpret the desktop event union. Only the observed remote OK
and Back codes and standard navigation/pointer events are mapped. Home, power,
volume and channel keys are not intercepted.

`package-preview.py` packages only `com.ash.nativehome.preview`. It requires a
fresh output staging directory and, for the interactive binary,
`NATIVE_HOME_SNAPSHOT_ASSETS`. Both previews include no services, boot hooks,
permissions or system-file payload. The device shell refuses root/unexpected
package identity, captures six pages to its own runtime namespace, then runs for
at most 30 seconds. Its report separates scripted component navigation from SDL
events and explicitly makes no performance claim. SDK locations, TV addresses,
credentials, real snapshots/icons and runtime receipts stay outside tracked
source.

## Evidence boundaries

The earlier [native webOS 24 control capture](docs/control-centre-webos24.png)
used synthetic values on an OLED55G26LA/Mali-G52 in an unprivileged package. Its
package, launcher entry, runtime, jail and mounts were removed; checked core
hashes were unchanged and LG Home/PlxNative launches passed afterward. That
capture proves rendering and scoped removal, not interactive navigation or
settings parity.

The newer host shell renders the G2's actual captured catalogue and readings.
Behaviour tests cover drawer identity, protected/stale controls, navigation,
press cancellation, hold-versus-release, empty favourites and distinct input
requests. Host frames and tests are not television performance or physical remote
proof. Each device deployment needs its own lock, frame inspection and cleanup
receipt before it can be called verified.
