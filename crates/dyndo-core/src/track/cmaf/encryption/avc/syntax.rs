use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Context {
    sps: HashMap<u32, Sps>,
    pps: HashMap<u32, Pps>,
}

#[derive(Clone, Copy)]
struct Sps {
    chroma_format_idc: u32,
    separate_colour_plane: bool,
    log2_max_frame_num: u32,
    pic_order_cnt: PicOrderCnt,
    frame_mbs_only: bool,
}

#[derive(Clone, Copy)]
enum PicOrderCnt {
    Type0 { log2_max_lsb: u32 },
    Type1 { delta_always_zero: bool },
    Type2,
}

#[derive(Clone, Copy)]
struct Pps {
    sps_id: u32,
    entropy_coding: bool,
    bottom_field_pic_order_present: bool,
    num_ref_l0: u32,
    num_ref_l1: u32,
    weighted_pred: bool,
    weighted_bipred_idc: u32,
    deblocking_filter_control_present: bool,
    redundant_pic_cnt_present: bool,
}

impl Context {
    pub(crate) fn new(sps: &[Vec<u8>], pps: &[Vec<u8>]) -> Result<Self, ()> {
        let mut context = Self::default();
        for nal in sps {
            let (id, value) = parse_sps(nal)?;
            context.sps.insert(id, value);
        }
        for nal in pps {
            let (id, value) = parse_pps(nal, &context.sps)?;
            context.pps.insert(id, value);
        }
        Ok(context)
    }

    /// Returns the number of bytes occupied by the NAL header and slice header.
    pub(crate) fn slice_header_size(&self, nal: &[u8]) -> Result<usize, ()> {
        let header = *nal.first().ok_or(())?;
        if header & 0x80 != 0 || !matches!(header & 0x1f, 1 | 5) {
            return Err(());
        }
        let mut bits = Bits::new(nal.get(1..).ok_or(())?);
        bits.ue()?; // first_mb_in_slice
        let slice_type = bits.ue()? % 5;
        let pps = *self.pps.get(&bits.ue()?).ok_or(())?;
        let sps = *self.sps.get(&pps.sps_id).ok_or(())?;

        if sps.separate_colour_plane {
            bits.skip(2)?;
        }
        bits.skip(sps.log2_max_frame_num)?;
        let field_pic = if !sps.frame_mbs_only {
            let field_pic = bits.bit()?;
            if field_pic {
                bits.bit()?;
            }
            field_pic
        } else {
            false
        };
        if header & 0x1f == 5 {
            bits.ue()?;
        }
        match sps.pic_order_cnt {
            PicOrderCnt::Type0 { log2_max_lsb } => {
                bits.skip(log2_max_lsb)?;
                if pps.bottom_field_pic_order_present && !field_pic {
                    bits.se()?;
                }
            }
            PicOrderCnt::Type1 { delta_always_zero } if !delta_always_zero => {
                bits.se()?;
                if pps.bottom_field_pic_order_present && !field_pic {
                    bits.se()?;
                }
            }
            _ => {}
        }
        if pps.redundant_pic_cnt_present {
            bits.ue()?;
        }
        if slice_type == 1 {
            bits.bit()?;
        }
        let mut refs_l0 = pps.num_ref_l0 + 1;
        let mut refs_l1 = pps.num_ref_l1 + 1;
        if matches!(slice_type, 0 | 1 | 3) && bits.bit()? {
            refs_l0 = bits.ue()?.checked_add(1).ok_or(())?;
            if slice_type == 1 {
                refs_l1 = bits.ue()?.checked_add(1).ok_or(())?;
            }
        }
        skip_ref_pic_list_modification(&mut bits, slice_type)?;
        if (pps.weighted_pred && matches!(slice_type, 0 | 3))
            || (pps.weighted_bipred_idc == 1 && slice_type == 1)
        {
            skip_pred_weight_table(
                &mut bits,
                sps.chroma_format_idc,
                refs_l0,
                refs_l1,
                slice_type,
            )?;
        }
        if header >> 5 != 0 {
            skip_dec_ref_pic_marking(&mut bits, header & 0x1f == 5)?;
        }
        if pps.entropy_coding && !matches!(slice_type, 2 | 4) {
            bits.ue()?;
        }
        bits.se()?;
        if matches!(slice_type, 3 | 4) {
            if slice_type == 3 {
                bits.bit()?;
            }
            bits.se()?;
        }
        if pps.deblocking_filter_control_present {
            let disable = bits.ue()?;
            if disable != 1 {
                bits.se()?;
                bits.se()?;
            }
        }
        Ok(1 + bits.encoded_bytes())
    }
}

fn parse_sps(nal: &[u8]) -> Result<(u32, Sps), ()> {
    if nal.first().copied().ok_or(())? & 0x1f != 7 {
        return Err(());
    }
    let mut b = Bits::new(&nal[1..]);
    let profile = b.read(8)?;
    b.skip(16)?; // constraint flags and level_idc
    let id = b.ue()?;
    if id > 31 {
        return Err(());
    }
    let mut chroma_format_idc = 1;
    let mut separate_colour_plane = false;
    if matches!(
        profile,
        44 | 83 | 86 | 100 | 110 | 118 | 122 | 128 | 134 | 135 | 138 | 139 | 144 | 244
    ) {
        chroma_format_idc = b.ue()?;
        if chroma_format_idc > 3 {
            return Err(());
        }
        if chroma_format_idc == 3 {
            separate_colour_plane = b.bit()?;
        }
        b.ue()?;
        b.ue()?;
        b.bit()?;
        if b.bit()? {
            let count = if chroma_format_idc == 3 { 12 } else { 8 };
            for i in 0..count {
                if b.bit()? {
                    skip_scaling_list(&mut b, if i < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    let log2_max_frame_num = b.ue()?.checked_add(4).filter(|v| *v <= 16).ok_or(())?;
    let pic_order_cnt = match b.ue()? {
        0 => PicOrderCnt::Type0 {
            log2_max_lsb: b.ue()?.checked_add(4).filter(|v| *v <= 16).ok_or(())?,
        },
        1 => {
            let delta_always_zero = b.bit()?;
            b.se()?;
            b.se()?;
            for _ in 0..b.ue()? {
                b.se()?;
            }
            PicOrderCnt::Type1 { delta_always_zero }
        }
        2 => PicOrderCnt::Type2,
        _ => return Err(()),
    };
    b.ue()?;
    b.bit()?;
    b.ue()?;
    b.ue()?;
    let frame_mbs_only = b.bit()?;
    if !frame_mbs_only {
        b.bit()?;
    }
    Ok((
        id,
        Sps {
            chroma_format_idc: if separate_colour_plane {
                0
            } else {
                chroma_format_idc
            },
            separate_colour_plane,
            log2_max_frame_num,
            pic_order_cnt,
            frame_mbs_only,
        },
    ))
}

fn parse_pps(nal: &[u8], sps_map: &HashMap<u32, Sps>) -> Result<(u32, Pps), ()> {
    if nal.first().copied().ok_or(())? & 0x1f != 8 {
        return Err(());
    }
    let mut b = Bits::new(&nal[1..]);
    let id = b.ue()?;
    if id > 255 {
        return Err(());
    }
    let sps_id = b.ue()?;
    if !sps_map.contains_key(&sps_id) {
        return Err(());
    }
    let entropy_coding = b.bit()?;
    let bottom_field_pic_order_present = b.bit()?;
    let groups = b.ue()?;
    if groups > 7 {
        return Err(());
    }
    if groups > 0 {
        skip_slice_groups(&mut b, groups)?;
    }
    let num_ref_l0 = b.ue()?;
    let num_ref_l1 = b.ue()?;
    if num_ref_l0 > 31 || num_ref_l1 > 31 {
        return Err(());
    }
    let weighted_pred = b.bit()?;
    let weighted_bipred_idc = b.read(2)?;
    b.se()?;
    b.se()?;
    b.se()?;
    let deblocking_filter_control_present = b.bit()?;
    b.bit()?;
    let redundant_pic_cnt_present = b.bit()?;
    Ok((
        id,
        Pps {
            sps_id,
            entropy_coding,
            bottom_field_pic_order_present,
            num_ref_l0,
            num_ref_l1,
            weighted_pred,
            weighted_bipred_idc,
            deblocking_filter_control_present,
            redundant_pic_cnt_present,
        },
    ))
}

fn skip_scaling_list(b: &mut Bits<'_>, size: u32) -> Result<(), ()> {
    let mut last = 8_i32;
    let mut next = 8_i32;
    for _ in 0..size {
        if next != 0 {
            next = (last + b.se()? + 256) % 256;
        }
        if next != 0 {
            last = next;
        }
    }
    Ok(())
}

fn skip_slice_groups(b: &mut Bits<'_>, groups_minus1: u32) -> Result<(), ()> {
    match b.ue()? {
        0 => {
            for _ in 0..=groups_minus1 {
                b.ue()?;
            }
        }
        1 => {}
        2 => {
            for _ in 0..groups_minus1 {
                b.ue()?;
                b.ue()?;
            }
        }
        3..=5 => {
            b.bit()?;
            b.ue()?;
        }
        6 => {
            let map_units = b.ue()?.checked_add(1).ok_or(())?;
            let width = (32 - groups_minus1.leading_zeros()).max(1);
            for _ in 0..map_units {
                b.skip(width)?;
            }
        }
        _ => return Err(()),
    }
    Ok(())
}

fn skip_ref_pic_list_modification(b: &mut Bits<'_>, slice_type: u32) -> Result<(), ()> {
    if !matches!(slice_type, 2 | 4) && b.bit()? {
        loop {
            match b.ue()? {
                0 | 1 => {
                    b.ue()?;
                }
                2 => {
                    b.ue()?;
                }
                3 => break,
                _ => return Err(()),
            }
        }
    }
    if slice_type == 1 && b.bit()? {
        loop {
            match b.ue()? {
                0 | 1 => {
                    b.ue()?;
                }
                2 => {
                    b.ue()?;
                }
                3 => break,
                _ => return Err(()),
            }
        }
    }
    Ok(())
}

fn skip_pred_weight_table(
    b: &mut Bits<'_>,
    chroma: u32,
    l0: u32,
    l1: u32,
    slice_type: u32,
) -> Result<(), ()> {
    b.ue()?;
    if chroma != 0 {
        b.ue()?;
    }
    skip_weights(b, chroma, l0)?;
    if slice_type == 1 {
        skip_weights(b, chroma, l1)?;
    }
    Ok(())
}

fn skip_weights(b: &mut Bits<'_>, chroma: u32, count: u32) -> Result<(), ()> {
    for _ in 0..count {
        if b.bit()? {
            b.se()?;
            b.se()?;
        }
        if chroma != 0 && b.bit()? {
            for _ in 0..2 {
                b.se()?;
                b.se()?;
            }
        }
    }
    Ok(())
}

fn skip_dec_ref_pic_marking(b: &mut Bits<'_>, idr: bool) -> Result<(), ()> {
    if idr {
        b.skip(2)?;
        return Ok(());
    }
    if b.bit()? {
        loop {
            let operation = b.ue()?;
            match operation {
                0 => break,
                1 | 3 => {
                    b.ue()?;
                }
                2 => {
                    b.ue()?;
                }
                6 => {
                    b.ue()?;
                }
                4 => {
                    b.ue()?;
                }
                5 => {}
                _ => return Err(()),
            }
            if operation == 3 {
                b.ue()?;
            }
        }
    }
    Ok(())
}

struct Bits<'a> {
    data: &'a [u8],
    raw: usize,
    current: u8,
    left: u8,
    zeroes: u8,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            raw: 0,
            current: 0,
            left: 0,
            zeroes: 0,
        }
    }
    fn encoded_bytes(&self) -> usize {
        self.raw
    }
    fn bit(&mut self) -> Result<bool, ()> {
        if self.left == 0 {
            loop {
                let byte = *self.data.get(self.raw).ok_or(())?;
                self.raw += 1;
                if self.zeroes >= 2 && byte == 3 {
                    self.zeroes = 0;
                    continue;
                }
                self.zeroes = if byte == 0 {
                    self.zeroes.saturating_add(1)
                } else {
                    0
                };
                self.current = byte;
                self.left = 8;
                break;
            }
        }
        self.left -= 1;
        Ok(self.current & (1 << self.left) != 0)
    }
    fn read(&mut self, count: u32) -> Result<u32, ()> {
        if count > 32 {
            return Err(());
        }
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | u32::from(self.bit()?);
        }
        Ok(value)
    }
    fn skip(&mut self, count: u32) -> Result<(), ()> {
        self.read(count).map(|_| ())
    }
    fn ue(&mut self) -> Result<u32, ()> {
        let mut zeroes = 0;
        while !self.bit()? {
            zeroes += 1;
            if zeroes > 31 {
                return Err(());
            }
        }
        Ok(((1_u32 << zeroes) - 1) + self.read(zeroes)?)
    }
    fn se(&mut self) -> Result<i32, ()> {
        let value = self.ue()?;
        if value & 1 == 0 {
            Ok(-(value as i32 / 2))
        } else {
            Ok(value.div_ceil(2) as i32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_skips_emulation_prevention_bytes_and_tracks_encoded_size() {
        let mut bits = Bits::new(&[0, 0, 3, 0x80]);

        assert_eq!(bits.read(17), Ok(1));
        assert_eq!(bits.encoded_bytes(), 4);
    }

    #[test]
    fn context_reads_minimal_baseline_slice_header() {
        let sps = nal(0x67, |b| {
            b.bits(66, 8); // baseline profile
            b.bits(0, 8);
            b.bits(30, 8);
            b.ue(0); // sps id
            b.ue(0); // log2_max_frame_num_minus4
            b.ue(0); // pic_order_cnt_type
            b.ue(0); // log2_max_pic_order_cnt_lsb_minus4
            b.ue(1); // max_num_ref_frames
            b.bit(false);
            b.ue(39);
            b.ue(29);
            b.bit(true); // frame_mbs_only_flag
        });
        let pps = nal(0x68, |b| {
            b.ue(0);
            b.ue(0);
            b.bit(false);
            b.bit(false);
            b.ue(0);
            b.ue(0);
            b.ue(0);
            b.bit(false);
            b.bits(0, 2);
            b.se(0);
            b.se(0);
            b.se(0);
            b.bit(true); // deblocking filter syntax is present
            b.bit(false);
            b.bit(false);
        });
        let slice = nal(0x65, |b| {
            b.ue(0);
            b.ue(2); // I slice
            b.ue(0);
            b.bits(0, 4); // frame_num
            b.ue(0); // idr_pic_id
            b.bits(0, 4); // pic_order_cnt_lsb
            b.bits(0, 2); // IDR reference picture marking
            b.se(0); // slice_qp_delta
            b.ue(1); // disable_deblocking_filter_idc
        });
        let context = Context::new(&[sps], &[pps]).expect("parameter sets should be valid");

        assert_eq!(context.slice_header_size(&slice), Ok(4));
    }

    #[test]
    fn context_rejects_slice_with_unknown_picture_parameter_set() {
        let context = Context::default();
        let slice = nal(0x41, |b| {
            b.ue(0);
            b.ue(2);
            b.ue(0);
        });

        assert_eq!(context.slice_header_size(&slice), Err(()));
    }

    fn nal(header: u8, write: impl FnOnce(&mut BitWriter)) -> Vec<u8> {
        let mut writer = BitWriter::default();
        write(&mut writer);
        let mut bytes = vec![header];
        bytes.extend(writer.finish());
        bytes
    }

    #[derive(Default)]
    struct BitWriter {
        bits: Vec<bool>,
    }

    impl BitWriter {
        fn bit(&mut self, value: bool) {
            self.bits.push(value);
        }

        fn bits(&mut self, value: u32, count: u32) {
            for shift in (0..count).rev() {
                self.bit(value & (1 << shift) != 0);
            }
        }

        fn ue(&mut self, value: u32) {
            let encoded = value + 1;
            let width = 32 - encoded.leading_zeros();
            self.bits
                .extend(std::iter::repeat_n(false, (width - 1) as usize));
            self.bits(encoded, width);
        }

        fn se(&mut self, value: i32) {
            let encoded = if value <= 0 {
                (-value as u32) * 2
            } else {
                value as u32 * 2 - 1
            };
            self.ue(encoded);
        }

        fn finish(mut self) -> Vec<u8> {
            self.bit(true); // rbsp_stop_one_bit
            while !self.bits.len().is_multiple_of(8) {
                self.bit(false);
            }
            self.bits
                .chunks_exact(8)
                .map(|chunk| {
                    chunk
                        .iter()
                        .fold(0, |byte, bit| (byte << 1) | u8::from(*bit))
                })
                .collect()
        }
    }
}
