# Add thumbnail sprites

Use thumbnail sprites to let DASH and HLS players show a timeline preview. A
thumbnail configuration belongs to the asset, not to an individual video track:
dyndo selects a suitable source video when it builds the manifest and generates
each JPEG sprite only when a client requests it.

## Before you start

You need an `asset.json` containing at least one video track. Create one with
[Index your CMAF sources](./index-sources.md) if needed.

## Add a thumbnail configuration

Add a thumbnail track to the descriptor's `tracks` array. This configuration creates
4-by-4 sprites, each 640 pixels wide, with one frame per second:

```json
{
  "tracks": [
    /* existing tracks */,
    {
      "id": "preview",
      "type": "thumbnail",
      "tile_size": 4,
      "width": 640,
      "step": 1000
    }
  ]
}
```

`tile_size` is the number of tiles in both a sprite row and column. A value of
`4` therefore creates 16 frames per sprite. `width` is the width of the whole
composite JPEG, not one tile; it must divide evenly by `tile_size`. `step` is
the interval between frames in milliseconds. This example makes every sprite
cover 16 seconds of presentation time.

The `id` names this thumbnail track in manifest URLs. It is independent of every
video track ID, so you can add more than one configuration for the
same asset.

## Let dyndo choose the video source

Do not configure a video-track ID. dyndo uses the smallest video at least as
wide as the requested sprite; if every video is narrower, it uses the largest
one. This gives the sprite enough source pixels without always decoding the
highest rendition.

If the descriptor has no usable video source, the thumbnail configuration is
not advertised and its image routes return `404`.

## Verify the manifests

Request either manifest as usual:

```bash
curl "http://localhost:8080/out/(asset:asset)/index.mpd"
curl "http://localhost:8080/out/(asset:asset)/master.m3u8"
```

The DASH MPD gains an `image/jpeg` adaptation set with the DASH thumbnail-tile
property. The HLS multivariant playlist gains an image-stream reference like:

```text
#EXT-X-IMAGE-STREAM-INF:BANDWIDTH=...,CODECS="jpeg",RESOLUTION=160x90,URI="preview.m3u8"
```

The image media playlist points at JPEG sprites such as
`preview/0.jpg`. Both protocols reference the same generated JPEG
sprites; requesting a sprite does not write it to storage.

## Select thumbnail configurations per request

Use the manifest `filter` parameter to choose the configurations a particular
client sees. For example, this excludes all thumbnails:

```text
/out/(asset:asset)/master.m3u8?filter=type!=thumbnail
```

This keeps only thumbnail configurations whose composite width is at least 640
pixels while retaining every media track:

```text
/out/(asset:asset)/master.m3u8?filter=type!=thumbnail%7C%7Cwidth>=640
```

Filters affect manifests only. A client must use the image URLs emitted by the
manifest it requested. For the complete descriptor fields and route contract,
see the [asset.json reference](../reference/asset-json.md#thumbnail-tracks)
and [server routes reference](../reference/server/routes.md#thumbnail-sprites).
