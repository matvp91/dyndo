# Add a subtitle track

This guide shows how to add a WebVTT subtitle track to an existing asset. Two
source forms work:

- **Raw `.vtt`** — the WebVTT file itself. dyndo parses it and packages a CMAF
  `wvtt` track as it reads it, so your `.vtt` stays the single source of truth.
  Nothing is written back beside it.
- **CMAF `wvtt`** — WebVTT already packaged into ISO-BMFF by a packager. Indexed,
  probed, and served like any other CMAF track.

> Packaging happens on the way out, per request, so a `.vtt` you edit is served
> edited on the next request — no re-indexing and no repackaging step. How the
> packaged track is cut is set by
> [`segment_options.text_length` and `boundaries`](../reference/asset-json.md#segmentation).

How a track is *delivered* then depends on the protocol. DASH always references
packaged `wvtt` segments. HLS references plain WebVTT documents, because that is
the form its players handle most widely — see
[Choose how HLS delivers subtitles](#choose-how-hls-delivers-subtitles).

## Before you start

You need:

- an `asset.json` (see [Index your CMAF sources](./index-sources.md)); and
- a [WebVTT](https://www.w3.org/TR/webvtt1/) subtitle file, either raw (`.vtt`)
  or already packaged as CMAF `wvtt` (`.mp4`).

## Add a CMAF `wvtt` subtitle

Index it like any other CMAF source. It becomes a regular text track, advertised
in DASH as a text `AdaptationSet` and in HLS as a `TYPE=SUBTITLES` rendition with
its own media playlist:

```bash
dyndo index text_wvtt_nld.mp4,language=nld,role=subtitle -o asset.json
```

```text
wrote asset.json (3 tracks)
```

```json
{
  "id": "text_61af48e7-44a0-5911-9cf3-abf5d1d9c70e",
  "path": "text_wvtt_nld.mp4",
  "codec": "wvtt",
  "type": "text",
  "language": "nld",
  "role": "subtitle"
}
```

## Add a raw `.vtt` subtitle

Index the `.vtt` with a `language`:

```bash
dyndo index subtitles_nl.vtt,language=nld -o asset.json
```

```json
{
  "id": "text_5b9fbdae-2717-5f58-80ed-4f067605a5e6",
  "path": "subtitles_nl.vtt",
  "codec": "wvtt",
  "type": "text",
  "language": "nld"
}
```

A WebVTT file declares no language of its own, so set it here — without
`language=` the track's language is `und` (undetermined). The `codec` is recorded
as `wvtt` because that is what the track is packaged as; HLS unpacks it back into
a document on the way out.

By default the whole subtitle becomes one segment, cut only at the asset's splice
points. Ask for a grid instead with `--segment-text-length` (or `text_length` in
the descriptor) when you want the text timeline to resemble the media's:

```bash
dyndo dash -i asset.json -o stream.mpd --segment-text-length 4000
```

## Choose how HLS delivers subtitles

By default an HLS text rendition points at WebVTT documents — one `.vtt` per
segment, no `EXT-X-MAP`, cue timestamps absolute:

```text
#EXTINF:4.000,
text_5b9fbdae-2717-5f58-80ed-4f067605a5e6/0.vtt
```

To point it at the packaged `wvtt` segments instead, pass `wvtt` in the request's
options, or `--wvtt` when generating playlists offline:

```bash
curl "http://localhost:8080/out/(asset:asset,wvtt:!t)/master.m3u8"
dyndo hls -i asset.json -o hls --wvtt
```

Both forms describe the same segments — the same cut points, the same durations —
so this changes only how each one is delivered, and never how the asset is cut.
DASH is unaffected either way.

Reach for `wvtt` when a client specifically wants the packaged track, or when a
text track came from a CMAF `wvtt` file another packager wrote: such a track has
no document behind it, so its `.vtt` segments cannot be served and a request for
one fails.

## Set the language

The `language` value is a BCP 47 language tag. Both short tags such as `en` and
region-specific tags such as `pt-BR` are accepted; the existing three-letter
forms (`eng`, `nld`, `fra`, …) remain supported. dyndo rejects malformed tags
and copies accepted tags into manifests unchanged.

The `language` stored in `asset.json` is authoritative. To relabel a track,
either re-index it with a new `language=` override or edit the field in the JSON
directly — the manifests follow without any repackaging, and the track's `id`
does not change, because ids follow the source path rather than its metadata.

## Add subtitles in several languages

Each file becomes one track; index them together or in separate runs against the
same descriptor:

```bash
dyndo index \
  text_wvtt_nld.mp4,language=nld \
  text_wvtt_eng.mp4,language=eng \
  -o asset.json
```

Re-indexing the same path never duplicates the track — `index` matches on the
source path and updates the existing entry in place.

Two subtitle tracks may not end up with the same HLS rendition `NAME`, which is
the language plus the role's label. Same language *and* same role is a conflict;
generating the playlist fails with `duplicate rendition name`. Give them
different roles, or different languages.

## Give the subtitle a role

By default a text track is presented as a plain subtitle. To mark it as closed
captions (SDH) or a forced-narrative track, re-index it with a `role` — this
updates the entry in place and changes nothing else:

```bash
dyndo index text_wvtt_eng.mp4,role=caption -o asset.json
```

The roles intended for text are `subtitle`, `caption`, and `forced-subtitle`.
Each changes how the track is signalled in both manifests — see
[Label tracks with roles](./label-roles.md).

## Next steps

- Mark subtitles as captions or forced narrative:
  [Label tracks with roles](./label-roles.md).
- Serve the asset: [Run and configure the server](./run-the-server.md).
- The text-track fields in detail:
  [asset.json descriptor](../reference/asset-json.md#text-tracks).
