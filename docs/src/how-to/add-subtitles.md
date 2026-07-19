# Add a subtitle track

This guide shows how to add a WebVTT subtitle track to an existing asset.
Subtitles are the one track type you don't package ahead of time: you hand
[`dyndo index`](../reference/cli/index.md) the raw `.vtt` file itself, and
serving works from that source directly — chunking the raw WebVTT, or packaging
it as CMAF `wvtt`, on the fly at request time. Your `.vtt` stays the single
source of truth.

> Text-track serving is the part of dyndo currently under construction: CMAF
> `wvtt` tracks are advertised in DASH manifests today, while manifest
> advertisement for raw `.vtt` tracks and HLS subtitle renditions are still
> being wired up. Indexing works as described below either way, and descriptors
> you build now will be served as those pieces land.

## Before you start

You need:

- an `asset.json` (see [Index your CMAF sources](./index-sources.md)); and
- a [WebVTT](https://www.w3.org/TR/webvtt1/) file (`.vtt`).

## Add the subtitle

Index the `.vtt` like any other source, with a `language`:

```bash
dyndo index subtitles_nl.vtt,language=nld -o asset.json
```

```text
wrote asset.json (3 tracks)
```

Your descriptor now carries the text track alongside the others, pointing
straight at the `.vtt`:

```json
{
  "id": "text_und",
  "path": "subtitles_nl.vtt",
  "type": "text",
  "language": "nld"
}
```

The `language` value is an [ISO 639-2](https://www.loc.gov/standards/iso639-2/php/code_list.php)
three-letter code (`eng`, `nld`, `fra`, …). A WebVTT file declares no language
of its own, so set it here — if you omit it, the track's language is `und`
(undetermined).

## Add subtitles in several languages

Each `.vtt` file becomes one track; index them together or in separate runs
against the same descriptor:

```bash
dyndo index \
  subtitles_nl.vtt,language=nld \
  subtitles_en.vtt,language=eng \
  -o asset.json
```

Re-indexing the same `.vtt` path never duplicates the track — `index` updates
the existing entry in place.

## Give the subtitle a role

By default a text track is presented as a plain `subtitle`. To mark it as
closed captions (SDH) or a forced-narrative track, re-index it with a `role` —
this updates the entry in place and changes nothing else:

```bash
dyndo index subtitles_en.vtt,role=caption -o asset.json
```

Valid text roles are `subtitle`, `caption`, and `forced-subtitle`. Each changes
how the track is signalled in the generated manifests — see
[Label tracks with roles](./label-roles.md).

## Already-packaged subtitles (CMAF `wvtt`)

If a packager already gave you WebVTT in ISO-BMFF — a CMAF `wvtt` track — index
it like any other CMAF source. It is a regular text track (these are the ones
DASH manifests advertise today):

```bash
dyndo index text_wvtt_eng.mp4,language=eng -o asset.json
```

```json
{
  "id": "text_eng_wvtt_586",
  "path": "text_wvtt_eng.mp4",
  "type": "text",
  "language": "eng",
  "fourcc": "wvtt"
}
```

## Correct a subtitle's language after the fact

The `language` stored in `asset.json` is authoritative. To relabel a track,
either re-index it with a new `language=` override or edit the field in the
JSON directly — the manifests follow without any repackaging. The track's `id`
never changes with it: ids are pinned at index time so segment URLs stay
stable (see [Representation ids](../reference/asset-json.md#representation-ids)).

## Next steps

- Mark subtitles as captions or forced narrative:
  [Label tracks with roles](./label-roles.md).
- Serve the asset: [Run and configure the server](./run-the-server.md).
- The text-track fields in detail:
  [asset.json descriptor](../reference/asset-json.md#text-tracks).
