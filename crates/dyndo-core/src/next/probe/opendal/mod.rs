mod vtt;

use ::opendal::Operator;

use self::vtt::VttLayer;
use super::super::segment_options::SegmentOptions;

pub(super) fn add_operator_layers(op: &Operator, options: &SegmentOptions) -> Operator {
    op.clone()
        .layer(VttLayer::new(&options.boundaries, options.text_length))
}
