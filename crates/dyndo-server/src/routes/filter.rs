use std::borrow::Cow;

use dyndo_core::asset::ResolvedAsset;
use dyndo_core::track::ResolvedTrack;
use serde::{Deserialize, Deserializer, de};
use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{
    Infix, alt, cut_err, delimited, dispatch, expression, fail, preceded, terminated,
};
use winnow::error::{StrContext, StrContextValue};
use winnow::prelude::*;
use winnow::token::take_while;

#[derive(Debug, thiserror::Error)]
#[error("at offset {offset}: {message}")]
pub(super) struct FilterParseError {
    pub offset: usize,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Filter {
    expression: Expression,
}

impl<'de> Deserialize<'de> for Filter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(de::Error::custom)
    }
}

impl Filter {
    pub(super) fn parse(input: &str) -> Result<Self, FilterParseError> {
        terminated(parse_expression, multispace0)
            .parse(input)
            .map(|expression| Self { expression })
            .map_err(|error| FilterParseError {
                offset: error.offset(),
                message: error.inner().to_string(),
            })
    }

    pub(super) fn apply(&self, asset: &mut ResolvedAsset) -> Result<(), FilterMatchedNothing> {
        asset.retain_tracks(|track| self.matches(track));

        if asset.tracks().is_empty() {
            Err(FilterMatchedNothing)
        } else {
            Ok(())
        }
    }

    pub(super) fn matches(&self, track: &ResolvedTrack) -> bool {
        self.expression.matches(track)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("no asset track matches the filter")]
pub(super) struct FilterMatchedNothing;

#[derive(Debug, PartialEq, Eq)]
enum Expression {
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Comparison(Comparison),
}

impl Expression {
    fn matches(&self, track: &ResolvedTrack) -> bool {
        match self {
            Self::And(left, right) => left.matches(track) && right.matches(track),
            Self::Or(left, right) => left.matches(track) || right.matches(track),
            Self::Comparison(comparison) => comparison.matches(track),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Comparison {
    attribute: Attribute,
    operator: Operator,
    value: Literal,
}

impl Comparison {
    fn matches(&self, track: &ResolvedTrack) -> bool {
        match &self.value {
            Literal::Text(wanted) => self
                .attribute
                .text(track)
                .is_some_and(|actual| self.operator.holds(actual.as_ref(), wanted.as_str())),
            Literal::Number(wanted) => self
                .attribute
                .number(track)
                .is_some_and(|actual| self.operator.holds(&actual, wanted)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attribute {
    Type,
    Format,
    Id,
    Codec,
    FrameRate,
    Language,
    Role,
    Width,
    Height,
    SampleRate,
    Channels,
    TileSize,
}

impl Attribute {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "type" => Some(Self::Type),
            "format" => Some(Self::Format),
            "id" => Some(Self::Id),
            "codec" => Some(Self::Codec),
            "frame_rate" => Some(Self::FrameRate),
            "language" => Some(Self::Language),
            "role" => Some(Self::Role),
            "width" => Some(Self::Width),
            "height" => Some(Self::Height),
            "sample_rate" => Some(Self::SampleRate),
            "channels" => Some(Self::Channels),
            "tile_size" => Some(Self::TileSize),
            _ => None,
        }
    }

    fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Width
                | Self::Height
                | Self::SampleRate
                | Self::Channels
                | Self::TileSize
        )
    }

    fn text(self, track: &ResolvedTrack) -> Option<Cow<'_, str>> {
        match self {
            Self::Type => Some(Cow::Borrowed(track.track_type().as_str())),
            Self::Format => Some(Cow::Borrowed(track.format().as_str())),
            Self::Id => Some(Cow::Borrowed(track.id())),
            Self::Codec => track.codec().map(Cow::Owned),
            Self::FrameRate => track
                .video_metadata()
                .map(|kind| Cow::Borrowed(kind.frame_rate.as_str())),
            Self::Language => track
                .language()
                .map(|language| Cow::Borrowed(language.as_str())),
            Self::Role => track.role().map(|role| Cow::Borrowed(role.as_str())),
            Self::Width
            | Self::Height
            | Self::SampleRate
            | Self::Channels
            | Self::TileSize => None,
        }
    }

    fn number(self, track: &ResolvedTrack) -> Option<u64> {
        let thumbnail = track.thumbnail();
        match self {
            Self::Width => track
                .video_metadata()
                .map(|kind| u64::from(kind.width))
                .or_else(|| thumbnail.map(|track| u64::from(track.width()))),
            Self::Height => track
                .video_metadata()
                .map(|kind| u64::from(kind.height))
                .or_else(|| thumbnail.map(|track| u64::from(track.height()))),
            Self::SampleRate => track
                .audio_metadata()
                .map(|kind| u64::from(kind.sample_rate)),
            Self::Channels => track.audio_metadata().map(|kind| u64::from(kind.channels)),
            Self::TileSize => thumbnail.map(|track| u64::from(track.tile_size())),
            Self::Type
            | Self::Format
            | Self::Id
            | Self::Codec
            | Self::FrameRate
            | Self::Language
            | Self::Role => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Operator {
    fn is_ordering(self) -> bool {
        !matches!(self, Self::Equal | Self::NotEqual)
    }

    fn holds<T: Ord + ?Sized>(self, actual: &T, wanted: &T) -> bool {
        let ordering = actual.cmp(wanted);
        match self {
            Self::Equal => ordering.is_eq(),
            Self::NotEqual => !ordering.is_eq(),
            Self::Less => ordering.is_lt(),
            Self::LessOrEqual => ordering.is_le(),
            Self::Greater => ordering.is_gt(),
            Self::GreaterOrEqual => ordering.is_ge(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Literal {
    Text(String),
    Number(u64),
}

fn parse_expression(input: &mut &str) -> ModalResult<Expression> {
    expression(parse_operand)
        .infix(
            dispatch! {delimited(multispace0, alt(("&&", "||")), multispace0);
                "&&" => Infix::Left(2, |_: &mut &str, left, right| {
                    Ok(Expression::And(Box::new(left), Box::new(right)))
                }),
                "||" => Infix::Left(1, |_: &mut &str, left, right| {
                    Ok(Expression::Or(Box::new(left), Box::new(right)))
                }),
                _ => fail,
            },
        )
        .parse_next(input)
}

fn parse_operand(input: &mut &str) -> ModalResult<Expression> {
    preceded(
        multispace0,
        alt((
            delimited('(', parse_expression, preceded(multispace0, ')')),
            parse_comparison,
        )),
    )
    .parse_next(input)
}

fn parse_comparison(input: &mut &str) -> ModalResult<Expression> {
    let attribute = cut_err(
        take_while(1.., ('a'..='z', '_'))
            .verify_map(Attribute::parse)
            .context(StrContext::Label("track attribute")),
    )
    .parse_next(input)?;
    let operator = delimited(multispace0, parse_operator, multispace0).parse_next(input)?;
    if operator.is_ordering() && !attribute.is_numeric() {
        return cut_err(
            fail.context(StrContext::Expected(StrContextValue::Description(
                "a numeric attribute before an ordering operator",
            ))),
        )
        .parse_next(input);
    }
    let value = parse_literal(attribute.is_numeric(), input)?;

    Ok(Expression::Comparison(Comparison {
        attribute,
        operator,
        value,
    }))
}

fn parse_operator(input: &mut &str) -> ModalResult<Operator> {
    alt((
        "==".value(Operator::Equal),
        "!=".value(Operator::NotEqual),
        "<=".value(Operator::LessOrEqual),
        ">=".value(Operator::GreaterOrEqual),
        "<".value(Operator::Less),
        ">".value(Operator::Greater),
    ))
    .context(StrContext::Label("comparison operator"))
    .parse_next(input)
}

fn parse_literal(numeric: bool, input: &mut &str) -> ModalResult<Literal> {
    if numeric {
        cut_err(digit1.parse_to().context(StrContext::Label("whole number")))
            .map(Literal::Number)
            .parse_next(input)
    } else {
        cut_err(
            take_while(1.., |character: char| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '/')
            })
            .context(StrContext::Label("value")),
        )
        .map(|value: &str| Literal::Text(value.to_string()))
        .parse_next(input)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dyndo_core::asset::ResolvedAsset;
    use dyndo_core::codec::{CodecConfig, WvttCodec};
    use dyndo_core::track::ResolvedTrack;
    use dyndo_core::track::cmaf::{CmafKind, InitSegment, ResolvedCmafTrack};
    use dyndo_core::track::metadata::{AudioMetadata, TextMetadata, VideoMetadata};
    use dyndo_core::track::thumbnail::ThumbnailTrack;
    use dyndo_core::track::timed_text::ResolvedTimedTextTrack;

    use super::Filter;

    fn cmaf(id: &str, kind: CmafKind) -> ResolvedCmafTrack {
        ResolvedCmafTrack::new(
            id.to_string(),
            format!("{id}.mp4").into(),
            kind,
            Arc::new(InitSegment::new(CodecConfig::Wvtt(WvttCodec), 1_000, 0, 0)),
            Vec::new(),
        )
    }

    fn asset() -> ResolvedAsset {
        let video = cmaf(
            "video",
            CmafKind::Video(VideoMetadata {
                width: 1_920,
                height: 1_080,
                frame_rate: "25/1".to_string(),
            }),
        );
        let audio = cmaf(
            "audio",
            CmafKind::Audio(AudioMetadata {
                sample_rate: 48_000,
                channels: 2,
                language: "eng".parse().unwrap(),
                role: None,
            }),
        );
        let cmaf_text = cmaf("cmaf-text", CmafKind::Text(TextMetadata::default()));
        let web_vtt = ResolvedTimedTextTrack::from_web_vtt_text(
            "webvtt".to_string(),
            "webvtt.vtt".into(),
            TextMetadata::default(),
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nText\n",
        )
        .unwrap();
        let thumbnail = ThumbnailTrack::new("preview".to_string(), 4, 640)
            .resolve([&video])
            .unwrap();
        ResolvedAsset::new(
            Vec::new(),
            vec![
                ResolvedTrack::Cmaf(video),
                ResolvedTrack::Cmaf(audio),
                ResolvedTrack::Cmaf(cmaf_text),
                ResolvedTrack::TimedText(web_vtt),
                ResolvedTrack::Thumbnail(thumbnail),
            ],
        )
    }

    #[test]
    fn apply_keeps_matching_thumbnail_tracks() {
        let mut asset = asset();

        Filter::parse("type==thumbnail&&width>=640")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        assert_eq!(asset.thumbnails().count(), 1);
    }

    #[test]
    fn apply_uses_resolved_track_fields() {
        let mut asset = asset();

        Filter::parse("codec==wvtt&&type==audio")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        assert_eq!(asset.tracks()[0].id(), "audio");
    }

    #[test]
    fn apply_keeps_cmaf_and_webvtt_text_tracks() {
        let mut asset = asset();

        Filter::parse("type==text")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        let ids: Vec<_> = asset.tracks().iter().map(ResolvedTrack::id).collect();

        assert_eq!(ids, ["cmaf-text", "webvtt"]);
    }

    #[test]
    fn apply_excludes_cmaf_and_webvtt_text_tracks_by_type() {
        let mut asset = asset();

        Filter::parse("type!=text")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        assert!(
            asset
                .tracks()
                .iter()
                .all(|track| track.track_type().as_str() != "text")
        );
    }

    #[test]
    fn apply_filters_raw_webvtt_tracks_by_format() {
        let mut asset = asset();

        Filter::parse("format==webvtt")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        let ids: Vec<_> = asset.tracks().iter().map(ResolvedTrack::id).collect();

        assert_eq!(ids, ["webvtt"]);
    }

    #[test]
    fn parse_rejects_segment_derived_attributes() {
        let result = Filter::parse("bitrate>=800000");

        assert!(result.is_err());
    }
}
