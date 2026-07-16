# Add a subtitle track

This guide shows how to add a WebVTT subtitle track to an existing asset with
`dyndo pack`. Unlike video and audio — which you supply as ready-made CMAF —
subtitles start life as a `.vtt` text file, so `pack` does two jobs: it converts
the subtitle into a CMAF `wvtt` track **and** adds it to your descriptor.

## Before you start

You need:

- an `asset.json` that already contains **at least one video track** (see
  [Index your CMAF sources](./index-sources.md)); and
- a [WebVTT](https://www.w3.org/TR/webvtt1/) file (`.vtt`).

`pack` aligns the subtitle to the first video track's segment timeline, so the
asset must have a video track to align to. Packing against an audio-only asset
fails.

## Pack the subtitle

```bash
dyndo pack -i subtitles_en.vtt -a asset.json -l eng
```

```text
wrote text_wvtt_eng.mp4; updated asset.json
```

This does three things:

1. reads the first video track's segment timeline from `asset.json`;
2. segments the subtitle to match that timeline and writes it as a CMAF `wvtt`
   file, `text_wvtt_<language>.mp4`, **beside the descriptor**; and
3. adds the new text track to `asset.json`.

The `-l/--language` value is an [ISO 639-2](https://www.loc.gov/standards/iso639-2/php/code_list.php)
three-letter code (`eng`, `nld`, `fra`, …). It becomes both the track's language
and part of its `id` and filename. If you omit it, the language defaults to
`und` (undetermined).

Your descriptor now carries the text track alongside the others:

```json
{
  "type": "text",
  "id": "text_wvtt_eng",
  "path": "text_wvtt_eng.mp4",
  "fourcc": "wvtt",
  "timescale": 1000,
  "language": "eng"
}
```

Manifests generated from this asset will advertise the subtitle: a `subtitle`
`Role` in the DASH `AdaptationSet`, and an `EXT-X-MEDIA:TYPE=SUBTITLES` entry in
the HLS multivariant playlist.

## Add subtitles in several languages

Run `pack` once per language, each against the same descriptor:

```bash
dyndo pack -i subtitles_en.vtt -a asset.json -l eng
dyndo pack -i subtitles_nl.vtt -a asset.json -l nld
```

Because the output filename and `id` are derived from the language,
`pack`-ing the same language again overwrites the previous track cleanly instead
of creating a duplicate.

## Give the subtitle a role

`pack` creates the track with a language but **no role**, so by default it is
advertised as a plain `subtitle`. To mark it as closed captions (SDH) or a
forced-narrative track, re-index the packed file with a `role`. Because `index`
merges by source path, this updates the existing track in place:

```bash
dyndo pack -i captions_en.vtt -a asset.json -l eng   # wrote text_wvtt_eng.mp4
dyndo index text_wvtt_eng.mp4,role=caption -o asset.json
```

Valid text roles are `subtitle`, `caption`, and `forced-subtitle`. Each changes
how the track is signalled in both manifests — see
[Label tracks with roles](./label-roles.md).

## Correct a subtitle's language after the fact

The language stored in `asset.json` wins over whatever the file itself declares.
To relabel a track, edit its `language` field in the descriptor — the manifests
will follow your edit without re-packing. (Emptying the field falls back to the
language recorded inside the file.) This override is described in the
[descriptor reference](../reference/asset-json.md#text-tracks).

## Next steps

- Mark subtitles as captions or forced narrative:
  [Label tracks with roles](./label-roles.md).
- Serve the asset (subtitles included):
  [Run and configure the server](./run-the-server.md).
- See exactly what `pack` accepts:
  [`dyndo pack` reference](../reference/cli/pack.md).
