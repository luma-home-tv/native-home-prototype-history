# Native Home

A separate native Home application built on PlxNative's shared UI libraries. The
Plex application, stable package and player remain separate and unchanged.

This branch starts with a small model and declarative forms. It directly reuses
`plx_ui` instead of copying its widgets. The current model executes no TV calls
and contains no setting-write action. Protected and stale settings cannot be
activated; normal settings open a read-only inspection, with LG Settings as the
route for changes until each operation has been independently validated.

The first four main areas are Home, Applications, Inputs and Controls. Detailed
controls are grouped into Picture, Sound, Network, Accessibility, General and
Panel Care. App identities and focus keys survive catalogue refreshes; hidden
services and store placeholders are excluded from the ordinary app drawer.

Host checks cover catalogue and control safety. A native simulator capture is
needed for layout, and an isolated webOS 24 device build is needed for runtime,
remote input, performance and clean removal. No simulator result is a TV result.

The application will use its own package ID. No Home-button interception, boot
hook, system-library replacement or settings writes are part of the first pilot.

All fixtures use synthetic application IDs. TV addresses, credentials, real
device snapshots, SDK paths and runtime receipts must remain outside tracked
source. Source derived from PlxNative remains GPL-3.0-or-later.

## Current evidence

The control-centre development surface renders through the shared `plx_ui` /
`plx_gfx` engine. Its full-resolution [native webOS 24 capture](docs/control-centre-webos24.png)
was taken on an OLED55G26LA with the Mali-G52 backend, in a separate unprivileged
package. The preview exited and its package, launcher entry, runtime files, jail
and mounts were removed. LG Home and PlxNative launches passed afterward, and the
checked Home/startup/jail-policy files retained their original hashes.

This capture uses synthetic example settings, not live TV values. It has no input
dispatcher or TV action executor. It proves rendering and the scoped deployment /
removal path, not interactive navigation, performance or complete settings parity.
The app drawer, favourites, inputs, interactive navigation and restricted live
data/control bridge remain ongoing work.

`build-client.py` provides separate `test`, `host` and `arm` targets. The ARM
target rebuilds Rust's standard library for ARMv7 softfp and uses the existing
auxv compatibility shim; it never builds or deploys the Plex application.
`package-preview.py` packages only `com.ash.nativehome.preview`, with no services,
boot hooks or system-file payload. SDK locations and build outputs are supplied
through local environment variables rather than committed machine paths.
