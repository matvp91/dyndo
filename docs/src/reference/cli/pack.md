# dyndo pack

Pack a source subtitle file into a CMAF text track aligned to the first video
track of an asset, write it beside the descriptor, and add it to the asset.

## Synopsis

```text
dyndo pack [OPTIONS] --input <INPUT>
```

## Options

| Option | Description | Default |
|---|---|---|
| `-i, --input <INPUT>` | Input source subtitle file. The extension selects the packer (currently `.vtt` → `wvtt`). | *(required)* |
| `-a, --asset <ASSET>` | Asset descriptor to align to and update. | `asset.json` |
| `-l, --language <LANGUAGE>` | ISO 639-2 language code stored in the track. | `und` |
| `-h, --help` | Print help. | |

## Description

Unlike video and audio, subtitles are supplied as source text, so `pack` both
converts and registers them. Given a `.vtt` input it:

1. reads the first video track's segment timeline from `--asset` (the subtitle
   is segmented to align with it);
2. parses the WebVTT, expands its cues across that timeline, and packs the
   result into a CMAF `wvtt` file written **beside the descriptor** as
   `text_wvtt_<language>.mp4`; and
3. adds the new text track to the descriptor at `--asset`, then rewrites it.

```text
wrote text_wvtt_eng.mp4; updated asset.json
```

Both the output filename and the track `id` are `text_wvtt_<language>`, so the
name is fully determined by `--language`.

## Language handling

- `--language` takes an [ISO 639-2](https://www.loc.gov/standards/iso639-2/php/code_list.php)
  three-letter code.
- An empty value normalizes to `und` (the file is written as
  `text_wvtt_und.mp4`, never `text_wvtt_.mp4`).
- The value stored in the descriptor is authoritative: editing a text track's
  `language` in `asset.json` overrides what the file itself declares. See the
  [descriptor reference](../asset-json.md#text-tracks).

## Requirements

- `--asset` must reference an existing descriptor that contains **at least one
  video track** — the video timeline is what the subtitle is aligned to.
  Packing against an asset with no video track fails.
- The input extension must be supported. Anything other than `.vtt` aborts with
  an "unsupported input extension" error.

## Idempotency

Because the output name and `id` derive from the language, re-packing the same
language replaces the existing track cleanly: the stale same-`id` entry is
removed from the descriptor before the new one is added, and the `.mp4` is
overwritten. Packing a different language adds a separate track.

## Examples

Add English subtitles to an asset:

```bash
dyndo pack -i subtitles_en.vtt -a asset.json -l eng
```

Add a second language:

```bash
dyndo pack -i subtitles_nl.vtt -a asset.json -l nld
```

## See also

- [Add a subtitle track](../../how-to/add-subtitles.md) — task-oriented guide.
- [asset.json descriptor: text tracks](../asset-json.md#text-tracks).
