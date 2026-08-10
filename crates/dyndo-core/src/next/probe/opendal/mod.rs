mod wvtt;

use ::opendal::Operator;

use self::wvtt::WvttLayer;
use super::super::segment_options::SegmentOptions;

pub(super) fn add_operator_layers(op: &Operator, options: &SegmentOptions) -> Operator {
    op.clone()
        .layer(WvttLayer::new(&options.boundaries, options.text_length))
}
