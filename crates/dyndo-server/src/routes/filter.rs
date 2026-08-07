//! Which of an asset's tracks a request asks to be served, as a boolean expression
//! over track attributes.
//!
//! The expression language follows Unified Streaming's URL filters, so an operator
//! arriving from that stack reads ours without relearning it — including the rule
//! that a track is kept only when the expression is true *for that track*. A
//! comparison against an attribute the track does not carry is false whatever the
//! operator, so `height<=720` alone drops every audio and text track, and sparing a
//! type takes the `type!=video||…` idiom. `type` is the one attribute every track
//! has, which is what makes that idiom work.
//!
//! Filtering reads resolved tracks rather than descriptors: `bitrate`,
//! `avg_bitrate` and `duration` exist only once a track has been probed, and `codec`
//! is then the probed value rather than the descriptor's claim.

use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use dyndo_core::role::Role;
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::{Track, average_bitrate, max_bitrate};
use serde::Deserialize;
use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{
    Infix, alt, cut_err, delimited, dispatch, expression, fail, preceded, terminated,
};
use winnow::error::{StrContext, StrContextValue};
use winnow::prelude::*;
use winnow::token::take_while;

use crate::error::ServerError;

/// Every attribute a filter can name, each row carrying how it is spelled, whether
/// it orders, and where its value comes from. Adding an attribute is one row.
///
/// `numeric` decides two things at once: which parser reads the value, and whether
/// `<`, `<=`, `>` and `>=` are accepted at all.
const ATTRIBUTES: &[Attribute] = &[
    Attribute::text("type", |_, track| Some(track.content_type())),
    Attribute::text("id", |descriptor, _| Some(descriptor.id.as_str())),
    Attribute::text("codec", |_, track| Some(track.codec())),
    Attribute::text("frame_rate", |_, track| match track.kind() {
        TrackKind::Video(video) => Some(video.frame_rate.as_str()),
        _ => None,
    }),
    Attribute::text("language", |_, track| match track.kind() {
        TrackKind::Audio(audio) => Some(audio.language.as_str()),
        TrackKind::Text(text) => Some(text.language.as_str()),
        TrackKind::Video(_) => None,
    }),
    Attribute::text("role", |_, track| match track.kind() {
        TrackKind::Audio(audio) => audio.role.map(Role::as_str),
        TrackKind::Text(text) => text.role.map(Role::as_str),
        TrackKind::Video(_) => None,
    }),
    Attribute::number("bitrate", |track, options| {
        Some(max_bitrate(track, options))
    }),
    Attribute::number("avg_bitrate", |track, options| {
        Some(average_bitrate(track, options))
    }),
    Attribute::number("duration", |track, _| Some(track.duration().into())),
    Attribute::number("width", |track, _| match track.kind() {
        TrackKind::Video(video) => Some(video.width.into()),
        _ => None,
    }),
    Attribute::number("height", |track, _| match track.kind() {
        TrackKind::Video(video) => Some(video.height.into()),
        _ => None,
    }),
    Attribute::number("sample_rate", |track, _| match track.kind() {
        TrackKind::Audio(audio) => Some(audio.sample_rate.into()),
        _ => None,
    }),
    Attribute::number("channels", |track, _| match track.kind() {
        TrackKind::Audio(audio) => Some(audio.channels.into()),
        _ => None,
    }),
];

/// The `filter` query parameter and its shorthand, as a request spells them.
#[derive(Debug, Default, Deserialize)]
pub(super) struct FilterQuery {
    filter: Option<String>,
    f: Option<String>,
}

impl FilterQuery {
    /// Parses whichever spelling the request used.
    ///
    /// `raw` is the undecoded query string, needed only to catch an unencoded `&&`:
    /// that splits the query into separate parameters, leaving a filter that is
    /// still valid on its own — `f=type!=video` out of `f=type!=video&&height<=720`
    /// — so the request would otherwise succeed while silently ignoring half of what
    /// it asked for. A well-formed query string never contains `&&`.
    pub(super) fn resolve(&self, raw: Option<&str>) -> Result<Option<Filter>, ServerError> {
        if raw.is_some_and(|raw| raw.contains("&&")) {
            return Err(ServerError::InvalidFilter(
                "unencoded `&&`: percent-encode it as `%26%26`".to_string(),
            ));
        }
        match (&self.filter, &self.f) {
            (Some(_), Some(_)) => Err(ServerError::InvalidFilter(
                "`filter` and `f` are the same option; pass one".to_string(),
            )),
            (Some(expression), None) | (None, Some(expression)) => {
                Filter::parse(expression).map(Some)
            }
            (None, None) => Ok(None),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Filter(Expr);

impl Filter {
    fn parse(expression: &str) -> Result<Self, ServerError> {
        terminated(expr, multispace0)
            .parse(expression)
            .map(Self)
            .map_err(|error| {
                ServerError::InvalidFilter(format!(
                    "at offset {}: {}",
                    error.offset(),
                    error.inner()
                ))
            })
    }
}

/// Drops the tracks the filter rejects.
///
/// The descriptor list and the probed tracks are zipped and unzipped rather than
/// filtered apart, because the builders pair them positionally: manifests take ids
/// from the descriptor and media facts from the track, and a `zip` over lists that
/// disagree would quietly emit one track's id for another track's media.
pub(super) fn prune(
    mut asset: AssetDescriptor,
    tracks: Vec<Track>,
    filter: Option<&Filter>,
) -> Result<(AssetDescriptor, Vec<Track>), ServerError> {
    let Some(Filter(expr)) = filter else {
        return Ok((asset, tracks));
    };

    let declared = std::mem::take(&mut asset.tracks);
    let (descriptors, tracks): (Vec<_>, Vec<_>) = declared
        .into_iter()
        .zip(tracks)
        .filter(|(descriptor, track)| expr.matches(descriptor, track, &asset.segment_options))
        .unzip();

    if descriptors.is_empty() {
        return Err(ServerError::FilterMatchedNothing);
    }
    asset.tracks = descriptors;

    Ok((asset, tracks))
}

#[derive(Debug, PartialEq, Eq)]
enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cmp(&'static Attribute, Op, Value),
}

impl Expr {
    fn matches(&self, descriptor: &TrackDescriptor, track: &Track, of: &SegmentOptions) -> bool {
        match self {
            Self::And(left, right) => {
                left.matches(descriptor, track, of) && right.matches(descriptor, track, of)
            }
            Self::Or(left, right) => {
                left.matches(descriptor, track, of) || right.matches(descriptor, track, of)
            }
            Self::Cmp(attribute, op, wanted) => attribute
                .of(descriptor, track, of)
                .is_some_and(|actual| op.holds(&actual, wanted)),
        }
    }
}

/// One filterable track attribute.
#[derive(Debug)]
struct Attribute {
    name: &'static str,
    extract: Extract,
}

/// Names are unique within [`ATTRIBUTES`], so a name identifies an attribute — and
/// its extractor is a function pointer, which does not compare meaningfully.
impl PartialEq for Attribute {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Attribute {}

/// Where an attribute's value comes from, which also settles its type: only a
/// numeric attribute can be ordered.
#[derive(Debug)]
enum Extract {
    Text(for<'a> fn(&'a TrackDescriptor, &'a Track) -> Option<&'a str>),
    Number(fn(&Track, &SegmentOptions) -> Option<u64>),
}

impl Attribute {
    const fn text(
        name: &'static str,
        extract: for<'a> fn(&'a TrackDescriptor, &'a Track) -> Option<&'a str>,
    ) -> Self {
        Self {
            name,
            extract: Extract::Text(extract),
        }
    }

    const fn number(
        name: &'static str,
        extract: fn(&Track, &SegmentOptions) -> Option<u64>,
    ) -> Self {
        Self {
            name,
            extract: Extract::Number(extract),
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self.extract, Extract::Number(_))
    }

    /// The track's own value, or `None` when the track does not carry the attribute
    /// — a video track has no language, an audio track no height.
    fn of(
        &self,
        descriptor: &TrackDescriptor,
        track: &Track,
        options: &SegmentOptions,
    ) -> Option<Value> {
        match self.extract {
            Extract::Text(extract) => {
                extract(descriptor, track).map(|text| Value::Text(text.to_string()))
            }
            Extract::Number(extract) => extract(track, options).map(Value::Number),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn is_ordering(self) -> bool {
        !matches!(self, Self::Eq | Self::Ne)
    }

    fn holds(self, actual: &Value, wanted: &Value) -> bool {
        let ordering = actual.cmp(wanted);
        match self {
            Self::Eq => ordering.is_eq(),
            Self::Ne => !ordering.is_eq(),
            Self::Lt => ordering.is_lt(),
            Self::Le => ordering.is_le(),
            Self::Gt => ordering.is_gt(),
            Self::Ge => ordering.is_ge(),
        }
    }
}

/// Ordering a `Number` against a `Number` is the numeric comparison the ordering
/// operators want. The variants never mix, because an attribute's type decides both
/// how its value is parsed and what the track is asked for.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Value {
    Number(u64),
    Text(String),
}

/// `||` binds loosest, then `&&`; parentheses group. The enclosing parentheses
/// Unified writes are accepted but not required, which keeps a filter from reading
/// like the rison options object earlier in the same URL.
fn expr(input: &mut &str) -> ModalResult<Expr> {
    expression(operand)
        .infix(
            dispatch! {delimited(multispace0, alt(("&&", "||")), multispace0);
                "&&" => Infix::Left(2, |_: &mut &str, left: Expr, right: Expr| {
                    Ok(Expr::And(Box::new(left), Box::new(right)))
                }),
                "||" => Infix::Left(1, |_: &mut &str, left: Expr, right: Expr| {
                    Ok(Expr::Or(Box::new(left), Box::new(right)))
                }),
                _ => fail,
            },
        )
        .parse_next(input)
}

fn operand(input: &mut &str) -> ModalResult<Expr> {
    preceded(
        multispace0,
        alt((delimited('(', expr, preceded(multispace0, ')')), comparison)),
    )
    .parse_next(input)
}

fn comparison(input: &mut &str) -> ModalResult<Expr> {
    let attribute = cut_err(
        take_while(1.., ('a'..='z', '_'))
            .verify_map(|name: &str| ATTRIBUTES.iter().find(|attribute| attribute.name == name))
            .context(StrContext::Label("track attribute")),
    )
    .parse_next(input)?;
    let op = delimited(multispace0, operator, multispace0).parse_next(input)?;
    if op.is_ordering() && !attribute.is_numeric() {
        return cut_err(
            fail.context(StrContext::Expected(StrContextValue::Description(
                "a numeric attribute before an ordering operator",
            ))),
        )
        .parse_next(input);
    }
    let value = value(attribute.is_numeric(), input)?;

    Ok(Expr::Cmp(attribute, op, value))
}

/// `<` and `>` are not legal URI characters, so a request carries them
/// percent-encoded and they arrive here already decoded.
fn operator(input: &mut &str) -> ModalResult<Op> {
    // `<=` and `>=` come first: `alt` takes the first branch that matches, so the
    // one-character forms would otherwise consume their prefix and leave the `=`.
    alt((
        "==".value(Op::Eq),
        "!=".value(Op::Ne),
        "<=".value(Op::Le),
        ">=".value(Op::Ge),
        "<".value(Op::Lt),
        ">".value(Op::Gt),
    ))
    .context(StrContext::Label("comparison operator"))
    .parse_next(input)
}

/// A value is read at the type its attribute takes, so `height==tall` fails at parse
/// time rather than never matching at run time. A textual value is not checked
/// against what exists — an unknown language or role simply matches no track.
fn value(numeric: bool, input: &mut &str) -> ModalResult<Value> {
    if numeric {
        cut_err(digit1.parse_to().context(StrContext::Label("whole number")))
            .map(Value::Number)
            .parse_next(input)
    } else {
        // Enough for codecs (`avc1.640028`), ids (`video_6b74…`), language tags
        // (`nl-BE`) and frame rates (`25/1`), while stopping at whatever follows.
        cut_err(
            take_while(1.., |c: char| {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/')
            })
            .context(StrContext::Label("value")),
        )
        .map(|value: &str| Value::Text(value.to_string()))
        .parse_next(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(expression: &str) -> Filter {
        Filter::parse(expression).unwrap()
    }

    fn attribute(name: &str) -> &'static Attribute {
        ATTRIBUTES
            .iter()
            .find(|attribute| attribute.name == name)
            .unwrap()
    }

    fn number(name: &str, op: Op, value: u64) -> Expr {
        Expr::Cmp(attribute(name), op, Value::Number(value))
    }

    fn text(name: &str, op: Op, value: &str) -> Expr {
        Expr::Cmp(attribute(name), op, Value::Text(value.to_string()))
    }

    #[test]
    fn parses_a_single_comparison() {
        assert_eq!(parse("height<=720"), Filter(number("height", Op::Le, 720)));
    }

    #[test]
    fn parses_every_operator() {
        for (spelling, op) in [
            ("==", Op::Eq),
            ("!=", Op::Ne),
            ("<", Op::Lt),
            ("<=", Op::Le),
            (">", Op::Gt),
            (">=", Op::Ge),
        ] {
            assert_eq!(
                parse(&format!("height{spelling}720")),
                Filter(number("height", op, 720)),
                "for {spelling}"
            );
        }
    }

    /// `&&` binds tighter than `||`, so the conjunction is the right operand.
    #[test]
    fn conjunction_binds_tighter_than_disjunction() {
        assert_eq!(
            parse("type==text||type==video&&height<=720"),
            Filter(Expr::Or(
                Box::new(text("type", Op::Eq, "text")),
                Box::new(Expr::And(
                    Box::new(text("type", Op::Eq, "video")),
                    Box::new(number("height", Op::Le, 720)),
                )),
            ))
        );
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(
            parse("(type==text||type==video)&&height<=720"),
            Filter(Expr::And(
                Box::new(Expr::Or(
                    Box::new(text("type", Op::Eq, "text")),
                    Box::new(text("type", Op::Eq, "video")),
                )),
                Box::new(number("height", Op::Le, 720)),
            ))
        );
    }

    #[test]
    fn enclosing_parentheses_are_optional() {
        assert_eq!(parse("(type==video)"), parse("type==video"));
    }

    #[test]
    fn whitespace_around_operators_is_allowed() {
        assert_eq!(parse("  height  <=  720  "), parse("height<=720"));
    }

    #[test]
    fn parses_values_holding_dots_dashes_and_slashes() {
        assert_eq!(
            parse("codec==avc1.640028"),
            Filter(text("codec", Op::Eq, "avc1.640028"))
        );
        assert_eq!(
            parse("frame_rate==25/1"),
            Filter(text("frame_rate", Op::Eq, "25/1"))
        );
        assert_eq!(
            parse("language==nl-BE"),
            Filter(text("language", Op::Eq, "nl-BE"))
        );
    }

    /// Textual values are not checked against what exists, so an unknown role parses
    /// and simply matches no track.
    #[test]
    fn an_unknown_textual_value_parses() {
        assert_eq!(
            parse("role==narrator"),
            Filter(text("role", Op::Eq, "narrator"))
        );
    }

    #[test]
    fn rejects_an_unknown_attribute() {
        assert!(Filter::parse("heigth<=720").is_err());
    }

    #[test]
    fn rejects_an_ordering_operator_on_a_textual_attribute() {
        for expression in ["language<nl", "type>video", "codec>=avc1", "role<main"] {
            assert!(Filter::parse(expression).is_err(), "for {expression}");
        }
    }

    #[test]
    fn rejects_a_non_numeric_value_for_a_numeric_attribute() {
        for expression in ["height==tall", "channels==stereo", "bitrate<=fast"] {
            assert!(Filter::parse(expression).is_err(), "for {expression}");
        }
    }

    #[test]
    fn rejects_an_empty_or_incomplete_expression() {
        for expression in ["", "   ", "height", "height<=", "<=720", "height<=720&&"] {
            assert!(Filter::parse(expression).is_err(), "for {expression}");
        }
    }

    #[test]
    fn rejects_unbalanced_parentheses() {
        for expression in ["(height<=720", "height<=720)", "((height<=720)"] {
            assert!(Filter::parse(expression).is_err(), "for {expression}");
        }
    }

    #[test]
    fn rejects_trailing_input() {
        assert!(Filter::parse("height<=720 height<=1080").is_err());
    }

    #[test]
    fn parse_error_reports_an_offset() {
        let error = Filter::parse("type==video&&heigth<=720").unwrap_err();

        assert!(
            error.to_string().contains("offset 13"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn resolve_accepts_either_spelling() {
        let long = FilterQuery {
            filter: Some("type==video".to_string()),
            f: None,
        };
        let short = FilterQuery {
            filter: None,
            f: Some("type==video".to_string()),
        };

        assert_eq!(
            long.resolve(None).unwrap().unwrap(),
            short.resolve(None).unwrap().unwrap()
        );
    }

    #[test]
    fn resolve_without_a_filter_yields_none() {
        assert!(FilterQuery::default().resolve(None).unwrap().is_none());
    }

    #[test]
    fn resolve_rejects_both_spellings_at_once() {
        let query = FilterQuery {
            filter: Some("type==video".to_string()),
            f: Some("type==audio".to_string()),
        };

        assert!(query.resolve(None).is_err());
    }

    /// An unencoded `&&` splits the query string, leaving a filter that parses on its
    /// own — so it has to be caught from the raw query or it silently drops half of
    /// what was asked for.
    #[test]
    fn resolve_rejects_an_unencoded_conjunction() {
        let query = FilterQuery {
            filter: None,
            f: Some("type!=video".to_string()),
        };

        assert!(query.resolve(Some("f=type!=video&&height<=720")).is_err());
    }

    #[test]
    fn resolve_accepts_an_encoded_conjunction() {
        let query = FilterQuery {
            filter: None,
            f: Some("type!=video&&height<=720".to_string()),
        };

        assert!(
            query
                .resolve(Some("f=type!=video%26%26height%3C=720"))
                .unwrap()
                .is_some()
        );
    }
}
