use std::borrow::Cow;

use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{
    Infix, alt, cut_err, delimited, dispatch, expression, fail, preceded, terminated,
};
use winnow::error::{StrContext, StrContextValue};
use winnow::prelude::*;
use winnow::token::take_while;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;

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

    fn matches(&self, track: &Track, options: &SegmentOptions) -> bool {
        self.expression.matches(track, options)
    }

    pub(super) fn narrow(
        &self,
        tracks: Vec<Track>,
        options: &SegmentOptions,
    ) -> Result<Vec<Track>, FilterMatchedNothing> {
        let tracks: Vec<_> = tracks
            .into_iter()
            .filter(|track| self.matches(track, options))
            .collect();

        if tracks.is_empty() {
            Err(FilterMatchedNothing)
        } else {
            Ok(tracks)
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("no track matches the filter")]
pub(super) struct FilterMatchedNothing;

#[derive(Debug, PartialEq, Eq)]
enum Expression {
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Comparison(Comparison),
}

impl Expression {
    fn matches(&self, track: &Track, options: &SegmentOptions) -> bool {
        match self {
            Self::And(left, right) => left.matches(track, options) && right.matches(track, options),
            Self::Or(left, right) => left.matches(track, options) || right.matches(track, options),
            Self::Comparison(comparison) => comparison.matches(track, options),
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
    fn matches(&self, track: &Track, options: &SegmentOptions) -> bool {
        match &self.value {
            Literal::Text(wanted) => self
                .attribute
                .text(track)
                .is_some_and(|actual| self.operator.holds(actual.as_ref(), wanted.as_str())),
            Literal::Number(wanted) => self
                .attribute
                .number(track, options)
                .is_some_and(|actual| self.operator.holds(&actual, wanted)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attribute {
    Type,
    Id,
    Codec,
    FrameRate,
    Language,
    Role,
    Bitrate,
    AverageBitrate,
    Duration,
    Width,
    Height,
    SampleRate,
    Channels,
}

impl Attribute {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "type" => Some(Self::Type),
            "id" => Some(Self::Id),
            "codec" => Some(Self::Codec),
            "frame_rate" => Some(Self::FrameRate),
            "language" => Some(Self::Language),
            "role" => Some(Self::Role),
            "bitrate" => Some(Self::Bitrate),
            "avg_bitrate" => Some(Self::AverageBitrate),
            "duration" => Some(Self::Duration),
            "width" => Some(Self::Width),
            "height" => Some(Self::Height),
            "sample_rate" => Some(Self::SampleRate),
            "channels" => Some(Self::Channels),
            _ => None,
        }
    }

    fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Bitrate
                | Self::AverageBitrate
                | Self::Duration
                | Self::Width
                | Self::Height
                | Self::SampleRate
                | Self::Channels
        )
    }

    fn text<'a>(self, track: &'a Track) -> Option<Cow<'a, str>> {
        match self {
            Self::Type => Some(Cow::Borrowed(track.kind().content_type())),
            Self::Id => Some(Cow::Borrowed(track.id())),
            Self::Codec => Some(Cow::Owned(track.codec().rfc6381())),
            Self::FrameRate => match track.kind() {
                TrackKind::Video(video) => Some(Cow::Borrowed(&video.frame_rate)),
                _ => None,
            },
            Self::Language => match track.kind() {
                TrackKind::Audio(audio) => Some(Cow::Borrowed(audio.language.as_str())),
                TrackKind::Text(text) => Some(Cow::Borrowed(text.language.as_str())),
                TrackKind::Video(_) => None,
            },
            Self::Role => match track.kind() {
                TrackKind::Audio(audio) => audio.role.map(|role| Cow::Borrowed(role.as_str())),
                TrackKind::Text(text) => text.role.map(|role| Cow::Borrowed(role.as_str())),
                TrackKind::Video(_) => None,
            },
            _ => None,
        }
    }

    fn number(self, track: &Track, options: &SegmentOptions) -> Option<u64> {
        match self {
            Self::Bitrate => {
                let segments = served_segments(track, options);
                Some(ServedSegment::maximum_bitrate(&segments))
            }
            Self::AverageBitrate => {
                let segments = served_segments(track, options);
                Some(ServedSegment::average_bitrate(&segments))
            }
            Self::Duration => Some(u64::from(track.duration())),
            Self::Width => match track.kind() {
                TrackKind::Video(video) => Some(u64::from(video.width)),
                _ => None,
            },
            Self::Height => match track.kind() {
                TrackKind::Video(video) => Some(u64::from(video.height)),
                _ => None,
            },
            Self::SampleRate => match track.kind() {
                TrackKind::Audio(audio) => Some(u64::from(audio.sample_rate)),
                _ => None,
            },
            Self::Channels => match track.kind() {
                TrackKind::Audio(audio) => Some(u64::from(audio.channels)),
                _ => None,
            },
            _ => None,
        }
    }
}

fn served_segments<'a>(track: &'a Track, options: &SegmentOptions) -> Vec<ServedSegment<'a>> {
    ServedSegment::group(track.segments(), options.min_length, &options.boundaries)
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
