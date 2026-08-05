# Label tracks with roles

A track's **role** is its author-declared purpose — main audio, director's
commentary, audio description, closed captions, forced narrative subtitles, and
so on. dyndo records the role in the descriptor and renders it into both the
DASH and HLS manifests, so players present the track correctly: which audio a
viewer hears by default, which subtitles auto-enable, and which renditions are
flagged for accessibility.

Roles apply to **audio and text tracks only** — never video.

## Set a role while indexing

A role is never probed from the media; you declare it. Add `role=<role>` to a
track descriptor when you [index](./index-sources.md) it:

```bash
dyndo index \
  video.mp4 \
  audio_en.mp4,language=eng,role=main \
  audio_en_commentary.mp4,language=eng,role=commentary \
  -o asset.json
```

To set a role on a track that's already in the descriptor, re-index it — `index`
merges by source path, so this updates the entry in place and changes nothing
else about it:

```bash
dyndo index audio_en_commentary.mp4,role=commentary -o asset.json
```

You can also hand-edit the `role` field in `asset.json` directly; it takes the
same values.

## Audio roles

| Role | Use it for | Effect on the manifests |
|---|---|---|
| `main` | The primary audio. | Becomes the default HLS audio rendition; DASH `Role=main`. |
| `alternate` | An alternate mix of the main audio. | DASH `Role=alternate`. |
| `dub` | A dubbed rendition in another language. | DASH `Role=dub`. |
| `commentary` | Commentary, e.g. director's. | DASH `Role=commentary`. |
| `description` | Audio description for blind / low-vision viewers. | DASH `Accessibility=description`; HLS `CHARACTERISTICS=public.accessibility.describes-video`. |
| `enhanced-audio-intelligibility` | Dialogue enhanced for intelligibility. | DASH `Accessibility=enhanced-audio-intelligibility`; HLS `CHARACTERISTICS=public.accessibility.enhances-speech`. |

The **default** HLS audio rendition is the first track marked `main`; if none is,
it is the first audio track with no role at all. Mark exactly one track `main` to
control what plays by default — and note that if you give *every* audio track a
non-`main` role, no rendition is marked default.

Whether a non-default rendition is auto-selectable is decided by how
distinguishable it is, not by its role: `AUTOSELECT=YES` when a rendition is the
only one sharing its language, `FORCED` flag, and accessibility characteristics.
So a `main` + `commentary` pair in the same language leaves the commentary track
opt-in, while a lone commentary track is auto-selectable. See the
[roles reference](../reference/roles.md#autoselect) for the exact rule.

## Text (subtitle) roles

| Role | Use it for | Effect on the manifests |
|---|---|---|
| `subtitle` | Translation subtitles (dialogue only). | DASH `Role=subtitle`. This is also what a track with no role renders as. |
| `caption` | SDH / closed captions (dialogue plus non-dialogue sound). | DASH `Role=subtitle` plus `Accessibility=caption`; HLS `CHARACTERISTICS` naming the SDH characteristics. |
| `forced-subtitle` | Forced narrative (foreign dialogue, on-screen text). | DASH `Role=forced-subtitle`; HLS `FORCED=YES`. |

Text renditions are never marked `DEFAULT` in HLS — subtitles stay off until the
viewer enables them, or until the player acts on `FORCED`.

The role also names the rendition: an HLS rendition's `NAME` is its language plus
the role's label, so `caption` on an English track reads `eng (Captions)`. Two
renditions in one group cannot share a `NAME`.

## Examples

A main track plus a commentary track — the viewer hears the main audio by
default and can switch to commentary:

```bash
dyndo index \
  video.mp4 \
  audio_en.mp4,language=eng,role=main \
  audio_en_comm.mp4,language=eng,role=commentary \
  -o asset.json
```

An audio-description track for accessibility:

```bash
dyndo index audio_en_ad.mp4,language=eng,role=description -o asset.json
```

Forced subtitles that display automatically for foreign-language dialogue —
set the role on the subtitle source you [index](./add-subtitles.md):

```bash
dyndo index text_wvtt_forced_eng.mp4,language=eng,role=forced-subtitle -o asset.json
```

## What gets rejected

Only an **unrecognised role value** is rejected, and it aborts the whole run
before anything is written:

```text
error: invalid value 'audio.mp4,role=bogus' for '<INPUTS>...': unknown role: bogus
```

Everything else is accepted as given. In particular, `index` does **not** check
that a role suits the track: `audio.mp4,role=subtitle` is stored, and a `role=`
on a video input is silently discarded because video tracks have no role field.
Nothing warns you about a role that makes no sense for its track — the manifests
will simply carry it through. Use the tables above to pick deliberately.

## Next steps

- The exact DASH and HLS output for every role:
  [Track roles reference](../reference/roles.md).
- How `role` sits in the descriptor:
  [asset.json descriptor](../reference/asset-json.md).
- Where roles are set: [Index your CMAF sources](./index-sources.md).
