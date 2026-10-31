# Halloween Show DMX Fixture Plan

This document defines the proposed DMX patch for the Halloween show. It uses one
DMX512 universe and assigns non-overlapping start addresses to the planned
fixtures, with room for four optional High End Systems Studio Spot 250 fixtures.

## Design choices

- **Universe:** Universe 1.
- **Addressing convention:** DMX channels are numbered 1–512.
- **Grouping:** practical/architectural lighting is placed first, followed by
  interactive and moving fixtures, then optional conventional moving heads and
  the laser projector.
- **Spacing:** start addresses are kept on odd channel numbers where practical.
  This is required by the locally built fog, turret, and bird fixtures because
  their address decoders permanently set address bit 0 to `1`.
- **Expansion:** the optional Studio Spot fixtures are placed after the required
  fixtures and use the documented 18-channel Studio Spot 250 personality.
- **Laser projector:** the EHaho L2400 is patched at its requested start address
  of 256 using its 32-channel professional DMX personality.

## Fixture schedule

| Fixture ID | Fixture | Quantity | Start address(es) | Footprint | End channel(s) | Status |
|---|---|---:|---:|---:|---:|---|
| PWM-1 | DMX Light Controller – 12 V PWM | 1 | 1 | 8 | 8 | Required |
| RGB-1 | DMX Light Controller – RGB / WS2812 | 1 | 17 | 24 | 40 | Required |
| RGB-2 | DMX Light Controller – RGB / WS2812 | 1 | 49 | 24 | 72 | Required |
| FOG-1 | DMX Fog Controller | 1 | 81 | 7 | 87 | Required |
| TUR-1 | Spotlight – Turret Receiver | 1 | 97 | 12 | 108 | Required |
| BIRD-1 | Bird Skeleton Animatronic | 1 | 113 | 13 | 125 | Required |
| SSPOT-1 | High End Systems Studio Spot 250 | 1 | 129 | 18 | 146 | Optional |
| SSPOT-2 | High End Systems Studio Spot 250 | 1 | 147 | 18 | 164 | Optional |
| SSPOT-3 | High End Systems Studio Spot 250 | 1 | 165 | 18 | 182 | Optional |
| SSPOT-4 | High End Systems Studio Spot 250 | 1 | 183 | 18 | 200 | Optional |
| LASER-1 | EHaho L2400 Stage Animation Laser Light | 1 | 256 | 32 | 287 | Required |

Channels **201–255** and **288–512** remain unassigned for future fixtures, spare
channels, or show-specific control data.

## Required fixture channel maps

### PWM-1 — DMX Light Controller, 12 V PWM

Start address: **1**; channels **1–8**.

| DMX channel | Function |
|---:|---|
| 1 | PWM output 1 |
| 2 | PWM output 2 |
| 3 | PWM output 3 |
| 4 | PWM output 4 |
| 5 | PWM output 5 |
| 6 | PWM output 6 |
| 7 | PWM output 7 |
| 8 | PWM output 8 |

The local fixture documentation defines eight consecutive intensity channels and
valid start addresses from 1 through 505.

### RGB-1 — DMX Light Controller, RGB / WS2812

Start address: **17**; channels **17–40**.

Each segment uses three channels in **RGB** order at the DMX controller:

| DMX channels | Function |
|---:|---|
| 17–19 | Segment 1: Red, Green, Blue |
| 20–22 | Segment 2: Red, Green, Blue |
| 23–25 | Segment 3: Red, Green, Blue |
| 26–28 | Segment 4: Red, Green, Blue |
| 29–31 | Segment 5: Red, Green, Blue |
| 32–34 | Segment 6: Red, Green, Blue |
| 35–37 | Segment 7: Red, Green, Blue |
| 38–40 | Segment 8: Red, Green, Blue |

The fixture's documented footprint is 24 channels, with valid start addresses
from 1 through 489.

### RGB-2 — DMX Light Controller, RGB / WS2812

Start address: **49**; channels **49–72**. It has the same channel map as
RGB-1, with its first segment on channels 49–51 and its eighth segment on
channels 70–72.

The separation between RGB-1 and RGB-2 leaves channels 41–48 as an intentional
eight-channel gap. This makes the two fixtures easier to identify and leaves a
small expansion space without changing either RGB fixture's patch.

### FOG-1 — DMX Fog Controller

Start address: **81**; channels **81–87**.

| DMX channel | Function |
|---:|---|
| 81 | Fog machine 1 command |
| 82 | Fog machine 2 command |
| 83 | Fog machine 3 command |
| 84 | Fog machine 4 command |
| 85 | Shared burst/cycle-on duration |
| 86 | Shared cycle-off duration |
| 87 | Master enable |

The fog controller has a seven-channel footprint and accepts odd start addresses
from 1 through 505. It is assigned to its own block rather than being placed
immediately after RGB-2 so the effect controls are visually separated in a
console patch.

### TUR-1 — Spotlight Turret Receiver

Start address: **97**; channels **97–108**.

| DMX channels | Turret | Functions |
|---:|---:|---|
| 97–99 | 1 | Pan, tilt, light |
| 100–102 | 2 | Pan, tilt, light |
| 103–105 | 3 | Pan, tilt, light |
| 106–108 | 4 | Pan, tilt, light |

The receiver uses four three-channel turret blocks. Its local address decoder
also produces odd addresses, so 97 is intentionally used instead of 96.

### BIRD-1 — Bird Skeleton Animatronic

Start address: **113**; channels **113–125**.

| DMX channel | Function |
|---:|---|
| 113 | Mode |
| 114 | Speed |
| 115 | Left wing |
| 116 | Right wing |
| 117 | Beak |
| 118 | LED dimmer |
| 119 | LED red |
| 120 | LED green |
| 121 | LED blue |
| 122 | LED effect |
| 123 | Effect rate |
| 124 | Sound / track |
| 125 | Volume |

The local firmware defines a 13-channel footprint. The repository's bench
configuration uses address 289, but the same offsets work at any valid configured
address; 113 is the proposed show address.

## Optional Studio Spot 250 fixtures

The High End Systems Studio Spot 250 standard DMX personality uses **18 DMX
channels per fixture**. The optional fixtures are therefore patched as follows:

| Fixture | Start address | Channel range |
|---|---:|---:|
| SSPOT-1 | 129 | 129–146 |
| SSPOT-2 | 147 | 147–164 |
| SSPOT-3 | 165 | 165–182 |
| SSPOT-4 | 183 | 183–200 |

These addresses are consecutive because the manual's Studio Spot 250 start-
channel sequence advances by 18 channels (`1`, `19`, `37`, and so on). Configure
each physical fixture for **DMX start-channel mode**, not fixture-number mode,
and verify that the fixture is operating in the Studio Spot 250 personality
before patching the console. The exact per-channel function names should be
taken from the fixture profile/manual loaded into the show-control software.

## Address-setting notes

### Locally built fixtures with odd-address decoding

The following fixtures must be assigned odd start addresses using their local
active-low DIP switches:

- FOG-1 — address 81
- TUR-1 — address 97
- BIRD-1 — address 113

For these fixtures, switch changes generally require a power cycle or reset to be
recognized. Confirm the address using the fixture's startup indication or a DMX
tester before connecting the controlled load.

### PWM and RGB controllers

The PWM and RGB controllers use active-low DIP switches and support arbitrary
valid start addresses within their footprint limits. Their selected addresses
should be:

- PWM-1: 1
- RGB-1: 17
- RGB-2: 49

### Studio Spot 250

Set each optional Studio Spot 250 to its assigned DMX start channel and keep all
four fixtures in the same 18-channel personality. Do not use the fixture's
fixture-number/link addressing mode for this patch.

### EHaho L2400 laser projector

Set LASER-1 to start address **256** and select the **32-channel professional DMX
mode**. The absolute channel mapping is listed in `CHANNEL-MAPPINGS.md`, with
practical value presets and laser-safety notes in `VALUE-MAPPINGS.md`.

The L2400 is a Class 3B laser product. Confirm beam clearance, termination,
interlocks, emergency shutdown, and applicable local laser-safety requirements
before enabling laser output.

## Validation checklist

1. Patch the required fixtures with the addresses in the schedule.
2. Confirm that no fixture reports an invalid address or DMX timeout.
3. Test each required fixture at the start and end of its assigned footprint.
4. For FOG-1, test the master-enable channel before connecting fog machines.
5. Test the Bird Skeleton's park behavior and audio/LED channels with the load
   safely powered and mechanically clear.
6. If the Studio Spots are added, verify one fixture at a time at channels
   129–146, 147–164, 165–182, and 183–200.
7. If LASER-1 is installed, verify channels 256–287 in 32-channel mode with the
   beam safely controlled.
8. Keep channels 201–255 and 288–512 unpatched until future fixtures are
   documented.

## Source documentation

Local fixture documentation used for this plan:

- `DMX Light Controller/12V PWM/README.md`
- `DMX Light Controller/WS2812/README.md`
- `Fog Machine/DMX Fog Controller/README.md`
- `Spotlight/README.md`
- `Bird Skeleton Animatronic/README.md`
- EHaho, `L2400 Stage Animation Laser Light` manual:
  `https://manuals.plus/ehaho/l2400-stage-animation-laser-light-manual`

The Studio Spot 250 channel count and consecutive addressing are based on the
High End Systems Studio Spot 250 / Studio Color 250 user manual. Its Studio Spot
250 start-channel table advances by 18 channels per fixture, confirming the
18-channel footprint used here.