```
{
    "song": "path/to/song.mp3",
    "0": {
        "laser-1": [ // Might not be defined
            [x-pos, y-pos, r, g, b],
            [x-pos, y-pos, r, g, b],
        ],
        "laser-1-config": { // Might not be defined
          "home": true,
          "speed-profile": true
        }
        "light-1": 1.0 // Might not be defined, 1 through 7
    },
    "1000": {
        "laser-1": 0, // Turn it off (points 0, its id, enable true)
        "light-1": 0, // Assume anything that isn't zero is true
        "fog-1": 0.6
    }
}
```

0 is always a timestamp

Send reset packets (51 frames)
Need a homing packet at the beginning of each, 10 seconds each