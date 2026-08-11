# `one-second-silence-aac.mp4`

A 1.2 KiB fragmented MP4 containing one second of mono AAC silence.
It is intentionally tiny and only exercises container resolution and byte-range reads.

`three-frame-black-h264.mp4` is a 1.7 KiB fragmented MP4 containing three
16×16 black H.264 frames. Its deliberately variable frame durations ensure the
fragment carries explicit sample timing, which exercises video frame-rate discovery.

`two-segment-black-white-h264.mp4` is a 1.8 KiB fragmented MP4 with a black
first segment and white second segment. It verifies that frame extraction picks
the correct media segment at a segment boundary.

`four-colour-interframe-h264.mp4` is a 1.7 KiB fragmented MP4 with one H.264
keyframe followed by three interframes. Its coloured frames verify that frame
extraction seeks and decodes through an interframe to the requested time.

Keep each committed MP4 fixture at or below 4 KiB. The CI fixture-budget check
enforces this limit; use generated media only when a targeted behavior cannot be
covered with the existing fixtures.

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

ffmpeg -f lavfi -i color=c=black:s=16x16:r=8:d=0.5 \
  -f lavfi -i color=c=white:s=16x16:r=8:d=0.5 \
  -filter_complex '[0:v][1:v]concat=n=2:v=1:a=0,select=not(eq(n\,2))' \
  -vsync vfr -c:v libx264 -preset ultrafast -pix_fmt yuv420p -g 1 \
  /tmp/dyndo-two-segment-source.mp4
MP4Box -dash 500 -frag 500 -rap -no-frags-default -profile dashavc264:onDemand \
  -out /tmp/dyndo-two-segment-manifest.mpd /tmp/dyndo-two-segment-source.mp4
mv /tmp/dyndo-two-segment-source_dashinit.mp4 crates/dyndo-core/tests/fixtures/two-segment-black-white-h264.mp4

ffmpeg -f lavfi -i "nullsrc=s=16x16:r=4:d=1,geq=r='if(eq(N,0),255,if(eq(N,3),255,0))':g='if(eq(N,1),255,if(eq(N,3),255,0))':b='if(eq(N,2),255,0)'" \
  -vf "select='not(eq(n,1))'" -vsync vfr \
  -c:v libx264 -preset ultrafast -pix_fmt yuv420p -g 4 -keyint_min 4 -sc_threshold 0 \
  /tmp/dyndo-interframe-source.mp4
MP4Box -dash 1000 -frag 1000 -rap -no-frags-default -profile dashavc264:onDemand \
  -out /tmp/dyndo-interframe-manifest.mpd /tmp/dyndo-interframe-source.mp4
mv /tmp/dyndo-interframe-source_dashinit.mp4 crates/dyndo-core/tests/fixtures/four-colour-interframe-h264.mp4
```
