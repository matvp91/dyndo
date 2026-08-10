# `one-second-silence-aac.mp4`

A 1.2 KiB fragmented MP4 containing one second of mono AAC silence.
It is intentionally tiny and only exercises container probing and byte-range reads.

`three-frame-black-h264.mp4` is a 1.7 KiB fragmented MP4 containing three
16×16 black H.264 frames. Its deliberately variable frame durations ensure the
fragment carries explicit sample timing, which exercises video frame-rate probing.

Regenerate it from the repository root with:

```sh
ffmpeg -f lavfi -i anullsrc=r=8000:cl=mono -t 1 -c:a aac -b:a 8k /tmp/dyndo-audio-source.mp4
MP4Box -dash 1000 -frag 1000 -rap -profile dashavc264:onDemand \
  -out /tmp/dyndo-manifest.mpd /tmp/dyndo-audio-source.mp4
mv /tmp/dyndo-audio-source_dashinit.mp4 crates/dyndo-core/tests/fixtures/one-second-silence-aac.mp4

ffmpeg -f lavfi -i color=c=black:s=16x16:r=4:d=1 -vf "select='not(eq(n,2))'" \
  -vsync vfr -frames:v 3 -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
  /tmp/dyndo-video-source.mp4
MP4Box -dash 1000 -frag 1000 -rap -no-frags-default -profile dashavc264:onDemand \
  -out /tmp/dyndo-video-manifest.mpd /tmp/dyndo-video-source.mp4
mv /tmp/dyndo-video-source_dashinit.mp4 crates/dyndo-core/tests/fixtures/three-frame-black-h264.mp4
```
