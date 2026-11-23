# DLSS Frame Generation (through NVIDIA Streamline)

An optional choice on the Frame generation row: NVIDIA's DLSS Frame Generation (DLSS-G) makes one
to three frames between each two real frames, so motion looks smoother. It is the second kind
of frame generation beside [FSR 3.1's](frame-generation.md), and builds on
[DLSS Super Resolution](dlss-upscaling.md) through the same NVIDIA Streamline 2.14.16 (#2317).
NVForever ships no NVIDIA file; the player supplies them ([player page](../dlss.md)). What
Streamline documents of DLSS-G: [spec note](../spec-notes/streamline.md), "DLSS Frame
Generation". Code: `engine/nvforever/src/dlss/` (`win.rs` the loader, `frame_gen.rs` the frame,
`frame_token.rs` the frame numbers), `engine/nvforever/src/frame_gen.rs` (the row's choice and
request), `engine/nvforever/src/nv_low_latency.rs` (Reflex through Streamline).

## Using it

**Settings > Graphics > Frame generation: DLSS**, or `--frame-generation dlss`; **Generated
frames** below it sets 2x, 3x or 4x (one, two or three frames per real frame; 3x and 4x only
where the GPU offers them, `DLSSGState::numFramesToGenerateMax`, 5 on an RTX 5070 Ti
[observed, fork-notes `fg-spike-results.md`]). The DLSS choice is greyed out with the reason
when it cannot run, checked in this order:

| Reason | Why |
|---|---|
| The DLSS Frame Generation files are missing | Not all of `sl.dlss_g.dll`, `nvngx_dlssg.dll`, `sl.reflex.dll`, `sl.pcl.dll` are in the `dlss` folder beside the four DLSS files (or DLSS itself did not start). |
| Needs an NVIDIA RTX 40 series or newer | `slIsFeatureSupported(kFeatureDLSS_G)` answered that the adapter is not supported. |
| Needs hardware-accelerated GPU scheduling | It answered `eErrorOSDisabledHWS`: Windows' setting is off. |
| Needs a newer NVIDIA driver | It answered `eErrorDriverOutOfDate`. |
| Not on this GPU | Any other refusal (slInit or the device check with DLSS-G, a function not handed out), or not Windows. |
| Needs an upscaler | No upscaler draws: DLSS-G takes the upscaler's depth and motion (FSR 3.1 or DLSS, any mode). |
| Needs vsync off | Display sync is Vertical sync: DLSS-G requires vsync off on Vulkan [observed: its requirements' "vsync off required yes"; documented: "VSync with Frame Generation is only supported on D3D12", `ProgrammingGuideDLSS_G.md` §22.5]. Variable refresh is no bar: while DLSS-G is asked for the window presents with vsync off (`frame_pace::FramePace::presents_vsync`) and variable refresh's cap goes to Reflex as the page's does (below). |

Frames are generated only while the game's window is in front [observed: with other windows in
front the ratio fell to 1.00 with the mode on and status `eOk`; the smoke runner's
`front = true` holds the window in front for these checks]. Latency reduction turns on with it,
through Streamline's Reflex ([latency-reduction](latency-reduction.md)). FSR 3.1's frame
generation never runs while DLSS-G is loaded.

## How it works

### Loading

- With the four frame generation files beside the four DLSS files, the DLSS loader asks
  `slInit` for DLSS, DLSS-G and Reflex with frame-based tagging (`eUseFrameBasedResourceTagging`);
  PCL comes with them. DLSS-G's and Reflex's requirements are logged field by field and merged
  into the device check (`preflight::Requirements::merge`: lists joined, queues summed, optical
  flow queues taken as the interposer adds them only where the family has room). When `slInit` or
  that check fails, Streamline is shut down and started again with DLSS alone (the plugin
  manager may start over, `src/pluginManager.cpp:941`). Without the four files, DLSS runs as
  before and one `DLSS-G:` line names the missing files.
- Once the device exists (`Plugin::cleanup`, before the render world first runs and so before
  the window's swapchain is made): DLSS-G's, Reflex's and PCL's support and versions are logged,
  the DLSS-G and Reflex functions fetched (`slGetFeatureFunction`), Reflex set to `eLowLatency`
  once. DLSS-G stays loaded when the start's Frame generation choice is DLSS (`main` reads it
  from the command line and the INI by the row's rule); otherwise it is unloaded now
  (`slSetFeatureLoaded`) and loaded when chosen. Reflex stays loaded for the session.
  [documented: `main.rs`, `nvf_play::settings::apply_stored`] The early INI-resolved choice is
  only for this loader decision. The settings resource starts with the command-line/default
  choice, then normal INI application snapshots it for Defaults and Changed before loading
  saved choices.

### One frame number for everything

Every Streamline call that names a frame takes the token of our frame index plus one, the
number the latency markers carry, so DLSS-G can match the present markers to the frame's
constants (`frame_token.rs`). Streamline's token is a slot of a ring of six that moves on whenever
a different number is asked for (`src/sl.cpp:1120-1146`); the main thread (sleep, simulation
markers) and the render thread (constants, tags, evaluate, render and present markers) work a
frame apart, so a token is taken and used under one lock (the SR evaluate takes it under the lock
and calls after: at most three moves can happen meanwhile).

### Per frame (render world)

1. **Constants** (before the frame is recorded): the common constants once a frame for
   whichever upscaler draws, as DLSS SR's evaluate made them (`evaluate::constants_for`); the SR
   evaluate then sets none of its own.
2. **Tags** (after the post passes, before the UI): on a frame that may be generated (play
   state Playing, the upscaler's output real, constants set, drawn to the window rather than a
   screenshot, large enough, DLSS-G loaded and no failed status) the frame without the HUD is
   copied into a display-sized texture of its own, and the upscaler's depth (render size), the
   prepass motion (its render rectangle) and that copy are moved to `RESOURCE` and tagged
   `eValidUntilPresent`, each with the layout wgpu-hal gives `RESOURCE`. Other frames tag them
   null. A camera cut is not a reason to stop: the constants' reset says it.
3. **UI alpha** (after the UI, before Bevy's upscaling blit): `R16Float` (Streamline's Vulkan format
   table has no `R8_UNORM` [documented: `sl.chi/vulkan.cpp` `getFormat`]), 1 where the frame
   differs from its HUD-less copy, 0 elsewhere, tagged `kBufferTypeUIAlpha`; user interface
   recomposition is asked for (Hudless + UI alpha). A see-through HUD pixel counts as opaque UI.
4. **Options** (an exclusive system at the end of the render graph, on the render thread just
   before Bevy's present): `slDLSSGSetOptions` only on a change: on with the frames to generate
   (at most the state's limit) and the render and output sizes, off with resources kept on
   frames that are not generated; a failed call is made again next frame.
5. **State** (right after the present): `slDLSSGGetState` every frame; the generated frames feed
   the performance overlay's count (`frame_gen::GeneratedFrames`); a line every ten seconds with
   real and presented frames; a status other than `eOk` turns frame generation off for the
   session.

### Window changes and switching it on and off

- DLSS-G must be off before the swapchain changes (resize, present mode, full screen;
  `ProgrammingGuideDLSS_G.md` §12.0, §17). The vendored `bevy_render` holds a pending surface
  reconfigure (`HoldSurfaceReconfigure`, engine/vendor/README.md) for the frame DLSS-G's mode is
  set off and presented; Bevy reconfigures on the next. The OS window has already changed by
  then: what is met is "off before the swapchain changes", not "off before any window
  manipulation".
- Switching DLSS-G on or off (the row, or a reason coming or going) tears the swapchain down and
  makes it again (§18.0): the mode set off and presented once; in the next frame's extraction
  (the render thread waits, the main thread's Reflex calls come after, the device poll thread is
  held off) the GPU drained, the window's surface released (wgpu waits for the device and
  destroys the swapchain), DLSS-G's resources freed when it goes off, `slSetFeatureLoaded`, and
  Bevy's `create_surfaces` makes the surface and swapchain again that frame. Device polling and
  pipeline creation on task threads hold the read side of one gate; the switch holds its write
  side through the drain, surface release and feature load call [documented:
  `gpu_poll.rs::quiet`, vendored `pipeline_cache.rs::create_pipeline_task`]. Queued compiles wait
  at the gate; an in-progress compile finishes before the switch. No cache-task drain or
  timeout is needed. Streamline's own present queue is drained with the device.
  [documented: `dlss/frame_gen.rs::toggle_switch`, `frame_gen.rs::ReleasePresenter`]
  Loading waits until FSR's extraction has joined every presenter thread and released its
  shared surface. A single shared loaded flag gates FSR and DLSS-G's pacing; it changes only
  after `slSetFeatureLoaded` succeeds, so FSR stays blocked throughout an unload or its failure.
- At exit: the mode off for the last frame (on `AppExit`) or at once (a window closing), then at
  shutdown the mode off, null tags, the drain, `slFreeResources` for DLSS and DLSS-G, and the
  ordered `slShutdown`. A loaded save's handoff ends the old run with `std::process::exit`
  before `App::run` returns (`handoff.rs` `end_now`); it runs the same steps first
  (`dlss::end_now`, #2733): the render app taken back from the render thread, FSR frame
  generation's presenters stopped and joined, the mode off at once, the window's swapchain
  released, then the shutdown (waits without a limit, as every other exit does). Before #2733
  that exit left Streamline to the DLLs' detach [design], and in a player's runs every such
  exit with DLSS-G set up crashed in `nvngx_dlssg.dll` [observed: the player's logs and dumps,
  2026-11-15]; that the detach is what crashed is a [hypothesis] the change tests.

### Frame pacing

While DLSS-G runs, the frame cap (the page's, or variable refresh's) goes whole to Reflex's
`frameLimitUs` and frame pacing does not wait itself, so the driver and DLSS-G's pacer see one
limiter (`frame_pace::wait_for_cap`).

- **Reflex's limit holds the frames shown**, generated ones included, not the real frames: a limit
  of 13158 us (76) at 2x held 76 shown and 38 real [observed: the owner's run, 2026-11-23, 355
  real frames in 10 s, the overlay at 76 (38 rendered)]. So the real frames follow what DLSS-G
  presents by themselves: the cap divided by it while it generates (144 at 2x: 72), the whole cap
  in menus (DLSS-G's mode off) and while Streamline pauses generation itself with its status
  still `eOk` ("DLSS-G disabled: window not focused"), where real and presented frames are one.
- **History**: from #2317 to 2026-11-23 the cap was divided by the frames presented per real frame
  before it went to Reflex, first by the frames asked for, from #2731 by a divisor following
  `numFramesActuallyPresented` (`dlss/fg_cap.rs`, removed). With Reflex dividing as well, 2x held
  half the real frames meant (Variable refresh at 160 Hz: 76 shown, 38 real); #2731's menus and
  window-not-in-front cases (480 real and 480 presented in 10 s at a divided limit of 48
  [observed: a player's logs]) fit the same rule, as nothing was generated there.
- **Reflex's options are set again after every swapchain** (#2732). On Vulkan Streamline's Reflex
  hands the mode and the limit to the driver (`vkSetLatencySleepModeNV`) on the swapchain it
  tracks at the call, does nothing (and answers `eOk`) while it tracks none, and a swapchain it
  takes on later does not get them [documented: Streamline `vknvll2.cpp` `notifyCreateSwapchain`,
  `setSleepMode`; `reflexEntry.cpp` `slSetData`, `slHookVkCreateSwapchainKHRAfter`]. Options set
  before DLSS-G's swapchain (the start with frame generation on) were lost: real frames at 64-82 a
  second under a limit of 36 [observed: a player's logs, #2732]. The vendored wgpu-hal counts the
  swapchains it makes (`vulkan::swapchains_made`, moved on after `vkCreateSwapchainKHR` returns);
  when the count moved since the options were set, they are set again, once, with the line
  `DLSS-G: Reflex options set again for a new swapchain: … (swapchains made: N)`
  (`frame_token.rs`; a change of the options says `Reflex options: … (swapchains made: N)`, and a
  refused call, made again each frame, is said once while it repeats unchanged). This covers the
  start, the live switch and a resize's reconfigure alike.
- **The Generated frames row** says, in place of its note, when the limit divided by the choice
  (the real frames Reflex's limit on frames shown leaves) is under 60 a second ("4x at a limit of 144: 36 real frames a second, …"),
  which a player found choppy where 2x (72) was fine [observed: #2731].

## Checks

- Unit tests: the FFI layouts (`dlss/ffi.rs`), the merged requirements (`preflight.rs`), the
  frame numbers and PCL markers (`frame_token.rs`), the generate rule, options, window steps,
  state and ratio lines (`dlss/frame_gen.rs`), the Streamline latency calls (`nv_low_latency.rs`),
  the Reflex frame limit, whole on frames shown (`frame_pace.rs`), Reflex's options set again after a new swapchain (`dlss/frame_token.rs`),
  the row's choices and its few-real-frames note (`settings.rs`).
- Smoke (`tools/smoke/scenarios.toml`, `requires = ["NVF_DLSS_DIR"]`, `front = true`):
  `dlssg-on`, `dlssg-4x`, `dlssg-resize`, `dlssg-switch`, `dlssg-exit`, `dlssg-handoff-exit`,
  `dlssg-ui`.

  `dlssg-on`, `dlssg-4x`, `dlssg-cap-menu`, `dlssg-cap-start`, `dlssg-resize`, `dlssg-switch`,
  `dlssg-exit`, `dlssg-ui`. The scenarios measure with `--log-frame-times`, which leaves the
  page's Frame rate limit row out, so the two `dlssg-cap-*` ones give the limit with the test aid
  `--frame-cap 144`, in place from the start (before the window's swapchain).


Measured on an RTX 5070 Ti, driver 610.88, 1280x720, Goodsprings [observed, fork-notes
`fg-spike-results.md`]: 1.93 to 2.00 frames presented per real frame at 2x over DLSS Quality and
over FSR 3.1, 3.98 to 4.00 at 4x; no acquire stall through DLSS-G's proxy swapchain; resize in
play, the live switch and exit as designed.

## Limits

- Windows, Vulkan, NVIDIA RTX 40 series or newer; vsync off on Vulkan (Display sync Off, or
  Variable refresh, which presents with vsync off while DLSS-G is asked for).
- Only while the game's window is in front.
- The UI alpha treats translucent HUD pixels as opaque.
- The picture has been judged by eye only in passing (the HUD smear without the UI buffer before
  stage 4; screenshots show the real frame, never a generated one).
- Not tested on more than one GPU.
