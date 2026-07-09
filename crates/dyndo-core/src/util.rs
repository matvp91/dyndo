pub(crate) fn group_by_key<T, K: PartialEq>(
    items: &[T],
    key: impl Fn(&T) -> K,
) -> Vec<(K, Vec<usize>)> {
    let mut groups: Vec<(K, Vec<usize>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let k = key(item);
        match groups.iter_mut().find(|(existing, _)| *existing == k) {
            Some((_, idxs)) => idxs.push(i),
            None => groups.push((k, vec![i])),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_first_seen_order_of_keys_and_members() {
        let items = ["apple", "avocado", "banana", "cherry", "blueberry"];
        let groups = group_by_key(&items, |s| s.as_bytes()[0]);

        // Keys in first-seen order: a, b, c. Members preserve input order, so
        // "blueberry" (index 4) joins the 'b' bucket after "banana" (index 2).
        assert_eq!(
            groups,
            vec![(b'a', vec![0, 1]), (b'b', vec![2, 4]), (b'c', vec![3])]
        );
    }

    #[test]
    fn empty_input_yields_no_groups() {
        let items: [&str; 0] = [];
        let groups = group_by_key(&items, |s| s.len());
        assert!(groups.is_empty());
    }
}
