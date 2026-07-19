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

| Role | Use it for | Effect on players |
|---|---|---|
| `main` | The primary audio. | Becomes the default audio rendition. |
| `alternate` | An alternate mix of the main audio. | Auto-selectable (e.g. to match the viewer's language). |
| `dub` | A dubbed rendition in another language. | Auto-selectable (e.g. to match the viewer's language). |
| `commentary` | Commentary, e.g. director's. | Opt-in: never auto-selected, the viewer chooses it. |
| `description` | Audio description for blind / low-vision viewers. | Opt-in, and flagged as accessible audio description. |
| `enhanced-audio-intelligibility` | Dialogue enhanced for intelligibility. | Opt-in, and flagged as an accessibility rendition. |

The **default** audio rendition is the first track you mark `main`; if you mark
none, it's the first audio track. Mark exactly one track `main` to control what
plays by default.

## Text (subtitle) roles

> Text-track manifest output is still being completed — today a text role
> shows up in DASH (for CMAF `wvtt` sources); HLS subtitle renditions land
> next. See [Track roles](../reference/roles.md) for the exact per-protocol
> state.

| Role | Use it for | Effect on players |
|---|---|---|
| `subtitle` | Translation subtitles (dialogue only). | Off by default; the viewer turns them on. This is also the default when no role is set. |
| `caption` | SDH / closed captions (dialogue plus non-dialogue sound). | Off by default; flagged with the SDH accessibility characteristics. |
| `forced-subtitle` | Forced narrative (foreign dialogue, on-screen text). | Auto-enabled and marked `FORCED`, so it shows even when subtitles are otherwise off. |

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
set the role directly on the `.vtt` you [index](./add-subtitles.md):

```bash
dyndo index forced_en.vtt,language=eng,role=forced-subtitle -o asset.json
```

## What gets rejected

`index` validates roles as it reads each descriptor and aborts the whole run on:

- a `role` (or `language`) on a **video** input;
- a `role` that doesn't apply to the track's media type — e.g. `subtitle` on
  audio, or `main` on text; or
- an unknown role value.

## Next steps

- The exact DASH and HLS output for every role:
  [Track roles reference](../reference/roles.md).
- How `role` sits in the descriptor:
  [asset.json descriptor](../reference/asset-json.md).
- Where roles are set: [Index your CMAF sources](./index-sources.md).
