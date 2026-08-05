# Add a subtitle track

This guide shows how to add a WebVTT subtitle track to an existing asset.
There are two source forms, and today they behave differently:

- **CMAF `wvtt`** — WebVTT packaged into ISO-BMFF by a packager. Indexed, probed,
  and served like any other CMAF track. **Use this for subtitles that must
  play.**
- **Raw `.vtt`** — the WebVTT file itself. Accepted by `index` and advertised in
  both manifests, but not yet servable: converting raw WebVTT into a `wvtt`
  track on the fly is still a stub, so the track carries no segments.

> The target design is that you hand dyndo the `.vtt` and it does the packaging
> at request time, keeping your `.vtt` the single source of truth. The indexing
> half of that works now; the conversion does not. Descriptors you build from
> `.vtt` files today will serve correctly once it lands, with no re-indexing.

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
as `wvtt` because that is what the track will be packaged as.

The entry is complete and correct, but until the conversion is implemented its
HLS media playlist is empty (`#EXT-X-TARGETDURATION:0`, no segments) and its DASH
`SegmentTimeline` has no entries, so a player will find nothing to fetch.

## Set the language

The `language` value is an [ISO 639-2](https://www.loc.gov/standards/iso639-2/php/code_list.php)
three-letter code (`eng`, `nld`, `fra`, …). dyndo stores whatever you give it and
copies it into the manifests, so use codes your target players understand.

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
