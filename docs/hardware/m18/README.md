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
