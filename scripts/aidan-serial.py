import serial
import struct
import time
import random
import json

patterns_loaded = {}
patterns_final = []

with open("patterns.json") as f:
    patterns_loaded = json.load(f)

for key in patterns_loaded:

    points = sum(1 for entry in patterns_loaded[key] if "0x00000000" not in entry)
    index = 15
    home = 0
    enable = 1

    header = (
        ((index << 28) & 0xF0000000)
        | ((points << 20) & 0x0FF00000)
        | ((home << 19) & 0x00080000)
        | ((enable << 18) & 0x00040000)
        | (0x00002000)
    )

    cksum = header ^ (header >> 1)
    cksum = cksum ^ (cksum >> 2)
    cksum = cksum ^ (cksum >> 4)
    cksum = cksum ^ (cksum >> 8)
    cksum = cksum ^ (cksum >> 16)

    header = header ^ (cksum & 1)

    msg_header = []

    for i in range(0, 4):
        msg_header.append(int("{:08x}".format(header)[i * 2 : (i * 2) + 2], 16))

    pattern_id = list(patterns_loaded.keys()).index(key)
    colour_mask = 0

    if pattern_id % 3 == 0:
        colour_mask = 7 << 0
    elif pattern_id % 3 == 1:
        colour_mask = 7 << 3
    else:
        colour_mask = 7 << 6

    body = ((pattern_id << 24) & 0xFF000000) | ((colour_mask << 15) & 0x00FF8000)

    cksum = body ^ (body >> 1)
    cksum = cksum ^ (cksum >> 2)
    cksum = cksum ^ (cksum >> 4)
    cksum = cksum ^ (cksum >> 8)
    cksum = cksum ^ (cksum >> 16)

    body = body ^ (cksum & 1)

    msg_body = []

    for i in range(0, 4):
        msg_body.append(int("{:08x}".format(body)[i * 2 : (i * 2) + 2], 16))

    patterns_final.append([msg_header, msg_body])

    # Write the name of the pattern, then the pattern header and body all on the
    # same line
    print(key, msg_header, msg_body)

    # Then, print the data out, but in binary
    for byte in msg_header + msg_body:
        print("{:08b}".format(byte), end=" ")
    print()


ser = serial.Serial(
    port="/dev/serial0",
    baudrate=57600,
    parity=serial.PARITY_NONE,
    stopbits=serial.STOPBITS_ONE,
    bytesize=serial.EIGHTBITS,
)

print(ser.isOpen())
time.sleep(1)

counter = 0
state = 0
pattern_index = 0

while True:

    if counter < 3:
        state = 1
    elif counter >= 3 and counter < 6:
        state = 2
    elif counter >= 6 and counter < 9:
        state = 0
    else:
        counter = 0
    counter += 1

    projector_pattern = pattern_index
    pattern_index = (pattern_index + 1) % len(patterns_final)

    ser.write(patterns_final[projector_pattern][0])
    ser.write(patterns_final[projector_pattern][1])

    time.sleep(1)

# ser.write([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
ser.close()
