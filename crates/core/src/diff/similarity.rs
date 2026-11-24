/// Levenshtein similarity in millionths. A name supplies scalar values; an ancestor chain
/// supplies complete names.
#[must_use]
pub fn sequence<T: Eq>(left: &[T], right: &[T]) -> u32 {
    let longest = left.len().max(right.len());
    if longest == 0 {
        return 1_000_000;
    }
    let mut row: Vec<usize> = (0..=right.len()).collect();
    for (i, left_item) in left.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right_item) in right.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(left_item != right_item))
                .min(above + 1)
                .min(row[j] + 1);
            diagonal = above;
        }
    }
    ratio((longest - row[right.len()]) as u128, longest as u128)
}

/// Multiset intersection divided by union, counting occurrences.
#[must_use]
pub fn multiset<T: Ord>(
    left: &std::collections::BTreeMap<T, u64>,
    right: &std::collections::BTreeMap<T, u64>,
) -> u32 {
    let intersection: u128 = left
        .iter()
        .map(|(k, numerator)| u128::from((*numerator).min(*right.get(k).unwrap_or(&0))))
        .sum();
    let total: u128 = left
        .values()
        .chain(right.values())
        .copied()
        .map(u128::from)
        .sum();
    ratio(intersection, total - intersection)
}

fn ratio(numerator: u128, denominator: u128) -> u32 {
    u32::try_from(
        (numerator * 1_000_000)
            .checked_div(denominator)
            .unwrap_or(0),
    )
    .expect("fraction at most one")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn edits_count_sequence_elements() {
        assert_eq!(sequence(&['猫', '鳥'], &['魚', '鳥', '犬']), 333_333);
        assert_eq!(sequence(&["mod", "parse"], &["mod", "parser"]), 500_000);
        assert_eq!(sequence::<char>(&[], &[]), 1_000_000);
        assert_eq!(sequence(&['a'], &[]), 0);
        assert_eq!(sequence(&['a', 'b'], &['a']), 500_000);
    }

    #[test]
    fn multiset_counts_repeated_and_unshared_items() {
        let left = BTreeMap::from([("a", 3), ("b", 1)]);
        let right = BTreeMap::from([("a", 2), ("c", 1)]);
        assert_eq!(multiset(&left, &right), 400_000);
        assert_eq!(multiset(&BTreeMap::new(), &right), 0);
        assert_eq!(multiset::<u8>(&BTreeMap::new(), &BTreeMap::new()), 0);
    }
}
