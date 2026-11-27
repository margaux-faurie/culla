//! Prefetch and in-memory preview cache policy (ADR 0012, US-08): pure decisions, no I/O,
//! no threads, no clock.
//!
//! The adapters' preview pipeline (`culla_adapters::pipeline`) asks these types what to do;
//! they hold no pixels:
//!
//! * [`wanted`]: which positions of the swipe order should be ready, in priority order, for the
//!   current position and direction of travel ([`Window`]: 5 ahead and 2 behind, SPEC §5.1).
//! * [`Scheduler`]: which wanted photo to read next (not cached, not in flight, not failed),
//!   and which in-flight work went stale when the window moved (cancellation).
//! * [`Lru`]: the in-memory cache's bookkeeping, capped in entries and in bytes
//!   ([`Capacity`]), with the eviction order: least recently used outside the window first,
//!   then the window's own entries from its lowest priority up. A preview that would be the
//!   first to go is not admitted.
//! * [`Budget`]: a byte counter that bounds the memory in flight (files read, decode buffers),
//!   so the pipeline's peak stays under NFR-PERF-09 whatever the photos ([`Budgets`]).

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Which way the user is moving through the swipe order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Towards later positions (→, or a decision, which advances).
    #[default]
    Forward,
    /// Towards earlier positions (Shift+←).
    Backward,
}

/// How many photos around the current one to have ready, counted in the direction of travel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Window {
    /// Photos after the current one, in the direction of travel.
    pub ahead: usize,
    /// Photos before it.
    pub behind: usize,
}

impl Window {
    /// SPEC §5.1: 5 ahead and 2 behind (a tuning knob of B-09 and B-10).
    pub const DEFAULT: Self = Self {
        ahead: 5,
        behind: 2,
    };
}

impl Default for Window {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The positions (indices into a swipe order of `len` photos) to have ready when the user is
/// at `position` moving in `direction`, highest priority first: the current photo, then the
/// photos ahead nearest first, then those behind nearest first. Positions past either end are
/// left out; an empty list when `position` is not below `len`.
///
/// "Ahead" follows the direction of travel, so moving backward prefetches earlier photos.
#[must_use]
pub fn wanted(position: usize, len: usize, direction: Direction, window: Window) -> Vec<usize> {
    if position >= len {
        return Vec::new();
    }
    // Position `k` steps away, towards later positions or earlier ones.
    let step = |later: bool, k: usize| {
        if later {
            position.checked_add(k).filter(|&p| p < len)
        } else {
            position.checked_sub(k)
        }
    };
    let forward = direction == Direction::Forward;
    let mut out = Vec::with_capacity(1 + window.ahead.min(len) + window.behind.min(len));
    out.push(position);
    out.extend((1..=window.ahead).map_while(|k| step(forward, k)));
    out.extend((1..=window.behind).map_while(|k| step(!forward, k)));
    out
}

/// What happened to a photo handed out by [`Scheduler::next`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Its preview is in the cache.
    Cached,
    /// Its preview could not be made: not handed out again until [`Scheduler::retry`].
    Failed,
    /// It left the window before it was done; handed out again if it comes back.
    Cancelled,
    /// Made, but the cache did not admit it ([`Admission::Refused`]): not handed out again
    /// until the window changes.
    NotAdmitted,
}

/// Which photo to prepare next.
///
/// Holds the wanted list (from [`wanted`] or any priority list), the photos being worked on
/// and those that failed. The cache is asked through the `cached` argument of [`Self::next`],
/// so the two never disagree.
#[derive(Clone, Debug)]
pub struct Scheduler<K> {
    wanted: Vec<K>,
    rank: HashMap<K, usize>,
    in_flight: HashSet<K>,
    failed: HashSet<K>,
    not_admitted: HashSet<K>,
}

impl<K: Copy + Eq + Hash> Default for Scheduler<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Eq + Hash> Scheduler<K> {
    /// Nothing wanted.
    #[must_use]
    pub fn new() -> Self {
        Self {
            wanted: Vec::new(),
            rank: HashMap::new(),
            in_flight: HashSet::new(),
            failed: HashSet::new(),
            not_admitted: HashSet::new(),
        }
    }

    /// Replaces the wanted list (highest priority first; a repeated key keeps its first place)
    /// and returns the photos in flight that are no longer wanted, so their work can stop.
    /// They stay in flight until [`Self::finish`].
    pub fn set_wanted(&mut self, wanted: impl IntoIterator<Item = K>) -> Vec<K> {
        self.wanted.clear();
        self.rank.clear();
        for k in wanted {
            if !self.rank.contains_key(&k) {
                self.rank.insert(k, self.wanted.len());
                self.wanted.push(k);
            }
        }
        self.not_admitted.clear();
        self.in_flight
            .iter()
            .copied()
            .filter(|k| !self.rank.contains_key(k))
            .collect()
    }

    /// The wanted list, highest priority first.
    #[must_use]
    pub fn wanted(&self) -> &[K] {
        &self.wanted
    }

    /// The place of `k` in the wanted list (0 is the highest priority), or `None`.
    #[must_use]
    pub fn priority(&self, k: &K) -> Option<usize> {
        self.rank.get(k).copied()
    }

    /// Whether `k` is in the wanted list.
    #[must_use]
    pub fn is_wanted(&self, k: &K) -> bool {
        self.rank.contains_key(k)
    }

    /// Whether `k` is being worked on.
    #[must_use]
    pub fn in_flight(&self, k: &K) -> bool {
        self.in_flight.contains(k)
    }

    /// Whether `k` failed (and is not retried).
    #[must_use]
    pub fn failed(&self, k: &K) -> bool {
        self.failed.contains(k)
    }

    /// Whether the cache refused `k`'s preview since the window last changed.
    #[must_use]
    pub fn not_admitted(&self, k: &K) -> bool {
        self.not_admitted.contains(k)
    }

    /// The highest-priority wanted photo that is not `cached`, not in flight, not failed and
    /// not refused by the cache since the window last changed. It is marked in flight.
    pub fn next(&mut self, cached: impl Fn(&K) -> bool) -> Option<K> {
        let k = self.wanted.iter().copied().find(|k| {
            !self.in_flight.contains(k)
                && !self.failed.contains(k)
                && !self.not_admitted.contains(k)
                && !cached(k)
        })?;
        self.in_flight.insert(k);
        Some(k)
    }

    /// Records how the work on `k` ended. Returns whether `k` is still wanted.
    pub fn finish(&mut self, k: K, outcome: Outcome) -> bool {
        self.in_flight.remove(&k);
        match outcome {
            Outcome::Failed => {
                self.failed.insert(k);
            }
            Outcome::NotAdmitted => {
                self.not_admitted.insert(k);
            }
            Outcome::Cached | Outcome::Cancelled => {}
        }
        self.is_wanted(&k)
    }

    /// Lets a failed photo be handed out again. Returns whether it had failed.
    pub fn retry(&mut self, k: &K) -> bool {
        self.failed.remove(k)
    }
}

/// How much the in-memory cache holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Capacity {
    /// Most previews held.
    pub max_entries: usize,
    /// Most bytes held (the encoded JPEGs).
    pub max_bytes: u64,
}

impl Capacity {
    /// SPEC §5.1: about 12 screen previews. A 2700x1800 preview at q88 is about 0.5–1.5 MB
    /// (ADR 0012), so 48 MiB leaves room for detailed photos; provisional until B-09 and B-10.
    pub const DEFAULT: Self = Self {
        max_entries: 12,
        max_bytes: 48 * 1024 * 1024,
    };
}

impl Default for Capacity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why [`Lru::insert`] did not admit an entry. Nothing changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The entry alone is larger than [`Capacity::max_bytes`], or the capacity holds no entry.
    TooLarge,
    /// Making room would evict the entry itself first: everything else held ranks higher.
    LowestRank,
}

/// The result of [`Lru::insert`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admission<K> {
    /// Held now; these keys were evicted to make room, in eviction order.
    Admitted { evicted: Vec<K> },
    /// Not held; nothing was evicted.
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    bytes: u64,
    used: u64,
}

/// Bookkeeping of a least-recently-used cache capped in entries and in bytes. It holds keys
/// and sizes only; the values live with the caller, who removes those [`Self::insert`]
/// evicts.
///
/// Eviction order, when an insert needs room: entries outside `pinned` (the wanted list)
/// least recently used first; then entries in `pinned` from the lowest priority (its end) up.
/// The entry being inserted takes part with its own rank (most recently used, or its place in
/// `pinned`): if it would be evicted first it is refused instead and nothing changes.
#[derive(Clone, Debug)]
pub struct Lru<K> {
    capacity: Capacity,
    slots: HashMap<K, Slot>,
    bytes: u64,
    clock: u64,
}

impl<K: Copy + Eq + Hash> Lru<K> {
    /// An empty cache.
    #[must_use]
    pub fn new(capacity: Capacity) -> Self {
        Self {
            capacity,
            slots: HashMap::new(),
            bytes: 0,
            clock: 0,
        }
    }

    /// The capacity.
    #[must_use]
    pub fn capacity(&self) -> Capacity {
        self.capacity
    }

    /// Entries held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Bytes held.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Whether `k` is held.
    #[must_use]
    pub fn contains(&self, k: &K) -> bool {
        self.slots.contains_key(k)
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Marks `k` as just used. Returns whether it is held.
    pub fn touch(&mut self, k: &K) -> bool {
        let t = self.tick();
        match self.slots.get_mut(k) {
            Some(s) => {
                s.used = t;
                true
            }
            None => false,
        }
    }

    /// Stops holding `k`. Returns whether it was held.
    pub fn remove(&mut self, k: &K) -> bool {
        match self.slots.remove(k) {
            Some(s) => {
                self.bytes -= s.bytes;
                true
            }
            None => false,
        }
    }

    /// Holds `k` (`bytes` long, just used), evicting as the type docs say. `pinned` is the
    /// wanted list, highest priority first. Inserting a key already held replaces its size.
    pub fn insert(&mut self, k: K, bytes: u64, pinned: &[K]) -> Admission<K> {
        let cap = self.capacity;
        if bytes > cap.max_bytes || cap.max_entries == 0 {
            return Admission::Refused(Refusal::TooLarge);
        }
        let old = self.slots.get(&k).map_or(0, |s| s.bytes);
        let mut entries = self.slots.len() + usize::from(!self.slots.contains_key(&k));
        let mut total = self.bytes - old + bytes;
        if entries <= cap.max_entries && total <= cap.max_bytes {
            let used = self.tick();
            self.slots.insert(k, Slot { bytes, used });
            self.bytes = total;
            return Admission::Admitted {
                evicted: Vec::new(),
            };
        }
        let order = self.eviction_order(k, pinned);
        let mut evicted = Vec::new();
        for victim in order {
            if entries <= cap.max_entries && total <= cap.max_bytes {
                break;
            }
            if victim == k {
                return Admission::Refused(Refusal::LowestRank);
            }
            entries -= 1;
            total -= self.slots.get(&victim).map_or(0, |s| s.bytes);
            evicted.push(victim);
        }
        for v in &evicted {
            self.remove(v);
        }
        let used = self.tick();
        let old = self
            .slots
            .insert(k, Slot { bytes, used })
            .map_or(0, |s| s.bytes);
        self.bytes = self.bytes - old + bytes;
        Admission::Admitted { evicted }
    }

    /// Every held key and `new`, first to be evicted first.
    fn eviction_order(&self, new: K, pinned: &[K]) -> Vec<K> {
        self.order(Some(new), pinned)
    }

    /// Held keys, first to be evicted first, for `pinned` (the wanted list).
    #[must_use]
    pub fn victims(&self, pinned: &[K]) -> Vec<K> {
        self.order(None, pinned)
    }

    /// Held keys and `new` (held or not, counted as the most recently used), first to be
    /// evicted first.
    fn order(&self, new: Option<K>, pinned: &[K]) -> Vec<K> {
        let mut rank: HashMap<K, usize> = HashMap::new();
        for (i, k) in pinned.iter().enumerate() {
            rank.entry(*k).or_insert(i);
        }
        let mut free: Vec<(u64, K)> = Vec::new();
        let mut held: Vec<(usize, K)> = Vec::new();
        let used = |k: &K| {
            if Some(*k) == new {
                u64::MAX
            } else {
                self.slots.get(k).map_or(u64::MAX, |s| s.used)
            }
        };
        let extra = new.filter(|k| !self.slots.contains_key(k));
        for key in self.slots.keys().copied().chain(extra) {
            match rank.get(&key) {
                Some(&r) => held.push((r, key)),
                None => free.push((used(&key), key)),
            }
        }
        free.sort_by_key(|&(used, _)| used);
        held.sort_by_key(|&(r, _)| std::cmp::Reverse(r));
        free.into_iter()
            .map(|(_, k)| k)
            .chain(held.into_iter().map(|(_, k)| k))
            .collect()
    }
}

/// The result of [`Budget::reserve`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reservation {
    /// The bytes are reserved; give them back with [`Budget::release`].
    Granted,
    /// Not now: wait for a release and ask again.
    Wait,
    /// Never: more than the whole budget.
    Never,
}

/// A byte counter with a hard limit, for memory in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    limit: u64,
    used: u64,
}

impl Budget {
    /// Nothing reserved out of `limit` bytes.
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    /// The limit.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Bytes reserved now.
    #[must_use]
    pub const fn used(&self) -> u64 {
        self.used
    }

    /// Reserves `bytes` if they fit next to what is reserved.
    pub fn reserve(&mut self, bytes: u64) -> Reservation {
        if bytes > self.limit {
            return Reservation::Never;
        }
        if bytes > self.limit - self.used {
            return Reservation::Wait;
        }
        self.used += bytes;
        Reservation::Granted
    }

    /// Gives back `bytes` reserved before. Saturates: giving back more than is reserved
    /// leaves nothing reserved.
    pub fn release(&mut self, bytes: u64) {
        self.used = self.used.saturating_sub(bytes);
    }
}

/// The pipeline's memory limits, each a hard cap (NFR-PERF-09: 1 GB must, 600 MB should, for
/// the Rust process and WebView2 together).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    /// Source bytes read and not yet decoded or still being decoded (camera JPEGs, a lone
    /// NEF's embedded JPEG). At least one file at the input cap (64 MiB) must fit.
    pub input: u64,
    /// Decode, resize, orient and encode buffers of the calls in progress, and a multi-scan
    /// JPEG's coefficients ([`crate::preview::Plan::working_bytes`]).
    pub working: u64,
}

impl Budgets {
    /// Provisional defaults (SPEC §10): 128 MiB of input (two files at the 64 MiB cap, about
    /// five 45 MP camera JPEGs) and 256 MiB of working memory (four 24 or 45 MP calls at about
    /// 58 to 59 MB each, or one 25 MP multi-scan call with its coefficients next to a 24 MP
    /// one). Hard caps on what the pipeline reserves: what each call really allocates is
    /// estimated ([`crate::preview::Plan::working_bytes`]).
    pub const DEFAULT: Self = Self {
        input: 128 * 1024 * 1024,
        working: 256 * 1024 * 1024,
    };

    /// The most the pipeline holds at once with a cache of `cache` bytes: the two budgets and
    /// the cache. Per-thread stacks and allocator slack come on top.
    #[must_use]
    pub const fn ceiling(&self, cache: Capacity) -> u64 {
        self.input + self.working + cache.max_bytes
    }
}

impl Default for Budgets {
    fn default() -> Self {
        Self::DEFAULT
    }
}
