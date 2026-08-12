# Experimental sprite generator

`experimental_sprite_generator.rs` is a proof of concept for generating a
thumbnail sprite with one FFmpeg decoder rather than one decoder per tile. It
is wired into `thumbnail.rs` while the experiment is evaluated.

## Decisions

- A thumbnail sprite has a fixed grid and a configurable `step` in milliseconds.
  `step` remains the presentation spacing advertised by DASH and HLS.
- DASH and HLS address sprites with a zero-based number. The server resolves it
  once to the sprite's source start time before starting generation.
- Thumbnail generation is keyframe-only. Requested timestamps form the cadence;
  demuxed key-packet timestamps choose the preceding iframe for each one.
  Decoded-frame PTS is deliberately not used because it can be offset by codec
  reordering. A selected iframe is scaled once and copied to every tile it
  covers when `step` is larger than the GOP interval.
- Multi-period DASH increments a thumbnail template's `startNumber` when its
  timeline is sliced, so a later Period continues to request the correct sprite.
- The server does not cache sprite responses. A CDN with strict caching owns
  that responsibility.
- Two sprite generations may run concurrently. The semaphore prevents FFmpeg
  jobs from oversubscribing CPU when several CDN misses arrive together.

## Implemented optimisations

1. One generator decodes a whole sprite instead of decoding each tile
   independently.
2. Only key packets selected by the requested cadence reach FFmpeg's decoder.
3. The scaler is reused for the full sprite and a scaled keyframe is reused
   for every tile that selects it.
4. Media is read as one contiguous OpenDAL byte range and streamed through a
   bounded two-chunk channel into FFmpeg's synchronous custom IO callback.
   Memory is therefore bounded by the init segment, two media chunks, FFmpeg
   buffers, and the final RGB sprite canvas rather than the full media window.

## Next experiments

1. Use FFmpeg's `buffer`, `scale`, and `tile` filters through rsmpeg instead
   of manually composing the RGB canvas.
2. Build a CMAF sync-sample index to read only selected keyframe samples. This
   is the next substantial CPU and storage-IO optimisation; selecting packets
   in the decoder still requires FFmpeg to demux the full selected window.
