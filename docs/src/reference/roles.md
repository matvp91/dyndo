# Track roles

A track's **role** is its author-declared purpose. It is never discovered from the
media, and is stored in the descriptor's
[`role` field](./asset-json.md#track-object). This page is the exact reference
for the role vocabulary and how each role is rendered into DASH and HLS.

To set a role, see [Label tracks with roles](../how-to/label-roles.md).

## The vocabulary

There is one flat set of nine role values. Any of them may be stored on any
audio or text track — `index` rejects only values outside this list, and does
not check a role against the track's type. The grouping below is by intended
use, not by enforcement.

Video tracks have no `role` field; a `role=` override on a video input is
accepted and silently discarded.

### Intended for audio

| Value | Meaning |
|---|---|
| `main` | The primary audio. |
| `alternate` | An alternate version of the main audio. |
| `commentary` | Commentary (e.g. director's commentary). |
| `dub` | A dubbed rendition in another language. |
| `description` | Audio description for viewers who are blind or have low vision. |
| `enhanced-audio-intelligibility` | Dialogue enhanced for intelligibility. |

### Intended for text

| Value | Meaning |
|---|---|
| `subtitle` | Translation subtitles (dialogue only). |
| `caption` | SDH / closed captions (dialogue plus non-dialogue sound description). |
| `forced-subtitle` | Forced narrative subtitles (foreign dialogue or on-screen text). |

## DASH mapping

Roles are emitted as descriptors on the track's `AdaptationSet`. Both
descriptor kinds use the **same** scheme, `urn:mpeg:dash:role:2011`, and both
carry the role name as a string `value`. Tracks are grouped into adaptation sets partly by role, so a set's members always agree on it.

### Audio

| Audio role | `Role@value` | `Accessibility@value` |
|---|---|---|
| *(none)* | — | — |
| `main` | `main` | — |
| `alternate` | `alternate` | — |
| `commentary` | `commentary` | — |
| `dub` | `dub` | — |
| `description` | — | `description` |
| `enhanced-audio-intelligibility` | — | `enhanced-audio-intelligibility` |

The two accessibility roles emit an `Accessibility` descriptor **instead of** a
`Role`, not in addition to one.

### Text

| Text role | `Role@value` | `Accessibility@value` |
|---|---|---|
| *(none)* | `subtitle` | — |
| `subtitle` | `subtitle` | — |
| `caption` | `subtitle` | `caption` |
| `forced-subtitle` | `forced-subtitle` | — |

A text `AdaptationSet` always carries a `Role`; `caption` is signalled as a
subtitle role plus an accessibility descriptor.

```xml
<AdaptationSet id="2" contentType="text" lang="eng" mimeType="application/mp4" …>
  <Accessibility schemeIdUri="urn:mpeg:dash:role:2011" value="caption"/>
  <Role schemeIdUri="urn:mpeg:dash:role:2011" value="subtitle"/>
  …
</AdaptationSet>
```

## HLS mapping

Roles set the attributes on the `EXT-X-MEDIA` rendition entries in the
multivariant playlist. Audio renditions all share `GROUP-ID="audio"` and text
renditions all share `GROUP-ID="subtitles"`; the role does not affect group
membership.

Attributes whose value is `NO` are omitted from the output rather than written
out, which is equivalent.

### `NAME`

The rendition's language, qualified by a human-readable label when a role is
set:

| Role | Label | Example `NAME` |
|---|---|---|
| *(none)* | — | `nld` |
| `main` | Main | `nld (Main)` |
| `alternate` | Alternate | `nld (Alternate)` |
| `commentary` | Commentary | `eng (Commentary)` |
| `dub` | Dub | `fra (Dub)` |
| `description` | Audio Description | `eng (Audio Description)` |
| `enhanced-audio-intelligibility` | Enhanced Dialogue | `eng (Enhanced Dialogue)` |
| `subtitle` | Subtitles | `nld (Subtitles)` |
| `caption` | Captions | `eng (Captions)` |
| `forced-subtitle` | Forced Subtitles | `eng (Forced Subtitles)` |

Two renditions in the same group may not produce the same `NAME`; generating the
playlist fails with `duplicate rendition name: <name>` if they do.

### `DEFAULT`

`DEFAULT=YES` goes to exactly one **audio** rendition, chosen in this order:

1. the first audio track whose role is `main`;
2. otherwise, the first audio track with **no** role.

If every audio track carries a non-`main` role, no rendition is marked default.
Text renditions are never marked default.

### `AUTOSELECT`

`AUTOSELECT=YES` when either:

- the rendition is the group default (above); or
- it is the **only** rendition sharing its combination of media type, language,
  `FORCED` flag, and `CHARACTERISTICS`.

So a lone Dutch subtitle track is auto-selectable, while a Dutch subtitle track
sitting alongside a Dutch forced-subtitle track is still auto-selectable
(different `FORCED` flags), and two plain Dutch subtitle tracks are neither.

### `FORCED`

`FORCED=YES` only for `forced-subtitle`.

### `CHARACTERISTICS`

| Role | `CHARACTERISTICS` |
|---|---|
| `description` | `public.accessibility.describes-video` |
| `enhanced-audio-intelligibility` | `public.accessibility.enhances-speech` |
| `caption` | `public.accessibility.transcribes-spoken-dialog,public.accessibility.describes-music-and-sound` |
| all others | *(absent)* |

## See also

- [Label tracks with roles](../how-to/label-roles.md) — how to set them.
- [asset.json descriptor](./asset-json.md) — the `role` field in context.
- [Server routes](./server/routes.md) — the DASH and HLS outputs in which roles appear.
