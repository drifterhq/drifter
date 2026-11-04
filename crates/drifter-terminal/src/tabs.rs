pub fn span(from: usize, to: usize) -> Vec<usize> {
    match from <= to {
        true => (from..=to).collect(),
        false => (to..=from).collect(),
    }
}

pub fn toggle<T: PartialEq + Copy>(picked: &mut Vec<T>, one: T) {
    match picked.iter().position(|each| *each == one) {
        Some(at) => {
            picked.remove(at);
        }
        None => picked.push(one),
    }
}

pub fn take<T>(items: &mut Vec<T>, moving: &[usize]) -> Vec<T> {
    let mut order: Vec<usize> = moving.iter().copied().filter(|at| *at < items.len()).collect();
    order.sort_unstable();
    order.dedup();
    let mut out = Vec::with_capacity(order.len());
    for at in order.into_iter().rev() {
        out.push(items.remove(at));
    }
    out.reverse();
    out
}

pub fn reorder<T>(items: &mut Vec<T>, moving: &[usize], slot: usize) {
    let before = moving.iter().filter(|at| **at < slot && **at < items.len()).count();
    let taken = take(items, moving);
    let at = slot.saturating_sub(before).min(items.len());
    for (offset, item) in taken.into_iter().enumerate() {
        items.insert(at + offset, item);
    }
}

pub fn slot(centers: &[f64], along: f64) -> usize {
    centers.iter().filter(|center| **center < along).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_picks_a_run_either_way() {
        assert_eq!(span(1, 3), vec![1, 2, 3]);
        assert_eq!(span(3, 1), vec![1, 2, 3]);
        assert_eq!(span(2, 2), vec![2]);
    }

    #[test]
    fn ctrl_adds_or_drops_one_tab() {
        let mut picked = vec![4, 7];
        toggle(&mut picked, 9);
        toggle(&mut picked, 4);
        assert_eq!(picked, vec![7, 9]);
    }

    #[test]
    fn taking_tabs_keeps_their_order() {
        let mut tabs = vec!['a', 'b', 'c', 'd', 'e'];
        assert_eq!(take(&mut tabs, &[3, 1, 9]), vec!['b', 'd']);
        assert_eq!(tabs, vec!['a', 'c', 'e']);
    }

    #[test]
    fn a_dragged_group_lands_at_the_drop_slot() {
        let mut tabs = vec!['a', 'b', 'c', 'd', 'e'];
        reorder(&mut tabs, &[0, 1], 4);
        assert_eq!(tabs, vec!['c', 'd', 'a', 'b', 'e']);
        let mut tabs = vec!['a', 'b', 'c', 'd', 'e'];
        reorder(&mut tabs, &[4], 0);
        assert_eq!(tabs, vec!['e', 'a', 'b', 'c', 'd']);
        let mut tabs = vec!['a', 'b', 'c'];
        reorder(&mut tabs, &[1], 3);
        assert_eq!(tabs, vec!['a', 'c', 'b']);
    }

    #[test]
    fn the_slot_counts_tab_centers_before_the_pointer() {
        let centers = [50.0, 150.0, 250.0];
        assert_eq!(slot(&centers, 10.0), 0);
        assert_eq!(slot(&centers, 120.0), 1);
        assert_eq!(slot(&centers, 400.0), 3);
    }
}
