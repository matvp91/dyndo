use std::ops::Range;

pub(crate) fn partition<T>(range: Range<T>, cuts: impl IntoIterator<Item = T>) -> Vec<Range<T>>
where
    T: Ord + Copy,
{
    let mut edges: Vec<_> = cuts
        .into_iter()
        .filter(|&cut| cut > range.start && cut < range.end)
        .chain([range.start, range.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();

    edges.windows(2).map(|edges| edges[0]..edges[1]).collect()
}
