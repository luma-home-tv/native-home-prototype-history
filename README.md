<p align="center"><img src="assets/luma-home/banner.svg" alt="Luma Home — Native TV dashboard" width="800"></p>

Luma Home is an experimental native home dashboard for rooted LG webOS TVs.
It brings favourites, an app drawer, HDMI status and a control centre into one
remote-friendly interface, rendered with Rust, SDL2 and OpenGL ES.

**Luma Home builds directly on [PlxNative](https://github.com/GLinnik21/plx-native),
created by [Gleb Linnik](https://github.com/GLinnik21) and its contributors.**
Their research and engineering make this project possible. The native rendering
stack, reusable UI, focus/input machinery and device verification practices are
our foundation. This repository preserves their source history, GPL licensing,
authorship and third-party notices.

[Implementation plan](examples/native-home/docs/implementation-plan.md) ·
[Native client](examples/native-home/README.md) ·
[Design concepts](examples/native-home/docs/concepts/README.md) ·
[Brand assets](assets/luma-home/README.md) ·
[Upstream PlxNative README](PLXNATIVE_README.md)

## Interface direction

Large cards, restrained chrome, clear spatial focus and a simple Back hierarchy.
The visual inspiration is the Apple TV / tvOS feel interpreted by PlxNative.
Luma Home has its own identity and extends those native components into a TV
home dashboard.

The OLED canvas is **true black**. Graphite gives tiles and controls structure;
soft white supplies readable ink and a mint accent is used sparingly. Idle
behaviour must minimise persistent bright elements while preserving LG's panel
protections. Black backgrounds alone do not establish burn-in prevention.

![Synthetic four-screen interface concepts](examples/native-home/docs/concepts/overview.png)

These are labelled **design mockups with synthetic data**. They illustrate the
intended interface, including work that is still planned. Native device evidence
is recorded separately below.

## Building on PlxNative's engineering

PlxNative demonstrated a practical native UI path on consumer LG televisions,
and developed the platform knowledge, reusable components and test machinery
that we reuse here:

- **Native rendering and resources:** Rust UI crates, SDL/OpenGL ES, font/text
  layout, card artwork, shared theme roles and bounded resource handling.
- **Remote interaction:** identity-based focus, spatial navigation, card motion,
  hit maps, gesture ownership and tap/hold cancellation.
- **Firmware and lifecycle work:** ABI-aware native input handling, app-local
  Back delivery, foreground/hidden behaviour and disciplined compatibility
  checks against actual LG firmware and headers.
- **Verification:** separate host, visual and real-device evidence; measured
  performance claims tied to their actual workload and build; scoped device
  locking and cleanup checks.

Luma Home adds an independent dashboard client, a validated installed-app
catalogue, saved favourites, live TV readings, typed normal-app launch requests,
Home/source layouts and a context-aware control centre. It initially consumes
PlxNative's shared crates through path dependencies. The Plex application and
player retain their own behaviour. Generic changes, such as local artwork
support, can be proposed upstream separately after verification.

PlxNative's playback implementation and its reported frame-rate results belong
to PlxNative's workloads. They are not evidence that Luma Home has achieved the
same performance or provides native media playback.

## Current evidence and limits

Development has used an OLED55G26LA running webOS24 **9.2.2-61**, firmware
**23.25.55**, with a 32-bit ARMv7 softfp application on the Mali-G52.
This is a development target, not a compatibility claim for every webOS TV.

| Area | Current status |
| --- | --- |
| Native UI | Home, Apps, Inputs and control pages captured at 1920 × 1080 on the G2 |
| Home | Shared favourites shelf above four HDMI status cards; live catalogue/input/picture reads |
| App drawer | Registered visible launchable apps; hidden/stub/invalid targets excluded; stable focus identity |
| Favourites | Held OK pin/unpin; atomic private save/readback and reload across app launches; reboot persistence pending |
| App launching | Restricted normal-app adapter verified with a separately installed native test target and independent foreground readback |
| HDMI / LG Settings | Probes remain incomplete; normal execution stays disabled |
| Controls | Current-context picture readings; unsupported/protected rows remain read-only or LG-managed; no setting-write action |
| OLED palette | True-black canvas captured on G2; Home frame is 80.2% exact RGB zero; graphite tiles retained |
| Remaining UI | Clock/status header, search/filter/order, broader controls/audio; native rail repaint review |
| Remaining device proof | Physical remote/pointer/wheel, reboot persistence, frame pacing, latency, memory and idle behaviour |

The [milestone records](examples/native-home/docs/implementation-plan.md#verification-and-delivery)
identify exact code revisions and proof boundaries. The current client has passed
35 behaviour checks, Clippy, an ARM release build and native capture/removal
checks. The shared-library gate, shipping feature check and G2 palette capture also
passed on `2a53bfd`; the [OLED/branding milestone](examples/native-home/docs/oled-branding-milestone.json)
records exact framebuffer measurements and clean removal. Some native page
captures omit the navigation rail or clip glyph edges. Rendering review remains
open; host/model checks do not prove every device pixel.

The client remains a separate, bounded development preview. It does not replace
LG Home, intercept Home, install boot hooks, modify firmware/system libraries or
change jail policy. Networking, media/DRM/HDMI/audio services and OLED protections
remain under LG control. There is no generic root shell or arbitrary service
caller in the UI. Real device addresses, credentials, inventories, app icons,
runtime records and household captures remain outside tracked source.

## Source and development

The dashboard lives in [`examples/native-home/`](examples/native-home/).
Its standalone Cargo workspace uses the shared crates under `rust-modules/`.
See the [client build instructions](examples/native-home/README.md#build-and-verification)
for host and ARM paths and the [plan](examples/native-home/docs/implementation-plan.md)
for completion requirements. The inherited root build/install/release tools
belong to the PlxNative application; they are not Luma Home release installers.
No production Luma Home release is being published at this stage.

For the original project's documentation and installation instructions, see
[the preserved PlxNative README](PLXNATIVE_README.md). The initial fork base is
`d507c4f9ed5b4ecd88ce9b2e998c50da506e6271`; the [prior-art record](examples/native-home/prior-art.json)
explains the selection and alternatives considered.

## Licensing and attribution

Derived source remains **GPL-3.0-or-later**. Retain [`LICENSE`](LICENSE),
[`LICENSING.md`](LICENSING.md), [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md),
the notices in `licenses/`, and original copyright/source attributions.
The [inherited trademark guidance](TRADEMARKS.md) reserves PlxNative's identity;
Luma Home uses its own mark and branding.

Luma Home is an independent project. It is not affiliated with or endorsed by
LG, Apple, Plex or PlxNative's maintainers. LG/webOS, Apple/tvOS and Plex names
identify compatibility or design references; they are not Luma Home branding.
