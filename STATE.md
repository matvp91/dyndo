# State

## Known issues / follow-ups

- **HLS audio grouping is by fourcc, not full codec.** Two AAC tracks with different object types (e.g. `mp4a.40.2` LC vs `mp4a.40.5` HE-AAC) collapse into one `"mp4a"` group; the group's `CODECS` becomes whichever track was seen first. Revisit if HE-AAC is ever in play.
