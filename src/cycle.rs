pub fn step<T: Copy + PartialEq>(all: &[T], current: T, by: isize) -> T {
    if all.is_empty() {
        return current;
    }
    let i = all.iter().position(|&x| x == current).unwrap_or(0);
    all[(i as isize + by).rem_euclid(all.len() as isize) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_wraps_at_both_ends_and_tolerates_an_empty_slice() {
        let all = ["a", "b", "c"];
        assert_eq!(step(&all, "a", 1), "b");
        assert_eq!(step(&all, "c", 1), "a", "the end wraps to the start");
        assert_eq!(step(&all, "a", -1), "c", "the start wraps to the end");

        // a value that is not in the slice falls back to the first
        assert_eq!(step(&all, "z", 1), "b");

        // an empty slice has nowhere to step, and must not index or divide by zero
        assert_eq!(step(&[], "a", 1), "a");
        assert_eq!(step(&[], "a", -1), "a");
    }
}
