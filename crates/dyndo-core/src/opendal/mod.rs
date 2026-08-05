//! The layers dyndo reads assets through.
//!
//! A stored file is not always a CMAF track: a subtitle document is packaged into
//! one as it is read. Track reads go through [`add_operator_layers`], so the rest
//! of the crate only ever sees CMAF.

mod wvtt_layer;

use ::opendal::Operator;

use crate::opendal::wvtt_layer::WvttLayer;
use crate::segment::SegmentOptions;

/// Clones `op` with the layers that present a stored file as a CMAF track, packing
/// subtitle documents into `wvtt` as they are read.
pub(crate) fn add_operator_layers(op: &Operator, options: &SegmentOptions) -> Operator {
    op.clone().layer(WvttLayer::new(
        options.boundaries_ms(),
        options.text_segment_length_ms,
    ))
}
