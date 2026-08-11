use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::track_descriptor::TrackDescriptor;
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

    pub(super) fn apply(
        &self,
        descriptor: &mut AssetDescriptor,
    ) -> Result<(), FilterMatchedNothing> {
        descriptor
            .tracks
            .retain(|track| self.expression.matches(track));

        if descriptor.tracks.is_empty() {
            Err(FilterMatchedNothing)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("no asset descriptor entry matches the filter")]
pub(super) struct FilterMatchedNothing;

#[derive(Debug, PartialEq, Eq)]
enum Expression {
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Comparison(Comparison),
}

impl Expression {
    fn matches(&self, descriptor: &TrackDescriptor) -> bool {
        match self {
            Self::And(left, right) => left.matches(descriptor) && right.matches(descriptor),
            Self::Or(left, right) => left.matches(descriptor) || right.matches(descriptor),
            Self::Comparison(comparison) => comparison.matches(descriptor),
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
    fn matches(&self, descriptor: &TrackDescriptor) -> bool {
        match &self.value {
            Literal::Text(wanted) => self
                .attribute
                .text(descriptor)
                .is_some_and(|actual| self.operator.holds(actual.as_ref(), wanted.as_str())),
            Literal::Number(wanted) => self
                .attribute
                .number(descriptor)
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
    Width,
    Height,
    SampleRate,
    Channels,
    TileSize,
    Step,
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
            "width" => Some(Self::Width),
            "height" => Some(Self::Height),
            "sample_rate" => Some(Self::SampleRate),
            "channels" => Some(Self::Channels),
            "tile_size" => Some(Self::TileSize),
            "step" => Some(Self::Step),
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
                | Self::Step
        )
    }

    fn text(self, descriptor: &TrackDescriptor) -> Option<&str> {
        match (self, descriptor) {
            (Self::Type, track) => Some(track.content_type()),
            (Self::Id, track) => Some(track.id()),
            (Self::Codec, TrackDescriptor::Video(track)) => Some(&track.codec),
            (Self::Codec, TrackDescriptor::Audio(track)) => Some(&track.codec),
            (Self::Codec, TrackDescriptor::Text(track)) => Some(&track.codec),
            (Self::FrameRate, TrackDescriptor::Video(track)) => Some(&track.kind.frame_rate),
            (Self::Language, TrackDescriptor::Audio(track)) => Some(track.kind.language.as_str()),
            (Self::Language, TrackDescriptor::Text(track)) => Some(track.kind.language.as_str()),
            (Self::Language, TrackDescriptor::Vtt(track)) => Some(track.kind.language.as_str()),
            (Self::Role, TrackDescriptor::Audio(track)) => {
                track.kind.role.as_ref().map(|role| role.as_str())
            }
            (Self::Role, TrackDescriptor::Text(track)) => {
                track.kind.role.as_ref().map(|role| role.as_str())
            }
            (Self::Role, TrackDescriptor::Vtt(track)) => {
                track.kind.role.as_ref().map(|role| role.as_str())
            }
            _ => None,
        }
    }

    fn number(self, descriptor: &TrackDescriptor) -> Option<u64> {
        match (self, descriptor) {
            (Self::Width, TrackDescriptor::Video(track)) => Some(u64::from(track.kind.width)),
            (Self::Width, TrackDescriptor::Thumbnail(track)) => Some(u64::from(track.width)),
            (Self::Height, TrackDescriptor::Video(track)) => Some(u64::from(track.kind.height)),
            (Self::SampleRate, TrackDescriptor::Audio(track)) => {
                Some(u64::from(track.kind.sample_rate))
            }
            (Self::Channels, TrackDescriptor::Audio(track)) => Some(u64::from(track.kind.channels)),
            (Self::TileSize, TrackDescriptor::Thumbnail(track)) => Some(u64::from(track.tile_size)),
            (Self::Step, TrackDescriptor::Thumbnail(track)) => Some(u64::from(track.step)),
            _ => None,
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
    use dyndo_core::asset_descriptor::AssetDescriptor;

    use super::Filter;

    fn asset() -> AssetDescriptor {
        serde_json::from_str(
            r#"
            {
              "tracks": [
                {
                  "id": "video",
                  "path": "video.mp4",
                  "codec": "avc1.640028",
                  "type": "video",
                  "width": 1920,
                  "height": 1080,
                  "frame_rate": "25/1"
                },
                {
                  "id": "audio",
                  "path": "audio.mp4",
                  "codec": "mp4a.40.2",
                  "type": "audio",
                  "sample_rate": 48000,
                  "channels": 2,
                  "language": "eng"
                },
                {
                  "id": "preview",
                  "tile_size": 4,
                  "width": 640,
                  "step": 1000,
                  "type": "thumbnail"
                }
              ]
            }
            "#,
        )
        .unwrap()
    }

    #[test]
    fn apply_keeps_matching_thumbnail_descriptors() {
        let mut asset = asset();

        Filter::parse("type==thumbnail&&width>=640")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        assert_eq!(asset.thumbnail_tracks().count(), 1);
    }

    #[test]
    fn apply_uses_track_descriptor_fields_without_probing() {
        let mut asset = asset();

        Filter::parse("codec==mp4a.40.2")
            .unwrap()
            .apply(&mut asset)
            .unwrap();

        assert_eq!(asset.tracks[0].id(), "audio");
    }

    #[test]
    fn apply_returns_an_error_when_no_descriptor_entry_matches() {
        let mut asset = asset();

        let result = Filter::parse("type==text").unwrap().apply(&mut asset);

        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_segment_derived_attributes() {
        let result = Filter::parse("bitrate>=800000");

        assert!(result.is_err());
    }
}
