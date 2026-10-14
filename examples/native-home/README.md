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
