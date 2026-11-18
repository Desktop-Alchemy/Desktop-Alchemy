# M18 Hardware Reference

The reference adapter targets M18 with USB VID/PID `6603:1012`; firmware behavior is device-specific. A reference firmware identifier is `V3.M18V3_E_HXJDF.03.021`. Interface presence, declared model support, and physical effects are separate properties; check the actual target rather than generalizing from another firmware/model.

## Display and input layout

| Property     | Adapter contract                                           |
| ------------ | ---------------------------------------------------------- |
| Input        | 18 physical keys, press and release                        |
| Display keys | 5 columns × 3 rows, each 64×64                             |
| Bottom keys  | 16, 17, 18 left-to-right; no displays                      |
| Background   | 480×272                                                    |
| RGB          | 24 LEDs; whole/per-LED RGB output and proportional dimming |

Public numbering is:

```text
01  02  03  04  05
06  07  08  09  10
11  12  13  14  15
    16      17      18
```

The pinned C SDK numbers image slots from the bottom row. Image/GIF output maps `1–5 ↔ 11–15` and leaves 6–10 unchanged through `sdk::m18_key_index`. The adapter normalizes input ACK codes once: displays 1–15 and bottom raw codes `0x25`, `0x30`, `0x31` to 16–18. Do not flip normalized input again. UI/plugins use public IDs only.

Bottom keys default to previous/home/next page. Keys can perform navigation, launch actions, or no action. Rotary/touch input, other models' private settings, and a main-screen M18 GIF are outside this adapter's exposed contract. Do not infer M18 V2 RGB support from V3 behavior.

## Images, pages, and backgrounds

Screen brightness is 0–100. Hardware tests include grayscale, solid colors, gradients, checkerboard, and low/high luminance steps.

Prepare and encode a complete page before upload, submit its key images consecutively, and refresh once at the end. Image packets use BAT framing and the refresh ends in STP. A final marker does not establish atomic firmware page swapping; native page-number switching is not exposed.

Ordinary backgrounds use temporary RAM frames through `transport_set_background_frame_stream`. Boot backgrounds use persistent storage and require a separate confirmation. Do not use persistent writes for page switching or automated tests of the user's display. Key GIFs are host-decoded/timed and sent by the same C SDK worker; `running=true` or a successful receipt does not prove correct animation, placement, or frames.

### Page transitions

The module's optional `page_transition` configuration stores `mode` (`none`, `fade_in`, `slide_left`, `slide_right`, `slide_up`, `slide_down`, `slide_random`) and `duration_ms` (150–1000, default 300). The UI groups directional modes under Slide and shows a direction selector only for that animation. Random resolves uniformly to one fixed direction per transition before encoding any frames, shared by the grid and background. Existing directional modes remain compatible. Missing configuration keeps transitions off. Module operations `page_transition.get` and `page_transition.set` use the current connection and atomic profile persistence, without changing pages or plugin permissions. Writes carry `{settings, expected}`; the adapter compares the previously read transition and the host atomically compares the full module configuration during commit. Conflicts retain the editor draft until the user reads the latest settings. Unknown sibling module fields are retained.

Core marks only navigation between distinct pages as transition intent. The M18 adapter prepares the entire batch, reads its module settings, and revalidates the batch before submission. Fade in dims screen brightness to zero, submits the usual complete page with one final refresh, then schedules brightness toward the page's selected level using the same SDK worker. No intermediate page images, native page-number API, persistent background writes, or sleeps inside page upload are used. The first fade sample starts after SDK submission, not confirmed drawing completion.

Each handle owns one fade with a latest brightness sample at most every 25 ms; late ticks skip intermediate values. Another page or visual operation restores/cancels the previous fade, explicit screen-brightness commands supersede it, and normal shutdown restores its target before releasing handles. LED-only traffic does not cancel the fade. Failed dim/page submission attempts brightness restoration and reports any restoration failure. Disabled screen-brightness capability or a zero target skips the effect. Reconnects and same-page refreshes do not animate. Smoothness and firmware brightness response require physical observation; mock submission and receipts establish ordering only.

Slides use the last JPEG images accepted by this SDK owner as their source, including the latest submitted GIF/widget images. This cache is not hardware readback. The 15 tiles form a continuous logical 320×192 grid. Old and new grids are placed adjacent horizontally or vertically; a smoothstep viewport moves between them and is split back into public keys 1–15. The separate 480×272 temporary background follows the same normalized progress. No physical key positions or input mappings change.

All intermediate JPEGs and the exact destination batch are prepared before any animation upload, with a combined 64 MiB encoded budget. An unknown/incomplete source or an exceeded optional animation budget falls back to the ordinary destination upload. A per-handle presentation owns pre-encoded frames and one pending host receipt. Its target cadence is one frame per 50 ms (up to 20 frames/s); absolute elapsed time skips late intermediate frames and always selects the original destination last. Every frame is an uninterrupted image batch with one refresh, submitted through the existing SDK worker. Receipts complete only after the destination batch is accepted. SDK acceptance does not measure completed USB transmission or drawing, and already queued SDK operations may not be retractable.

The host retains/coalesces metrics until the presenting page completes, while explicit commands and other devices continue between frame uploads. GIF playback pauses during slides; destination GIF delays start when installed. New page generations invalidate the remaining old frames and can begin from the last accepted intermediate images. An explicit visual/brightness operation finishes a still-current destination before taking control. Each frame rechecks the originating batch's connection, page, source, and resource generations. Worker errors/disconnects drop the presentation receipt; an upload failure invalidates the source cache. Normal shutdown allows up to two seconds for an in-flight page to settle before cancelling unsent work. No per-frame public receipts, additional transport, persistent writes, or sleeps within image batches are introduced.

Unchanged backgrounds are omitted from intermediate encoding. During slide submission, static images identical to the owner's accepted JPEG cache are skipped, including the destination background; the final refresh and GIF installation remain. This avoids an unnecessary full-background upload at the end. One batch completion preserves every receipt's sent/acknowledged history in a single log append and snapshot publication, reducing synchronous end-of-slide work. The runtime log records submitted frame count/bytes, slowest/final frame SDK-call time, and receipt-publication time. These timings measure host SDK acceptance and application bookkeeping, never completed bus traffic or display refresh.

### Boot-image readback

The pinned public interface has no operation to read boot images, backgrounds, or displayed content. Image operations write/clear; input `read` and generic HID `transport_read` do not imply an image-download protocol.

The app cannot download an existing hardware image or reconstruct images previously written by another program from its own library. This is a public-interface limit, not proof of a fundamental firmware impossibility. Implementing readback would require a documented vendor protocol or separate authorized protocol research.

Imported originals remain in local `assets/`, but a local copy is not current hardware readback. Configuration does not associate the last persistent boot-image source with each device; even a retained source does not prove it remains installed or reproduce SDK-encoded storage bytes.

The vendor's `setBackgroundImgFile` describes a persistent ROM boot image. Standby, sleep, and client-disconnected presentation are separate firmware behaviors and must not be assumed equivalent. Pinned sources/licenses are in `vendor/device-sdk/` and [Device SDK](../../DEVICE-SDK.md).

## LED compatibility and dimming

The native `setLEDBrightness` operation is documented as 0–255 but changes colors incorrectly on the reference hardware. The adapter disables it and scales RGB instead:

```text
output component = round(original component × led_brightness / 255)
```

Store original colors separately to prevent cumulative precision loss. Raising brightness from zero restores those colors. Whole/per-LED output follows the same rule, and rapid color/brightness changes coalesce complete output. This is not native PWM control; low integer values can quantize color ratios.

`led_mode` supports solid, static per-LED, breathing, color cycle, rainbow, wave, comet, and firmware default. Software animation calculates complete 24-color frames, at most one every 50 ms (20 FPS). Speed 1–10 corresponds to roughly 9–1.8 seconds per cycle. Every frame uses RGB scaling.

Color cycle changes the whole ring together; rainbow distributes/rotates hues; breathing varies the saved primary color; wave moves a brightness wave; comet moves a bright head and four fading tail LEDs. Cycle/rainbow use an internal spectrum, while breathing/wave/comet use the primary color.

Restore saved mode/color/speed/brightness on restart/reconnect. Firmware default uses `resetLEDColor` and cannot be reliably software-dimmed. Normal exit restores that firmware effect. Physical appearance and per-LED ordering need target-specific observation; SDK receipts are not visual verification. Plugin overrides never persist these saved values.

See [USB capture](USB-CAPTURE.md) for descriptor-based identification and measurements. Bus submission establishes host-side order/timing, not completed drawing. Performance comparisons require identical encoded images and measurement boundaries.
