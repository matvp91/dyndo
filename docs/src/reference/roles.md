# Track roles

A track's **role** is its author-declared purpose. It applies to audio and text
tracks only, is never probed from the media, and is stored in the descriptor's
[`role` field](./asset-json.md#track-object). This page is the exact reference
for the role vocabulary and how each role is rendered into DASH and HLS.

To set a role, see [Label tracks with roles](../how-to/label-roles.md). Roles are
validated at index time: a role on a video track, a role that doesn't match the
track's media type, or an unknown value aborts the run. A role never affects a
track's representation `id`.

## Audio roles

| Value | Meaning |
|---|---|
| `main` | The primary audio. |
| `alternate` | An alternate version of the main audio. |
| `commentary` | Commentary (e.g. director's commentary). |
| `dub` | A dubbed rendition in another language. |
| `description` | Audio description for viewers who are blind or have low vision. |
| `enhanced-audio-intelligibility` | Dialogue enhanced for intelligibility. |

## Text roles

| Value | Meaning |
|---|---|
| `subtitle` | Translation subtitles (dialogue only). The default when no role is set. |
| `caption` | SDH / closed captions (dialogue plus non-dialogue sound description). |
| `forced-subtitle` | Forced narrative subtitles (foreign dialogue or on-screen text), shown even when subtitles are otherwise off. |

## DASH mapping

Roles are emitted as descriptors on the track's `AdaptationSet`; tracks are
grouped into adaptation sets by `(sample entry, timescale, language, role)`.

### `Role`

Scheme `urn:mpeg:dash:role:2011`; `value` is the role string verbatim.

| Track | `Role` emitted |
|---|---|
| Audio with a role | `Role@value` = the role. |
| Audio with no role | none. |
| Text | `Role@value` = the role, defaulting to `subtitle` when unset. |

### `Accessibility`

Scheme `urn:tva:metadata:cs:AudioPurposeCS:2007`, emitted only for two audio
roles:

| Audio role | `Accessibility@value` |
|---|---|
| `description` | `1` |
| `enhanced-audio-intelligibility` | `8` |

No other role emits an `Accessibility` descriptor.

## HLS mapping

Roles set the selection attributes on the `EXT-X-MEDIA` rendition entries in the
multivariant playlist.

### Audio renditions

Audio tracks are grouped by sample-entry code. Within a group:

| Attribute | Rule |
|---|---|
| `DEFAULT` | `YES` on the first `main`-role rendition; if no member is `main`, on the first rendition. `NO` otherwise. |
| `AUTOSELECT` | `NO` for the opt-in roles (`commentary`, `description`, `enhanced-audio-intelligibility`) unless the rendition is the group default; `YES` otherwise. |
| `CHARACTERISTICS` | `public.accessibility.describes-video` for `description`; `public.accessibility.enhances-speech-intelligibility` for `enhanced-audio-intelligibility`; absent otherwise. |
| `NAME` | The rendition's language, qualified by its role when one is set — `nld`, `eng (commentary)`. A counter disambiguates collisions (`eng (2)`). |

`DEFAULT=YES` implies `AUTOSELECT=YES`, so the group default is always
auto-selected even when its role is opt-in.

### Subtitle renditions

> HLS subtitle renditions are not emitted yet — text tracks currently appear in
> DASH manifests only (CMAF `wvtt` sources). The mapping below is the design
> that on-the-fly text serving is being built toward.

All text tracks share one `TYPE=SUBTITLES` group.

| Attribute | Rule |
|---|---|
| `DEFAULT` | Always `NO` — the viewer enables subtitles explicitly. |
| `AUTOSELECT` | `YES` only for `forced-subtitle`; `NO` for `subtitle` and `caption`. |
| `FORCED` | `YES` only for `forced-subtitle`. |
| `CHARACTERISTICS` | `public.accessibility.transcribes-spoken-dialog,public.accessibility.describes-music-and-sound` for `caption`; absent otherwise. |

## See also

- [Label tracks with roles](../how-to/label-roles.md) — how to set them.
- [asset.json descriptor](./asset-json.md) — the `role` field in context.
- [`dyndo dash`](./cli/dash.md) and [`dyndo hls`](./cli/hls.md) — the manifests
  roles appear in.
