//! V8's `Array.prototype.sort(comparefn)`, reproduced step for step.
//!
//! With a consistent comparator any stable sort gives the same result, but
//! the TS nodes sort numbers with `(a, b) => a - b`, which returns NaN for
//! NaN keys. The spec turns a NaN result into +0 ("equal"), and an "equal
//! to everything" element makes the final order depend on exactly which
//! comparisons the algorithm performs. Rust's `sort_by` would also be free
//! to panic on such a comparator. So this is a port of V8's TimSort
//! (`third_party/v8/builtins/array-sort.tq`): binary insertion sort for
//! short runs, galloping merges, `minGallop` adaptation.

use crate::value::Value;

const MIN_GALLOP_WINS: usize = 7;

/// `array.sort(cmp)` on a spread: `undefined` entries go to the end without
/// being compared (spec `SortIndexedProperties`), the rest are TimSorted.
pub fn sort_values(items: &[Value], cmp: impl FnMut(&Value, &Value) -> f64) -> Vec<Value> {
    let mut defined: Vec<Value> = items
        .iter()
        .filter(|v| !v.is_undefined())
        .cloned()
        .collect();
    let n_undef = items.len() - defined.len();
    sort_by(&mut defined, cmp);
    defined.extend(std::iter::repeat_n(Value::Undefined, n_undef));
    defined
}

/// TimSort with a JS comparator (`< 0` means "a before b"; NaN counts as 0).
pub fn sort_by<T: Clone>(a: &mut Vec<T>, cmp: impl FnMut(&T, &T) -> f64) {
    let n = a.len();
    if n < 2 {
        return;
    }
    let mut ts = TimSort {
        a: std::mem::take(a),
        cmp,
        min_gallop: MIN_GALLOP_WINS,
        runs: Vec::new(),
    };
    ts.run();
    *a = ts.a;
}

struct TimSort<T, F> {
    a: Vec<T>,
    cmp: F,
    min_gallop: usize,
    /// Pending runs: (base, length).
    runs: Vec<(usize, usize)>,
}

fn order<T, F: FnMut(&T, &T) -> f64>(cmp: &mut F, x: &T, y: &T) -> f64 {
    let v = cmp(x, y);
    if v.is_nan() {
        0.0
    } else {
        v
    }
}

fn min_run_length(mut n: usize) -> usize {
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

/// Leftmost position in `arr[base..base+length]` where `key` belongs.
fn gallop_left<T, F: FnMut(&T, &T) -> f64>(
    cmp: &mut F,
    arr: &[T],
    key: &T,
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let at = |i: isize| &arr[base + i as usize];
    let (length, hint) = (length as isize, hint as isize);
    let mut last_ofs: isize = 0;
    let mut offset: isize = 1;
    if order(cmp, at(hint), key) < 0.0 {
        let max_ofs = length - hint;
        while offset < max_ofs {
            if order(cmp, at(hint + offset), key) >= 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        last_ofs += hint;
        offset += hint;
    } else {
        let max_ofs = hint + 1;
        while offset < max_ofs {
            if order(cmp, at(hint - offset), key) < 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        let tmp = last_ofs;
        last_ofs = hint - offset;
        offset = hint - tmp;
    }
    last_ofs += 1;
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        if order(cmp, at(m), key) < 0.0 {
            last_ofs = m + 1;
        } else {
            offset = m;
        }
    }
    offset as usize
}

/// Rightmost position in `arr[base..base+length]` where `key` belongs.
fn gallop_right<T, F: FnMut(&T, &T) -> f64>(
    cmp: &mut F,
    arr: &[T],
    key: &T,
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let at = |i: isize| &arr[base + i as usize];
    let (length, hint) = (length as isize, hint as isize);
    let mut last_ofs: isize = 0;
    let mut offset: isize = 1;
    if order(cmp, key, at(hint)) < 0.0 {
        let max_ofs = hint + 1;
        while offset < max_ofs {
            if order(cmp, key, at(hint - offset)) >= 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        let tmp = last_ofs;
        last_ofs = hint - offset;
        offset = hint - tmp;
    } else {
        let max_ofs = length - hint;
        while offset < max_ofs {
            if order(cmp, key, at(hint + offset)) < 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        if offset > max_ofs {
            offset = max_ofs;
        }
        last_ofs += hint;
        offset += hint;
    }
    last_ofs += 1;
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        if order(cmp, key, at(m)) < 0.0 {
            offset = m;
        } else {
            last_ofs = m + 1;
        }
    }
    offset as usize
}

enum End {
    Succeed,
    CopyRest,
}

impl<T: Clone, F: FnMut(&T, &T) -> f64> TimSort<T, F> {
    fn run(&mut self) {
        let n = self.a.len();
        let min_run = min_run_length(n);
        let mut low = 0;
        let mut remaining = n;
        while remaining != 0 {
            let mut cur = self.count_and_make_run(low, low + remaining);
            if cur < min_run {
                let forced = min_run.min(remaining);
                self.binary_insertion_sort(low, low + cur, low + forced);
                cur = forced;
            }
            self.runs.push((low, cur));
            self.merge_collapse();
            low += cur;
            remaining -= cur;
        }
        self.merge_force_collapse();
    }

    fn cmp(&mut self, x: &T, y: &T) -> f64 {
        order(&mut self.cmp, x, y)
    }

    fn count_and_make_run(&mut self, low: usize, high: usize) -> usize {
        if low + 1 == high {
            return 1;
        }
        let mut run = 2;
        let el_low = self.a[low].clone();
        let el_pre = self.a[low + 1].clone();
        let desc = self.cmp(&el_pre, &el_low) < 0.0;
        let mut prev = el_pre;
        for idx in low + 2..high {
            let cur = self.a[idx].clone();
            let ord = self.cmp(&cur, &prev);
            if desc {
                if ord >= 0.0 {
                    break;
                }
            } else if ord < 0.0 {
                break;
            }
            prev = cur;
            run += 1;
        }
        if desc {
            self.a[low..low + run].reverse();
        }
        run
    }

    fn binary_insertion_sort(&mut self, low: usize, start: usize, high: usize) {
        let mut start = if low == start { start + 1 } else { start };
        while start < high {
            let mut left = low;
            let mut right = start;
            let pivot = self.a[start].clone();
            while left < right {
                let mid = left + ((right - left) >> 1);
                let m = self.a[mid].clone();
                if self.cmp(&pivot, &m) < 0.0 {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            let mut p = start;
            while p > left {
                self.a[p] = self.a[p - 1].clone();
                p -= 1;
            }
            self.a[left] = pivot;
            start += 1;
        }
    }

    fn invariant(&self, n: usize) -> bool {
        if n < 2 {
            return true;
        }
        self.runs[n - 2].1 > self.runs[n - 1].1 + self.runs[n].1
    }

    fn merge_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if !self.invariant(n + 1) || !self.invariant(n) {
                if self.runs[n - 1].1 < self.runs[n + 1].1 {
                    n -= 1;
                }
                self.merge_at(n);
            } else if self.runs[n].1 <= self.runs[n + 1].1 {
                self.merge_at(n);
            } else {
                break;
            }
        }
    }

    fn merge_force_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if n > 0 && self.runs[n - 1].1 < self.runs[n + 1].1 {
                n -= 1;
            }
            self.merge_at(n);
        }
    }

    fn merge_at(&mut self, i: usize) {
        let (mut base_a, mut la) = self.runs[i];
        let (base_b, mut lb) = self.runs[i + 1];
        self.runs[i].1 = la + lb;
        if i + 3 == self.runs.len() {
            self.runs[i + 1] = self.runs[i + 2];
        }
        self.runs.pop();

        let key_right = self.a[base_b].clone();
        let k = gallop_right(&mut self.cmp, &self.a, &key_right, base_a, la, 0);
        base_a += k;
        la -= k;
        if la == 0 {
            return;
        }
        let key_left = self.a[base_a + la - 1].clone();
        lb = gallop_left(&mut self.cmp, &self.a, &key_left, base_b, lb, lb - 1);
        if lb == 0 {
            return;
        }
        if la <= lb {
            self.merge_low(base_a, la, base_b, lb);
        } else {
            self.merge_high(base_a, la, base_b, lb);
        }
    }

    fn merge_low(&mut self, base_a: usize, mut la: usize, base_b: usize, mut lb: usize) {
        let tmp: Vec<T> = self.a[base_a..base_a + la].to_vec();
        let mut dest = base_a;
        let mut ct = 0;
        let mut cb = base_b;
        self.a[dest] = self.a[cb].clone();
        dest += 1;
        cb += 1;
        let end = 'body: {
            lb -= 1;
            if lb == 0 {
                break 'body End::Succeed;
            }
            if la == 1 {
                break 'body End::CopyRest;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                loop {
                    let (x, y) = (self.a[cb].clone(), tmp[ct].clone());
                    if self.cmp(&x, &y) < 0.0 {
                        self.a[dest] = x;
                        dest += 1;
                        cb += 1;
                        wins_b += 1;
                        lb -= 1;
                        wins_a = 0;
                        if lb == 0 {
                            break 'body End::Succeed;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    } else {
                        self.a[dest] = y;
                        dest += 1;
                        ct += 1;
                        wins_a += 1;
                        la -= 1;
                        wins_b = 0;
                        if la == 1 {
                            break 'body End::CopyRest;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first {
                    first = false;
                    min_gallop = min_gallop.saturating_sub(1).max(1);
                    self.min_gallop = min_gallop;
                    let key = self.a[cb].clone();
                    wins_a = gallop_right(&mut self.cmp, &tmp, &key, ct, la, 0);
                    if wins_a > 0 {
                        self.a[dest..dest + wins_a].clone_from_slice(&tmp[ct..ct + wins_a]);
                        dest += wins_a;
                        ct += wins_a;
                        la -= wins_a;
                        if la == 1 {
                            break 'body End::CopyRest;
                        }
                        if la == 0 {
                            break 'body End::Succeed;
                        }
                    }
                    self.a[dest] = self.a[cb].clone();
                    dest += 1;
                    cb += 1;
                    lb -= 1;
                    if lb == 0 {
                        break 'body End::Succeed;
                    }
                    let key = tmp[ct].clone();
                    wins_b = gallop_left(&mut self.cmp, &self.a, &key, cb, lb, 0);
                    if wins_b > 0 {
                        for k in 0..wins_b {
                            self.a[dest + k] = self.a[cb + k].clone();
                        }
                        dest += wins_b;
                        cb += wins_b;
                        lb -= wins_b;
                        if lb == 0 {
                            break 'body End::Succeed;
                        }
                    }
                    self.a[dest] = tmp[ct].clone();
                    dest += 1;
                    ct += 1;
                    la -= 1;
                    if la == 1 {
                        break 'body End::CopyRest;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        match end {
            End::Succeed => {
                if la > 0 {
                    self.a[dest..dest + la].clone_from_slice(&tmp[ct..ct + la]);
                }
            }
            End::CopyRest => {
                // The last element of run A belongs at the end of the merge.
                for k in 0..lb {
                    self.a[dest + k] = self.a[cb + k].clone();
                }
                self.a[dest + lb] = tmp[ct].clone();
            }
        }
    }

    fn merge_high(&mut self, base_a: usize, mut la: usize, base_b: usize, mut lb: usize) {
        let tmp: Vec<T> = self.a[base_b..base_b + lb].to_vec();
        let mut dest = (base_b + lb - 1) as isize;
        let mut ct = lb as isize - 1;
        let mut ca = (base_a + la - 1) as isize;
        let u = |i: isize| i as usize;
        self.a[u(dest)] = self.a[u(ca)].clone();
        dest -= 1;
        ca -= 1;
        let end = 'body: {
            la -= 1;
            if la == 0 {
                break 'body End::Succeed;
            }
            if lb == 1 {
                break 'body End::CopyRest;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                loop {
                    let (x, y) = (tmp[u(ct)].clone(), self.a[u(ca)].clone());
                    if self.cmp(&x, &y) < 0.0 {
                        self.a[u(dest)] = y;
                        dest -= 1;
                        ca -= 1;
                        wins_a += 1;
                        la -= 1;
                        wins_b = 0;
                        if la == 0 {
                            break 'body End::Succeed;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    } else {
                        self.a[u(dest)] = x;
                        dest -= 1;
                        ct -= 1;
                        wins_b += 1;
                        lb -= 1;
                        wins_a = 0;
                        if lb == 1 {
                            break 'body End::CopyRest;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first {
                    first = false;
                    min_gallop = min_gallop.saturating_sub(1).max(1);
                    self.min_gallop = min_gallop;
                    let key = tmp[u(ct)].clone();
                    let k = gallop_right(&mut self.cmp, &self.a, &key, base_a, la, la - 1);
                    wins_a = la - k;
                    if wins_a > 0 {
                        dest -= wins_a as isize;
                        ca -= wins_a as isize;
                        for k in (0..wins_a).rev() {
                            self.a[u(dest + 1) + k] = self.a[u(ca + 1) + k].clone();
                        }
                        la -= wins_a;
                        if la == 0 {
                            break 'body End::Succeed;
                        }
                    }
                    self.a[u(dest)] = tmp[u(ct)].clone();
                    dest -= 1;
                    ct -= 1;
                    lb -= 1;
                    if lb == 1 {
                        break 'body End::CopyRest;
                    }
                    let key = self.a[u(ca)].clone();
                    let k = gallop_left(&mut self.cmp, &tmp, &key, 0, lb, lb - 1);
                    wins_b = lb - k;
                    if wins_b > 0 {
                        dest -= wins_b as isize;
                        ct -= wins_b as isize;
                        for k in 0..wins_b {
                            self.a[u(dest + 1) + k] = tmp[u(ct + 1) + k].clone();
                        }
                        lb -= wins_b;
                        if lb == 1 {
                            break 'body End::CopyRest;
                        }
                        if lb == 0 {
                            break 'body End::Succeed;
                        }
                    }
                    self.a[u(dest)] = self.a[u(ca)].clone();
                    dest -= 1;
                    ca -= 1;
                    la -= 1;
                    if la == 0 {
                        break 'body End::Succeed;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        match end {
            End::Succeed => {
                if lb > 0 {
                    let start = u(dest - (lb as isize - 1));
                    self.a[start..start + lb].clone_from_slice(&tmp[..lb]);
                }
            }
            End::CopyRest => {
                // The first element of run B belongs at the front of the merge.
                dest -= la as isize;
                ca -= la as isize;
                for k in (0..la).rev() {
                    self.a[u(dest + 1) + k] = self.a[u(ca + 1) + k].clone();
                }
                self.a[u(dest)] = tmp[u(ct)].clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_stable_sort_for_consistent_comparators() {
        // Pseudo-random data with many duplicates, long enough to exercise
        // run detection, galloping and both merge directions.
        let mut rng = crate::js::Mulberry32::new(7);
        for len in [0usize, 1, 2, 5, 63, 64, 65, 200, 1000, 5000] {
            let data: Vec<(i32, usize)> = (0..len)
                .map(|i| ((rng.next_f64() * 50.0) as i32, i))
                .collect();
            let mut want = data.clone();
            want.sort_by_key(|p| p.0);
            let mut got = data.clone();
            sort_by(&mut got, |a, b| f64::from(a.0 - b.0));
            assert_eq!(got, want, "len {len}");
            // Partially ordered input (long runs, descending runs).
            let mut runs: Vec<(i32, usize)> = (0..len)
                .map(|i| {
                    (
                        if (i / 37) % 2 == 0 {
                            i as i32
                        } else {
                            -(i as i32)
                        },
                        i,
                    )
                })
                .collect();
            let mut want = runs.clone();
            want.sort_by_key(|p| p.0);
            sort_by(&mut runs, |a, b| f64::from(a.0 - b.0));
            assert_eq!(runs, want, "runs len {len}");
        }
    }
}
