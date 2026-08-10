# dyndo image

Extract one full-resolution JPEG frame from the first video track in an asset descriptor.

## Synopsis

```text
dyndo image --input <INPUT> --time <TIME> --output <OUTPUT>
```

## Options

| Option | Description |
|---|---|
| `-i, --input <INPUT>` | Asset descriptor path. Required. |
| `-t, --time <TIME>` | Frame time in milliseconds. Required. |
| `-o, --output <OUTPUT>` | Output JPEG path. Required. |
| `-h, --help` | Print help. |

## Behavior

The command reads the descriptor, selects its first video track, probes that track from storage, and decodes the displayed frame at the requested millisecond. The JPEG keeps the video's declared width and height.

All paths use the CLI's [OpenDAL filesystem root](../cli.md#storage-root). The video source path is resolved relative to the descriptor. The output path is resolved from the storage root.

The command fails when the descriptor has no video track, the requested time falls outside that track, the source cannot be decoded, or the JPEG cannot be written. On success it prints:

```text
wrote frame.jpg
```

## Example

```bash
dyndo image --input assets/asset.json --time 1250 --output frame.jpg
```

## See also

- [Extract a video frame](../../how-to/extract-image.md) — task-oriented instructions.
- [asset.json descriptor](../asset-json.md) — the input format.
