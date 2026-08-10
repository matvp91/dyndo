# Extract a video frame

Use `dyndo image` to save a full-resolution JPEG from the first video track in an asset descriptor.

## Extract the frame

Pass the descriptor, a presentation time in milliseconds, and an output path:

```bash
dyndo image --input assets/asset.json --time 5000 --output frame.jpg
```

```text
wrote frame.jpg
```

The command resolves the video source through the descriptor, reads only the initialization data and media segment needed for that time, decodes the displayed frame, and writes it at the video's declared dimensions.

## Use a different storage root

CLI paths are handled by an OpenDAL filesystem rooted at the current directory. Set `OPENDAL_FS_ROOT` when the descriptor, source, or output should resolve from another root:

```bash
OPENDAL_FS_ROOT=/srv/media dyndo image \
  --input assets/asset.json \
  --time 5000 \
  --output previews/frame.jpg
```

The output directory must already exist.

## Troubleshooting

- `asset has no video track` means the descriptor contains only audio or text tracks.
- A time outside the video track is rejected; times are milliseconds from the presentation timeline.
- Decode and JPEG errors usually indicate unsupported or malformed source media or an FFmpeg build without the required decoder or Motion JPEG encoder.

For the exact command contract, see [`dyndo image`](../reference/cli/image.md).
