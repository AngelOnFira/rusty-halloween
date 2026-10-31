# Halloween Show DMX Channel Mappings

This document is the channel-by-channel reference for the proposed Universe 1
patch in `README.md`. DMX channels are shown using the standard 1–512 numbering
scheme. The **offset** is relative to the fixture's configured start address.

## Patch summary

| Fixture ID | Fixture | Start address | Channel range | Status |
|---|---|---:|---:|---|
| PWM-1 | DMX Light Controller – 12 V PWM | 1 | 1–8 | Required |
| RGB-1 | DMX Light Controller – RGB / WS2812 | 17 | 17–40 | Required |
| RGB-2 | DMX Light Controller – RGB / WS2812 | 49 | 49–72 | Required |
| FOG-1 | DMX Fog Controller | 81 | 81–87 | Required |
| TUR-1 | Spotlight – Turret Receiver | 97 | 97–108 | Required |
| BIRD-1 | Bird Skeleton Animatronic | 113 | 113–125 | Required |
| SSPOT-1 | High End Systems Studio Spot 250 | 129 | 129–146 | Optional |
| SSPOT-2 | High End Systems Studio Spot 250 | 147 | 147–164 | Optional |
| SSPOT-3 | High End Systems Studio Spot 250 | 165 | 165–182 | Optional |
| SSPOT-4 | High End Systems Studio Spot 250 | 183 | 183–200 | Optional |
| LASER-1 | EHaho L2400 Stage Animation Laser Light | 256 | 256–287 | Required |

## PWM-1 — DMX Light Controller, 12 V PWM

**Start address:** 1 · **Footprint:** 8 channels · **Range:** 1–8

| Offset | DMX channel | Function | Value/behavior |
|---:|---:|---|---|
| 1 | 1 | PWM output 1 | 0 = off; 255 = full output |
| 2 | 2 | PWM output 2 | 0 = off; 255 = full output |
| 3 | 3 | PWM output 3 | 0 = off; 255 = full output |
| 4 | 4 | PWM output 4 | 0 = off; 255 = full output |
| 5 | 5 | PWM output 5 | 0 = off; 255 = full output |
| 6 | 6 | PWM output 6 | 0 = off; 255 = full output |
| 7 | 7 | PWM output 7 | 0 = off; 255 = full output |
| 8 | 8 | PWM output 8 | 0 = off; 255 = full output |

## RGB-1 — DMX Light Controller, RGB / WS2812

**Start address:** 17 · **Footprint:** 24 channels · **Range:** 17–40

| Offset | DMX channel | Function |
|---:|---:|---|
| 1–3 | 17–19 | Segment 1: Red, Green, Blue |
| 4–6 | 20–22 | Segment 2: Red, Green, Blue |
| 7–9 | 23–25 | Segment 3: Red, Green, Blue |
| 10–12 | 26–28 | Segment 4: Red, Green, Blue |
| 13–15 | 29–31 | Segment 5: Red, Green, Blue |
| 16–18 | 32–34 | Segment 6: Red, Green, Blue |
| 19–21 | 35–37 | Segment 7: Red, Green, Blue |
| 22–24 | 38–40 | Segment 8: Red, Green, Blue |

Each segment's RGB value is applied to the seven WS2812 pixels in that segment.

## RGB-2 — DMX Light Controller, RGB / WS2812

**Start address:** 49 · **Footprint:** 24 channels · **Range:** 49–72

| Offset | DMX channel | Function |
|---:|---:|---|
| 1–3 | 49–51 | Segment 1: Red, Green, Blue |
| 4–6 | 52–54 | Segment 2: Red, Green, Blue |
| 7–9 | 55–57 | Segment 3: Red, Green, Blue |
| 10–12 | 58–60 | Segment 4: Red, Green, Blue |
| 13–15 | 61–63 | Segment 5: Red, Green, Blue |
| 16–18 | 64–66 | Segment 6: Red, Green, Blue |
| 19–21 | 67–69 | Segment 7: Red, Green, Blue |
| 22–24 | 70–72 | Segment 8: Red, Green, Blue |

Each segment's RGB value is applied to the seven WS2812 pixels in that segment.

## FOG-1 — DMX Fog Controller

**Start address:** 81 · **Footprint:** 7 channels · **Range:** 81–87

| Offset | DMX channel | Function | Value/behavior |
|---:|---:|---|---|
| 1 | 81 | Fog machine 1 command | 0–9 off/arm; 10–99 idle; 100–149 burst; 150–199 cycle; 200–255 continuous |
| 2 | 82 | Fog machine 2 command | Same command ranges as channel 81 |
| 3 | 83 | Fog machine 3 command | Same command ranges as channel 81 |
| 4 | 84 | Fog machine 4 command | Same command ranges as channel 81 |
| 5 | 85 | Shared burst/cycle-on duration | 0.25–30 seconds, mapped linearly |
| 6 | 86 | Shared cycle-off duration | 2–300 seconds, mapped linearly |
| 7 | 87 | Master enable | 0–127 disabled; 128–255 enabled |

The fog controller uses active-low relay outputs. The master-enable channel must
be active before normal fog operation is permitted.

## TUR-1 — Spotlight Turret Receiver

**Start address:** 97 · **Footprint:** 12 channels · **Range:** 97–108

| Offset | DMX channel | Turret | Function | Value/behavior |
|---:|---:|---:|---|---|
| 1 | 97 | 1 | Pan | Servo position, limited to 0–180 |
| 2 | 98 | 1 | Tilt | Input limited to 45–180, then inverted |
| 3 | 99 | 1 | Light | 0 off; 1 on; 2–255 flashing |
| 4 | 100 | 2 | Pan | Servo position, limited to 0–180 |
| 5 | 101 | 2 | Tilt | Input limited to 45–180, then inverted |
| 6 | 102 | 2 | Light | 0 off; 1 on; 2–255 flashing |
| 7 | 103 | 3 | Pan | Servo position, limited to 0–180 |
| 8 | 104 | 3 | Tilt | Input limited to 45–180, then inverted |
| 9 | 105 | 3 | Light | 0 off; 1 on; 2–255 flashing |
| 10 | 106 | 4 | Pan | Servo position, limited to 0–180 |
| 11 | 107 | 4 | Tilt | Input limited to 45–180, then inverted |
| 12 | 108 | 4 | Light | 0 off; 1 on; 2–255 flashing |

## BIRD-1 — Bird Skeleton Animatronic

**Start address:** 113 · **Footprint:** 13 channels · **Range:** 113–125

| Offset | DMX channel | Function | Value/behavior |
|---:|---:|---|---|
| 0 | 113 | Mode | 0–15 static positioning; higher ranges select animated effects |
| 1 | 114 | Speed | Animation phase and servo movement speed |
| 2 | 115 | Left wing | Position mapped between calibrated limits |
| 3 | 116 | Right wing | Position mapped between calibrated limits |
| 4 | 117 | Beak | 128–255 maps closed/resting to open; values below 128 clamp closed |
| 5 | 118 | LED dimmer | Scales RGB output |
| 6 | 119 | LED red | RGB intensity |
| 7 | 120 | LED green | RGB intensity |
| 8 | 121 | LED blue | RGB intensity |
| 9 | 122 | LED effect | Selects the LED effect range |
| 10 | 123 | Effect rate | Controls effect timing or intensity where applicable |
| 11 | 124 | Sound / track | Track number, command, or configured named-track alias |
| 12 | 125 | Volume | 0–255 mapped to DFPlayer volume 0–30 |

The bird firmware defines its channel offsets from zero, so offset 0 is the
fixture's first DMX channel.

## SSPOT-1 — High End Systems Studio Spot 250

**Start address:** 129 · **Footprint:** 18 channels · **Range:** 129–146

| Offset | DMX channel | Function |
|---:|---:|---|
| 1 | 129 | Pan – high resolution/coarse |
| 2 | 130 | Pan – low resolution/fine |
| 3 | 131 | Tilt – high resolution/coarse |
| 4 | 132 | Tilt – low resolution/fine |
| 5 | 133 | Color function |
| 6 | 134 | Color position |
| 7 | 135 | Litho function |
| 8 | 136 | Litho position |
| 9 | 137 | Litho rotate |
| 10 | 138 | Effects position |
| 11 | 139 | Effects rotate |
| 12 | 140 | Focus |
| 13 | 141 | Iris |
| 14 | 142 | Shutter |
| 15 | 143 | Dimmer |
| 16 | 144 | MSpeed / motor speed |
| 17 | 145 | Macro / LAD |
| 18 | 146 | Control |

## SSPOT-2 — High End Systems Studio Spot 250

**Start address:** 147 · **Footprint:** 18 channels · **Range:** 147–164

| Offset | DMX channel | Function |
|---:|---:|---|
| 1 | 147 | Pan – high resolution/coarse |
| 2 | 148 | Pan – low resolution/fine |
| 3 | 149 | Tilt – high resolution/coarse |
| 4 | 150 | Tilt – low resolution/fine |
| 5 | 151 | Color function |
| 6 | 152 | Color position |
| 7 | 153 | Litho function |
| 8 | 154 | Litho position |
| 9 | 155 | Litho rotate |
| 10 | 156 | Effects position |
| 11 | 157 | Effects rotate |
| 12 | 158 | Focus |
| 13 | 159 | Iris |
| 14 | 160 | Shutter |
| 15 | 161 | Dimmer |
| 16 | 162 | MSpeed / motor speed |
| 17 | 163 | Macro / LAD |
| 18 | 164 | Control |

## SSPOT-3 — High End Systems Studio Spot 250

**Start address:** 165 · **Footprint:** 18 channels · **Range:** 165–182

| Offset | DMX channel | Function |
|---:|---:|---|
| 1 | 165 | Pan – high resolution/coarse |
| 2 | 166 | Pan – low resolution/fine |
| 3 | 167 | Tilt – high resolution/coarse |
| 4 | 168 | Tilt – low resolution/fine |
| 5 | 169 | Color function |
| 6 | 170 | Color position |
| 7 | 171 | Litho function |
| 8 | 172 | Litho position |
| 9 | 173 | Litho rotate |
| 10 | 174 | Effects position |
| 11 | 175 | Effects rotate |
| 12 | 176 | Focus |
| 13 | 177 | Iris |
| 14 | 178 | Shutter |
| 15 | 179 | Dimmer |
| 16 | 180 | MSpeed / motor speed |
| 17 | 181 | Macro / LAD |
| 18 | 182 | Control |

## SSPOT-4 — High End Systems Studio Spot 250

**Start address:** 183 · **Footprint:** 18 channels · **Range:** 183–200

| Offset | DMX channel | Function |
|---:|---:|---|
| 1 | 183 | Pan – high resolution/coarse |
| 2 | 184 | Pan – low resolution/fine |
| 3 | 185 | Tilt – high resolution/coarse |
| 4 | 186 | Tilt – low resolution/fine |
| 5 | 187 | Color function |
| 6 | 188 | Color position |
| 7 | 189 | Litho function |
| 8 | 190 | Litho position |
| 9 | 191 | Litho rotate |
| 10 | 192 | Effects position |
| 11 | 193 | Effects rotate |
| 12 | 194 | Focus |
| 13 | 195 | Iris |
| 14 | 196 | Shutter |
| 15 | 197 | Dimmer |
| 16 | 198 | MSpeed / motor speed |
| 17 | 199 | Macro / LAD |
| 18 | 200 | Control |

## LASER-1 — EHaho L2400 Stage Animation Laser Light

**Start address:** 256 · **Footprint:** 32 channels · **Range:** 256–287

Configure the unit for the EHaho L2400 **32-channel professional DMX mode**.
The fixture also has a separate simple line-pattern DMX mode, which is not
compatible with this patch.

Channels 1–12 are taken from the printed manual ("06/ Channel menu for
professional DMX mode", pages 6–7). This is the same layout as the `lp-1`
projector in last year's hardware config, so the 2022–2024 show data is sent
unchanged. The keyword column is the name used in `config/hardware.json` and in
show files.

| Offset | DMX channel | Keyword | Function | Settings / values |
|---:|---:|---|---|---|
| 1 | 256 | `state` | ON/OFF lamp | 0 lights off; 1–99 DMX manual; 100–199 DMX dynamic sound control; 200–219 tune the dynamic program library; 221–249 sound-control dynamic program library; 250–255 lights off |
| 2 | 257 | `size` | Out-of-bounds mode and pattern size | 0–49 out-of-bounds parts blank; 50–99 out-of-bounds pattern returns; 100–149 out-of-bounds pattern folds; 150–199 out-of-bounds crossing (in each of these ranges, a larger value gives a smaller pattern); 200–255 out-of-bounds blanking, where a larger value gives a bigger pattern |
| 3 | 258 | `gallery` | Gallery selection | 0–239 beam gallery (0); 244–255 animation gallery (1) |
| 4 | 259 | `pattern` | Pattern selection | Each value selects a pattern. When channel 1 is 220–249, 0 plays the dynamic pictures in sequence and 1 selects a dynamic picture individually; every two values select one dynamic picture |
| 5 | 260 | `zoom` | Pattern zooming | 0–127 static size; 128–159 dynamic zoom in; 160–191 dynamic zoom out; 192–255 dynamic flip zooming |
| 6 | 261 | `rotation` | Pattern rotation | 0–127 static rotation; 128–192 dynamic inversion; 193–255 dynamic inversion |
| 7 | 262 | `h-move` | Horizontal movement | 0–127 static translation; 128–159 dynamic push-up wave; 160–191 dynamic push-down wave; 192–223 dynamic left shift; 224–255 dynamic right shift |
| 8 | 263 | `v-move` | Vertical movement | 0–127 static vertical movement; 128–159 dynamic right-push wave; 160–191 dynamic left-push wave; 192–223 dynamic move up; 224–255 dynamic move down |
| 9 | 264 | `h-zoom` | Horizontal zooming | 0–127 static size; 128–159 dynamic push-up distortion; 160–191 dynamic push-down distortion; 192–223 dynamic zooming; 224–255 dynamic flip zooming |
| 10 | 265 | `v-zoom` | Vertical zooming | 0–127 static size; 128–159 dynamic right-push distortion; 160–191 dynamic left-push distortion; 192–223 dynamic zoom; 224–255 dynamic flip zooming |
| 11 | 266 | `forced-colour` | Forced color setting | 0 primary color; 1–255 change color every N points |
| 12 | 267 | `colour` | Discoloration | 0–7 primary; 8–15 white; 16–23 red; 24–31 yellow; 32–39 green; 40–47 indigo; 48–55 blue; 56–63 purple; 64–95 full-picture red, green and blue change; 96–127 full-picture yellow, indigo and purple change; 128–159 full-picture colorful change; 160–191 colorful change; 192–223 forward color movement; 224–255 reverse color movement |

Manual notes:

- Channel 1: channel 3 selects the animation or beam library, and channel 4
  selects an individual program in it. Only dynamic beam effects respond to
  sound control; flash animations don't, and only flash automatically.
- The printed ranges have gaps: channel 1 skips 220, and channel 3 skips
  240–243. The purple range on channel 12 is printed as "65-63"; it's listed
  here as 56–63, between blue and the first color-change range.

### Last year's values on this fixture

The 2022–2024 shows only use four keywords on `lp-1`. Their values fall inside
the manual's ranges as follows:

| Keyword | Values used | Meaning on the L2400 |
|---|---|---|
| `state` | 0, 1 | 0 lights off; 1 DMX manual |
| `gallery` | 0, 255 | 0 beam gallery; 255 animation gallery |
| `pattern` | 0–232 | Pattern within the selected gallery |
| `colour` | 0, 2, 10, 18, 26, 34, 42, 50, 58, 66, 162, 194, 226 | Primary, primary, white, red, yellow, green, indigo, blue, purple, full-picture red/green/blue change, colorful change, forward color movement, reverse color movement |

### Channels 13–32 (provisional)

The photographed manual pages only cover channels 1–12. The rows below come
from an earlier online reference that could not be verified, and that
reference had channels 1–4 wrong, so check these against the remaining manual
pages or a DMX tester before using them in a show.

| Offset | DMX channel | Keyword | Function | Settings / values |
|---:|---:|---|---|---|
| 13 | 268 | `nodes` | Node highlighting | 0–63 brighten nodes; 64–127 broken lines; 128–159 scanning line; 224–255 reserved |
| 14 | 269 | `node-expand` | Node expansion | 0–255 expansion points or point-by-point erasure; works with channel 15 |
| 15 | 270 | `drawing` | Gradual drawing | 0–63 forward; 64–127 reverse; 128–159 effect A; 160–191 effect B; 192–223 effect C; 224–255 effect D |
| 16 | 271 | `distortion-1` | Distortion degree 1 | 0–255 distortion amount |
| 17 | 272 | `distortion-2` | Distortion degree 2 | 0–255 distortion amount for the second pattern |
| 18 | 273 | `p2-select` | Second lamp pattern selection | 0 off; 1–255 select/enable second pattern |
| 19 | 274 | `p2-size` | Out-of-bounds mode / pattern size | 0–49 blank; 50–99 return; 100–149 fold; 150–199 crossing; 200–255 blanking |
| 20 | 275 | `p2-gallery` | Pattern library selection | Beam library is default in dual-pattern mode; may have no function |
| 21 | 276 | `p2-pattern` | Pattern selection | Internal pattern index; dynamic ranges support sequence or individual selection |
| 22 | 277 | `p2-zoom` | Pattern zooming | 0–127 static; 128–159 zoom in; 160–191 zoom out; 192–255 flip zoom, with speed increasing by value |
| 23 | 278 | `p2-rotation` | Pattern rotation | 0–127 static; 128–255 dynamic inversion/rotation |
| 24 | 279 | `p2-h-move` | Horizontal movement | 0–127 static; 128–159 upward wave; 160–191 downward wave; 192–223 left shift; 224–255 right shift |
| 25 | 280 | `p2-v-move` | Vertical movement | 0–127 static; 128–159 right-push wave; 160–191 left-push wave; 192–224 move up; 225–255 move down |
| 26 | 281 | `p2-h-flip` | Horizontal flip | 0–127 static; 128–159 push-up distortion; 160–191 push-down distortion; 192–223 flip |
| 27 | 282 | `p2-v-flip` | Vertical flip | 0–127 static; 128–159 right-push distortion; 160–191 left-push distortion; 192–255 flip/flip zoom |
| 28 | 283 | `p2-forced-colour` | Forced color setting 2 | 0 primary color; 1–255 change color every N points |
| 29 | 284 | `p2-colour` | Color change 2 | 0–7 primary; 8–15 white; 16–23 red; 24–31 yellow; 32–39 green; 40–47 indigo; 48–55 blue; 56–63 purple; 64–255 dynamic color modes |
| 30 | 285 | `p2-nodes` | Node highlighting / array guide | 0–63 brighten; 64–127 broken lines; 128–255 scanning/array guide; 161–255 can use channels 28–29 for array color |
| 31 | 286 | `p2-node-expand` | Node expansion 2 | 0–255 expansion or delay after expansion; can show point-by-point erasure |
| 32 | 287 | `p2-drawing` | Gradual drawing 2 | 0–63 forward; 64–127 reverse; 128–159 effect A; 160–191 effect B; 192–223 effect C; 224–255 effect D |

## Notes and references

- The PWM, RGB, fog, turret, and bird mappings are based on the local README
  files and firmware in this repository.
- The Studio Spot 250 mapping uses the standard 18-channel `SPOT` personality.
  The High End Systems user manual lists the Studio Spot 250 construct/channel
  assignments in Table 3-1 on printed pages 3-3 through 3-4.
- In the Studio Spot 250 mapping, channels 1 and 2 form the pan coarse/fine pair,
  and channels 3 and 4 form the tilt coarse/fine pair.
- Set all optional Studio Spot fixtures to the same `SPOT` personality and use
  DMX start-channel control mode.
- The EHaho L2400 channels 1–12 come from the printed manual's professional
  DMX mode tables (pages 6–7). Channels 13–32 are still from an unverified
  online reference
  (`https://manuals.plus/ehaho/l2400-stage-animation-laser-light-manual`) and
  should be checked against the remaining manual pages.
