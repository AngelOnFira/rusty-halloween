# DMX and Projector Control Protocol Specification

## Overview

This document outlines the message and control protocol for managing projectors, lighting systems, lasers, and DMX controllers. This spec describes how to format and send data for these devices using both DMX and GPIO protocols. The spec also includes how patterns and colors are selected for projectors, how DMX data is structured, and instructions on managing devices with JSON configurations.

All data is sent over the same UART bus on `/dev/serial0`. Devices know when to listen to data that is coming in.

---

## Packet Structure

### **Header Mode (32-bit Packet)**

The header packet is used for projector control. This packet contains information about the projector ID, point count, and various modes of operation, including configuration and boundary settings.

| Frame # | Bits         | Definition                                                                                              |
| ------- | ------------ | ------------------------------------------------------------------------------------------------------- |
| 0       | `0xF0000000` | **Projector ID** — Projectors reserve addresses `0x0 - 0x9`. `0xF` is reserved to index all projectors. |
|         | `0x0FF00000` | **Point Count**                                                                                         |
|         | `0x00040000` | **Enable** flag                                                                                         |
|         | `0x00080000` | **Home** flag                                                                                           |
|         | `0x00020000` | **Configuration Mode** flag                                                                             |
|         | `0x00010000` | **Draw Boundary** flag                                                                                  |
|         | `0x00008000` | **Oneshot** flag                                                                                        |
|         | `0x00007000` | **Speed Profile**                                                                                       |
|         | `0x00000001` | **Checksum**                                                                                            |

### **Pattern Selection (32-bit Packet)**

This packet defines the pattern ID and color mask used for projector visualizations.

| Frame # | Bits         | Definition                                                        |
| ------- | ------------ | ----------------------------------------------------------------- |
| 1       | `0xFF000000` | **Pattern ID** — Selected from a pattern lookup array             |
|         | `0x00FF8000` | **Color Mask** — 3-bit Red, 3-bit Green, 3-bit Blue (9-bit total) |
|         | `0x00000001` | **Checksum**                                                      |

### **Pattern Lookup Array**

The available patterns include the following:

```text
{bat, bow, bow_slow, candy, circle, circle_slow, clockwise_spiral_slow, counterclockwise_spiral_slow, crescent, ghost, gravestone_cross, hexagon, hexagon_slow, horizontal_lines_left_to_right_slow, horizontal_lines_right_to_left_slow, lightning_bolt, octagon, octagon_slow, parallelogram, parallelogram_slow, pentagon, pentagon_slow, pentagram, pentagram_slow, pumpkin, septagon_slow, square_large, square_large_slow, square_small, square_small_slow, star, star_slow, triangle_large, triangle_large_slow, triangle_small, triangle_small_slow, vertical_lines_bottom_to_top_slow, vertical_lines_top_to_bottom_slow}
```

These patterns are referenced by their respective IDs in the JSON.

---

## DMX Data Transmission

DMX data is sent in 8-bit packets. Each message includes a header to identify the addressed controller and selected DMX universe. Data for multiple DMX channels is sent sequentially.

### **DMX Header Mode (8-bit Packet)**

The header packet identifies the controller and universe.

| Byte # | Bits   | Definition                                                                    |
| ------ | ------ | ----------------------------------------------------------------------------- |
| 0      | `0xF0` | **Controller ID** — DMX Controllers reserve addresses `0xA-0xE`.              |
|        | `0x0F` | **Universe Selector** — Used to extend the number of addressable DMX devices. |

### **DMX Data (8-bit Packet)**

DMX data is straightforward, containing 255 bytes for DMX channel data. Indexing starts at 1, not 0. This means that a hardware device with ID 1 would start writing its data the byte after the header. Writing again to the DMX controller requires that you address it again and send another 255 bytes.

| Byte #   | Bits   | Definition                                                              |
| -------- | ------ | ----------------------------------------------------------------------- |
| 1 -> 255 | `0xFF` | **DMX Channel Data** — Forward the DMX data as required by the channel. |

---

## JSON Configuration

Devices and their protocols are mapped in JSON format. The example below shows how to define each device's protocol (GPIO, DMX, SERIAL) and other parameters, such as ID and format.

```json
{
  "light-0": { "protocol": "GPIO",
               "pin": 12 },
  "light-1": { "protocol": "GPIO",
               "pin": 15 },
  ...
  "light-8": {
    "protocol": "DMX",
    "format": [ /* DMX channel format */ ],
    "ID": 0
  },
  ...
  "laser-1": { "protocol": "SERIAL",
               "ID": 1},
  "laser-2": { "protocol": "SERIAL",
               "ID": 2},
  ...
  "laser-6": {
    "protocol": "DMX",
    "format": [ /* DMX channel format */ ],
    "ID": 1
  },
  ...
  "turret-0": {
    "protocol": "DMX",
    "format": [ /* DMX channel format */ ],
    "ID": 2
  }
}
```

You can see the 2024 hardware spec [here](https://gist.github.com/AngelOnFira/5fded8e144a2c716e5685398c16081d1).

### **DMX Format**

For DMX devices, the format array provides a lookup for keywords in the instruction JSON. When the index of the keyword is searched, it will return a channel value, that when combined with the device ID will give the channel address for that value. The number of channels used by a device is equal to the length of the format array.

Ex:

ID: 5
format: ["state", "", "library", "pattern", "" ""]

This example format block defines a device that takes 6 channels of DMX data; state, library, and pattern. The index of these in the DMX packet is then; (ID + index) 5, 7, and 8. Channels 6, 9, and 10 should be then set to 0x00. Remember, when updating the DMX data to send, you need to retain old states and override when changing values only.

### **DMX Device Data Mapping**

For DMX devices, the ID serves as an offset for the channel number. For example:

- A device with `ID: 1` and 3 channels of data will write to channels 1, 2, and 3.
- A device with `ID: 5` and 32 channels of data will write to channels 5-37.

If there are gaps between devices (e.g., if a device occupies channels 1-3 and another device starts at channel 5), you need to generate blank packets to "fill" the space.

### **GPIO Device States**

For GPIO-controlled lights, states are defined as follows:

- `0 = Off`
- `1 = On`
- `2 = Flashing` — The light flashes at a fixed 50ms interval

---

This spec is intended to give you full control over various devices using DMX, GPIO, and SERIAL protocols. Make sure to adhere to the structure provided for consistent device communication and handling.

### **End of Show**

At the end of a show the serial projectors must be sent a homing packet as previously, and a packet of all zeroes should be sent to the DMX controller.
