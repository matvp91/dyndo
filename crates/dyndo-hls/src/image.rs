use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;

use crate::options::HlsOptions;

const BITS_PER_PIXEL: u64 = 1;

struct ImageLayout {
    tile_size: u32,
    step: u32,
    width: u32,
    height: u32,
}

impl ImageLayout {
    fn new(track: &Track, options: &HlsOptions) -> Option<Self> {
        if options.thumbnail_tile_size == 0 || options.thumbnail_step == 0 {
            return None;
        }
        let TrackKind::Video(video) = track.kind() else {
            return None;
        };
        let width = video.width - video.width % options.thumbnail_tile_size;
        let height = video.height - video.height % options.thumbnail_tile_size;
        if width == 0 || height == 0 || track.duration() == 0 {
            return None;
        }

        Some(Self {
            tile_size: options.thumbnail_tile_size,
            step: options.thumbnail_step,
            width,
            height,
        })
    }

    fn sprite_duration(&self) -> u64 {
        u64::from(self.tile_size)
            .saturating_mul(u64::from(self.tile_size))
            .saturating_mul(u64::from(self.step))
    }

    fn tile_dimensions(&self) -> (u32, u32) {
        (self.width / self.tile_size, self.height / self.tile_size)
    }

    fn bandwidth(&self) -> u64 {
        let bits = u128::from(self.width)
            .saturating_mul(u128::from(self.height))
            .saturating_mul(u128::from(BITS_PER_PIXEL));
        let bits_per_second = bits
            .saturating_mul(1_000)
            .div_ceil(u128::from(self.sprite_duration()));

        u64::try_from(bits_per_second).unwrap_or(u64::MAX).max(1)
    }
}

pub(crate) fn stream_inf(track: &Track, options: &HlsOptions) -> Option<String> {
    let layout = ImageLayout::new(track, options)?;
    let (width, height) = layout.tile_dimensions();

    Some(format!(
        "#EXT-X-IMAGE-STREAM-INF:BANDWIDTH={},CODECS=\"jpeg\",RESOLUTION={width}x{height},URI=\"{}.m3u8\"",
        layout.bandwidth(),
        crate::image_resource_name(track),
    ))
}

pub(crate) fn build_playlist(track: &Track, options: &HlsOptions) -> Option<String> {
    let layout = ImageLayout::new(track, options)?;
    let duration = u64::from(track.duration());
    let sprite_duration = layout.sprite_duration();
    let target_duration = sprite_duration.min(duration).div_ceil(1_000);
    let (width, height) = layout.tile_dimensions();
    let mut playlist = format!(
        "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:{target_duration}\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-IMAGES-ONLY\n"
    );

    for start in (0..duration).step_by(usize::try_from(sprite_duration).unwrap_or(usize::MAX)) {
        let remaining = duration - start;
        let image_duration = remaining.min(sprite_duration);
        playlist.push_str(&format!(
            "#EXT-X-TILES:RESOLUTION={width}x{height},LAYOUT={}x{},DURATION={}\n#EXTINF:{},\n{}/{}.jpg\n",
            layout.tile_size,
            layout.tile_size,
            seconds(u64::from(layout.step)),
            seconds(image_duration),
            crate::media_resource_name(track),
            start,
        ));
    }
    playlist.push_str("#EXT-X-ENDLIST\n");

    Some(playlist)
}

fn seconds(milliseconds: u64) -> String {
    format!("{}.{:03}", milliseconds / 1_000, milliseconds % 1_000)
}
