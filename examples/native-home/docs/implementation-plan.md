# Native Home implementation plan

The target is a responsive native Home dashboard for the OLED55G26LA on webOS24.
It remains a separate application: LG Home, Home/power/volume handling, firmware,
system libraries, boot configuration, media services and OLED protections stay
under LG's control. The C2 is outside the development/test scope.

The [concept renders](concepts/README.md) show the intended interface. They are
synthetic design illustrations, not screenshots of implemented functionality.
The [interface specification](../design.json) records the layout and interaction
requirements; the milestone records below identify what has actually been proved.

## Direction

The agreed visual direction is the Apple TV / tvOS feel that PlxNative interprets
well: large cards, restrained chrome, smooth spatial focus and an obvious Back
hierarchy. Keep this native and remote-first. The concepts are a starting point;
remove unnecessary screen furniture as the interactive screens take shape.

Build the optional Home client on PlxNative's shared Rust UI, input/focus and
SDL/OpenGL ES libraries. Reuse its components instead of maintaining a second
widget system. Generic improvements can be proposed upstream separately after
verification; the Home client does not change the Plex application or player.
The shipping architecture uses native rendering, with no WebView.

| Area | Intended behaviour | Current evidence |
| --- | --- | --- |
| Home | Clock/status header, immediate access to favourites and HDMI inputs, control-centre entry and LG Home return | Native favourites grid and live catalogue/status rendered on G2; combined Home layout and clock remain to build |
| Apps | Complete actionable app drawer, search/filter, pinning and dashboard-only ordering | Registered visible apps joined to launch points; stubs/hidden/invalid targets excluded; live updates and identity/focus checks pass; search/order/launching remain |
| Favourites | Pin/unpin with held OK, save reliably, retain selection across changes | ARM pin/save/readback/relaunch/unpin verified; bounded background writer; reboot survival remains untested |
| Inputs | Four HDMI cards with distinct plug, signal and active states; select through LG's normal source path | Native cards refreshed through normal-app reads; source selection/readback remain |
| Controls | Native category/detail pages, correct context, validated choices/ranges and access to the rest through LG Settings | Native category/value pages rendered; four picture values/context refreshed live; write capability and LG Settings launch remain unverified |
| Navigation | Remote focus, safe tap/hold, Back hierarchy, pointer and wheel | Shared-machine tests and scripted ARM navigation pass; injected Back reaches SDL and exits; physical remote/pointer/wheel proof remains |
| Performance | Smooth transitions, bounded memory and render only when needed | Native Mali-G52 rendering proved; this client has no validated FPS, latency, memory or idle result yet |

## Next implementation stages

1. **Expand live data.** The typed normal-app provider now refreshes catalogue,
   launch points, HDMI and picture state in the native interface. Focus/favourites,
   unchanged gestures and stale/context failure handling have checks; four real
   cycles were applied on the G2. Complete broader coverage, audio and newly
   installed icon loading. Do not treat subscription acknowledgement as delivery
   proof. Keep loading and queues bounded.
2. **Validate normal actions.** Add app, HDMI and LG Settings adapters with
   authoritative target checks and foreground/input readback. Exercise app
   launching first against an isolated test target, avoiding playback or writes
   to personal viewing records. Keep unknown targets/parameters rejected.
3. **Expand controls safely.** Audit webOS24 descriptors and semantics for each
   operation. A descriptor alone does not grant write authority. Require valid
   range/enum, matching input/HDR/mode context, bounded effect, readback and an
   original-state restoration test. Audio's current ordinary-app getter failure
   is unresolved; it does not establish that root is required. Use a restricted
   local helper only for operations that actually need privilege, with caller,
   executable and session validation and a fixed operation allowlist.
4. **Finish the interface and measure it.** Implement the illustrated combined
   Home layout, app search/filter/order, useful live status and native control
   widgets. Verify physical remote, pointer, wheel and Back behaviour on the G2.
   Measure the actual dashboard's frame pacing, input response, memory and idle
   behaviour; the earlier C renderer's results do not transfer to this client.

## Protected and unsupported operations

Network reconfiguration, firmware, factory reset, service/calibration controls,
OLED protections and compensation scheduling remain protected/delegated. An
unsupported control routes to LG Settings or stays unavailable; the UI never
invents a successful native action. No arbitrary shell command, service URI or
file path is exposed through the interface. No Home interception or boot hook is
part of this plan.

## Verification and delivery

Every TV build uses its own isolated package and the repository's device lock.
Build before taking the lock; release it before reviewing frames or changing
source. Inspect native frames, record exact source/build revisions and keep
private device data outside Git. After each test, verify removal of the package,
launcher entry, process, runtime, persistent test data, jail and mounts; compare
checked core hashes and picture/audio readbacks, then verify LG Home and restore
the original foreground.

The current evidence is in the [native shell milestone](shell-milestone.json)
and [favourites/read-capability milestone](preferences-milestone.json), and
[live-reading milestone](live-milestone.json). Completion
requires the intended functional interface and device evidence above, not only
concept renders, a successful build or component tests.
