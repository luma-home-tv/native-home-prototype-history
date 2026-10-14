# Luma Home native client

A separate native Home application for Luma Home, built on PlxNative's shared UI libraries, in
an optional example/client rather than in the Plex application. The selected
prior art and compatibility boundaries are recorded in `prior-art.json` and
`design.json`. Derived source remains GPL-3.0-or-later; no LG system component is
replaced.

The [four-screen design concepts](docs/concepts/README.md) illustrate the intended
Home, Apps, Inputs and Controls experience with synthetic data. They are labelled
mockups and are distinct from the native device captures and verified behaviour
described below.

Home places favourites above HDMI sources using the shared horizontal shelves.
Source tiles always show cable and signal readings; activity is a separate label.
The drawer uses the existing card grid. The shell also composes declarative forms, focus engine,
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
usable. Favourites now use a versioned record in an app-owned directory on
writable developer media. The store checks owner, private permissions, file type,
links, byte limits and app identities; it refuses corrupt or newer-schema records.
Directory-relative atomic replacement and fsync preserve the last complete state.
A bounded worker queue keeps disk flushes off the render thread; save success is
reported only after readback. The G2 verified pin/save, app exit, reload on a
second launch, then unpin/save. Both runs had confirmed readback as the normal
unprivileged app. Reboot survival has not been exercised. The test removed its
own persistent folder as well as its package/runtime/jail/mounts.

`collect-snapshot.py` reads registered apps/launch points, bounded icon files,
HDMI status and picture/audio values over SSH. It never installs, launches or
changes anything on the TV. Its output is a private build artifact. The renderer
uses this as its labelled startup snapshot and bounded local icon source. The
normal app now polls validated catalogue/launch-point, HDMI and picture getters
on a separate worker, applying typed updates to the native screens. HDMI plug,
signal and active state are distinct. Settings include their input/dynamic-range/mode
context; protected, unavailable or stale rows cannot activate.

The normal app's read-only native API probe successfully read the app catalogue,
launch points, HDMI status and the enumerated picture values on webOS24. The audio
getter returned `Message status unknown.` from that identity. Attaching the real
app ID through `LSCallFromApplicationOneReply` was rejected by the bus with code
`-1031` (not privileged). That denied attribution path is omitted from polling;
a working audio route remains unresolved. The later live-reading milestone
proved four successive real updates applied to the native G2 interface. It does
not prove subscriptions, external physical changes or launch/write capability.
Use an unprivileged adapter first and add a restricted helper only where a
specific operation actually needs it.

The shell now executes normal application requests through a restricted typed
adapter. It requires the dashboard to be foreground, re-reads registered app
metadata and launch points, rejects hidden/stub/self/unknown targets, sends only
an app ID to LG's normal launcher and distinguishes admission from foreground
readback. A separate native test target verified the live-card/shared-OK/adapter
path on the G2, with an independent foreground observation and target frame.
The proof package permits only that owned target; the normal client enables the
validated application operation.

HDMI and LG Settings request execution remain disabled pending their device
validation. Their device probes are incomplete: the Settings adapter observed an overlay but
the independent observer missed it; disconnected HDMI 4 reached its foreground
but EIM did not confirm activation. Neither is treated as a validated control.
There is still no setting-write action, arbitrary service caller, privileged
broker, Home interception or boot hook. Unsupported controls remain read-only. Comprehensive settings
coverage, broader live data, durable device verification, a restricted executor
and device performance remain incomplete.

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

The newer shell rendered all six capture pages on the G2/Mali-G52 from the actual
read-only catalogue and readings. Its scripted ARM focus/OK probe reached Inputs.
LG network input delivered Back through SDL (scancode 482, source 10), and the
shell exited from Home. The app-local SDL Back hint, also used by upstream, was
necessary for this delivery. Physical remote, pointer, wheel and performance
remain unverified for this client.

The final preview was removed completely: package, catalogue/launcher entry,
process, runtime, jail and mounts were absent; checked Home/startup/jail-policy
hashes and picture/audio readbacks were unchanged. LG Home was observed in the
foreground and the original PlxNative foreground was then restored and observed.
No reboot was needed. Actual snapshots, app icons, six inspected native frames
and detailed receipts stay in the private build workspace.

Eleven client behaviour tests and Clippy pass on code revision `dcc549d`, which
also produced the tested ARM release binary. The full upstream gate passed on
`ff25538`; the entire shared `rust-modules` tree is unchanged since that gate.
The public-safe [milestone record](docs/shell-milestone.json) keeps these evidence
boundaries explicit. Reboot persistence verification, comprehensive
control coverage and device performance remain ongoing work.

The storage/read-capability slice is recorded in
[its milestone record](docs/preferences-milestone.json). It passed 17 client tests,
Clippy and the ARM release link on `8aca0a6`. No shared upstream source changed
since the earlier full gate; only the optional client/package changed.

The [live-reading milestone](docs/live-milestone.json) records code revision
`30c02f3`: 25 client checks, Clippy, ARM release link and four native read cycles
applied during the G2 run. Updates preserve app identity/favourites; unchanged
updates retain an armed gesture, while vanished/changed targets cancel it.
Independent stale/unavailable domains are not displayed as fresh. The app,
runtime, persistent test folder, jail and mounts were removed with checked core
hashes and picture/audio readbacks unchanged; the original foreground was
restored and verified. Real frames and detailed receipts remain private.

The [application-launch milestone](docs/launch-milestone.json) records code
`8cbf255`: 30 client tests, Clippy and ARM release builds, native drawer gesture
launch to an isolated target, adapter plus independent readback, target identity
and frame, and complete removal of both packages and their owned data/mounts.
Checked core files and picture/audio values stayed unchanged; LG Home and the
original PlxNative foreground were verified afterward. This is not playback,
physical remote, HDMI/Settings launch or broad control/performance proof.


The [combined Home layout milestone](docs/home-layout-milestone.json) records
`1c736e3`: 35 client checks, Clippy, ARM release link and the native G2 capture.
Home uses a horizontally scrolling favourites shelf above all four source tiles.
Each source keeps its cable/signal status visible; a focused long name uses the
shared marquee. Host checks cover movement between shelves, empty favourites,
input identity/refresh, stale requests and spring motion settling. The device
run applied four real read cycles, confirmed preference readback and exited via
injected SDL Back. It proves rendering and bounded read/storage operation,
not physical remote navigation, frame pacing or idle CPU behaviour. The preview
and its data were removed, checked core hashes and picture/audio values stayed
unchanged, LG Home was verified and the original foreground was restored.
