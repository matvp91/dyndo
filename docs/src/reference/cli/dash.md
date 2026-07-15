# dyndo dash

Generate a DASH MPD from an `asset.json`.

## Synopsis

```text
dyndo dash [OPTIONS]
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input `asset.json` path. | `asset.json` |
| `-o, --output <OUTPUT>` | Output manifest path. | `stream.mpd` |
| `-c, --compact` | Hoist a `SegmentTemplate` shared by all representations up to the `AdaptationSet` level. | off |
| `-h, --help` | Print help. | |

## Description

`dash` reads the descriptor at `--input`, parses each track's CMAF header to
recover its segment index, and writes a static MPD to `--output`:

```text
wrote stream.mpd
```

The manifest is `type="static"` (video on demand). Each track becomes a
`Representation` inside an `AdaptationSet` for its content type (`video`,
`audio`, or `text`), carrying a `SegmentTemplate` with a `SegmentTimeline`
derived from the source's `sidx`. Segment URLs use the `$RepresentationID$` and
`$Time$` template variables — `<id>/init.mp4` and `<id>/<time>.m4s` — matching
the [server's segment routes](../server/routes.md).

Text tracks are emitted with a `subtitle` `Role`; audio and text `AdaptationSet`s
carry a `lang` attribute.

## The `--compact` flag

Without `--compact`, every `Representation` carries its own `SegmentTemplate`.
With it, a `SegmentTemplate` common to all representations in an `AdaptationSet`
is written once at the set level, ahead of the representations. The rendered
timeline is identical; only the structure and size differ.

The server always renders DASH in the compact form, so `--compact` makes the
CLI output match what the server serves.

## Examples

```bash
dyndo dash -i asset.json -o stream.mpd
dyndo dash -i asset.json -o stream.mpd --compact
```

## See also

- [Generate manifests without the server](../../how-to/offline-manifests.md).
- [`dyndo hls`](./hls.md) — the HLS equivalent.
