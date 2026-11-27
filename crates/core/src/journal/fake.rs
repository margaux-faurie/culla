//! A fake disk and journal store for the executor tests, modelling durability (file-engine §1):
//! every change is unflushed until a flush covers it, and a simulated crash may revert any
//! unflushed change. Partial writes are flushed at the end of `copy` (flush 1) and their rename
//! at the end of `commit` (flush 2); user-file renames, source removals and partial removals
//! are never flushed while the plan is open.
//!
//! The fake also checks, on every call, the rules the core must keep: a source is removed only
//! with a proof made in this process run, after every member of its group is committed and none
//! failed (INV-01, INV-12); a partial is removed only if the journal recorded it and the source
//! is there (INV-01); every changed path belongs to the operation (S, F or one of its partials);
//! and, after every call, each planned file is durably in S, in F, or in a partial the journal
//! holds at `verifying` or later with its H1 (INV-11).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::{
    At, Copied, Disk, DiskError, Entry, FileSeen, Hash, Journal, Kind, Op, OpId, OpState, Seen,
    Store, StoreError,
};
use crate::plan::Root;
use crate::preflight::TimeRule;

/// A file: what it holds (a fingerprint standing for its bytes), its size and mtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileRec {
    pub(super) content: u64,
    pub(super) size: u64,
    pub(super) mtime: Option<i64>,
}

pub(super) fn hash_of(content: u64) -> Hash {
    let mut h = [0u8; 32];
    h[..8].copy_from_slice(&content.to_le_bytes());
    h[8] = 0xC1;
    h
}

/// The path of a file: tree prefix and components.
pub(super) fn src_key(op: &Op) -> String {
    format!("S:{}", op.src.join("\\"))
}

pub(super) fn dst_key_of(root: Root, parts: &[String]) -> String {
    let t = match root {
        Root::Dest => "D",
        Root::Rejected => "R",
    };
    format!("{t}:{}", parts.join("\\"))
}

pub(super) fn dst_key(op: &Op) -> String {
    dst_key_of(op.root, &op.dst)
}

/// How a crash lands on the call where it fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Normal,
    /// Before the call does anything.
    Before,
    /// Half-way: the effect is there but not flushed (a copy may be torn).
    Mid,
    /// After the effect and its flush, before the caller hears back.
    After,
}

struct Change {
    paths: Vec<(String, Option<FileRec>, Option<FileRec>)>,
}

/// xorshift64*.
pub(super) struct Rng(pub(super) u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(super) fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    pub(super) fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

pub(super) struct World {
    pub(super) files: BTreeMap<String, FileRec>,
    unflushed: Vec<Change>,
    /// The journal as the store holds it.
    pub(super) journal: Journal,
    pub(super) entries: Vec<Entry>,
    /// Ops whose `VerifiedMove` this process run holds.
    proofs: BTreeSet<OpId>,
    pub(super) calls: u64,
    pub(super) crash_at: Option<u64>,
    pub(super) crashed: bool,
    pub(super) crashes: u64,
    pub(super) rng: Rng,
    /// Mutating disk calls and appends.
    pub(super) mutations: u64,
    pub(super) appends: u64,
    /// Each op's own content.
    pub(super) content: BTreeMap<OpId, u64>,
    /// Ops whose source is held by another process for good.
    pub(super) locked: BTreeSet<OpId>,
    /// Ops whose next copy reads wrong bytes once.
    pub(super) flaky: BTreeSet<OpId>,
    /// Ops whose target is taken by another process just before their rename or commit.
    pub(super) foreign_late: BTreeSet<OpId>,
    /// Ops whose next copy finds the disk full.
    pub(super) full: BTreeSet<OpId>,
    /// Ops whose next step finds the volume gone (a card pull, no crash).
    pub(super) pull: BTreeSet<(OpId, &'static str)>,
    /// Check INV-11 after every call.
    pub(super) check_inv11: bool,
    /// Files that are not ours, which nothing may change.
    pub(super) foreign: BTreeMap<String, FileRec>,
    /// Calls in order, for tests that look at the sequence.
    pub(super) log: Vec<String>,
}

pub(super) type Shared = Rc<RefCell<World>>;

impl World {
    pub(super) fn new(journal: Journal, seed: u64) -> Shared {
        let mut w = World {
            files: BTreeMap::new(),
            unflushed: Vec::new(),
            journal,
            entries: Vec::new(),
            proofs: BTreeSet::new(),
            calls: 0,
            crash_at: None,
            crashed: false,
            crashes: 0,
            rng: Rng::new(seed),
            mutations: 0,
            appends: 0,
            content: BTreeMap::new(),
            locked: BTreeSet::new(),
            flaky: BTreeSet::new(),
            foreign_late: BTreeSet::new(),
            full: BTreeSet::new(),
            pull: BTreeSet::new(),
            check_inv11: true,
            foreign: BTreeMap::new(),
            log: Vec::new(),
        };
        for op in w.journal.ops().to_vec() {
            let c = 10_000 + u64::try_from(op.id).unwrap_or(0);
            w.content.insert(op.id, c);
            w.files.insert(
                src_key(&op),
                FileRec {
                    content: c,
                    size: op.expected_size,
                    mtime: op.src_mtime_ns,
                },
            );
        }
        Rc::new(RefCell::new(w))
    }

    /// A file that is not ours, durable.
    pub(super) fn put_foreign(&mut self, key: String, content: u64) {
        let r = FileRec {
            content,
            size: 7,
            mtime: Some(1),
        };
        self.files.insert(key.clone(), r);
        self.foreign.insert(key, r);
    }

    /// A journal rebuilt from the stored entries, as a restarted process reads it.
    pub(super) fn reload(&self) -> Journal {
        let mut j = Journal::new(self.journal.ops().to_vec()).unwrap();
        j.replay(&self.entries).unwrap();
        j
    }

    fn change(&mut self, paths: Vec<(String, Option<FileRec>)>, flushed: bool) {
        let mut c = Change { paths: Vec::new() };
        for (k, after) in paths {
            let before = self.files.get(&k).copied();
            match after {
                Some(r) => self.files.insert(k.clone(), r),
                None => self.files.remove(&k),
            };
            c.paths.push((k, before, after));
        }
        if !flushed {
            self.unflushed.push(c);
        }
    }

    /// Everything unflushed is now durable (the plan closed and the file system wrote back).
    pub(super) fn flush_all(&mut self) {
        self.unflushed.clear();
    }

    /// The process dies: each unflushed change may be reverted, newest first; proofs are gone.
    pub(super) fn crash(&mut self) {
        self.crashed = true;
        self.crashes += 1;
        self.proofs.clear();
        let changes = std::mem::take(&mut self.unflushed);
        let mut kept = Vec::new();
        for c in changes.into_iter().rev() {
            let applies = c
                .paths
                .iter()
                .all(|(k, _, after)| self.files.get(k).copied() == *after);
            if applies && self.rng.chance(50) {
                for (k, before, _) in &c.paths {
                    match before {
                        Some(r) => self.files.insert(k.clone(), *r),
                        None => self.files.remove(k),
                    };
                }
            } else {
                kept.push(c);
            }
        }
        kept.reverse();
        self.unflushed = kept;
    }

    /// The files as they would be if every unflushed change were lost.
    fn durable(&self) -> BTreeMap<String, FileRec> {
        let mut d = self.files.clone();
        for c in self.unflushed.iter().rev() {
            for (k, before, _) in &c.paths {
                match before {
                    Some(r) => d.insert(k.clone(), *r),
                    None => d.remove(k),
                };
            }
        }
        d
    }

    /// INV-11 on the durable view.
    fn check_durable(&self) {
        for (k, r) in &self.foreign {
            assert_eq!(self.files.get(k), Some(r), "a foreign file changed: {k}");
        }
        if !self.check_inv11 {
            return;
        }
        let d = self.durable();
        for (i, op) in self.journal.ops().iter().enumerate() {
            let c = self.content[&op.id];
            let p = self.journal.progress(i);
            let at = |k: &str| d.get(k).is_some_and(|r| r.content == c);
            let in_partial = matches!(
                p.state(),
                OpState::Verifying | OpState::Committed | OpState::SourceRemoved
            ) && p.hash == Some(hash_of(c))
                && p.attempts > 0
                && at(&dst_key_of(op.root, &op.partial(p.attempts)));
            assert!(
                at(&src_key(op)) || at(&dst_key(op)) || in_partial,
                "INV-11: op {} ({:?}, {}) has no durable copy",
                op.id,
                op.kind,
                p.state()
            );
        }
    }

    /// One port call: counts it and decides whether the crash fires here.
    fn enter(&mut self) -> Result<Mode, ()> {
        if self.crashed {
            return Err(());
        }
        self.calls += 1;
        if self.crash_at == Some(self.calls) {
            self.crash_at = None;
            return Ok(match self.rng.below(3) {
                0 => Mode::Before,
                1 => Mode::Mid,
                _ => Mode::After,
            });
        }
        Ok(Mode::Normal)
    }

    fn owned(&self, op: &Op, key: &str) {
        let i = self
            .journal
            .ops()
            .iter()
            .position(|o| o.id == op.id)
            .unwrap();
        let mut ok = vec![src_key(op), dst_key(op)];
        // The attempt about to start is recorded before its copy runs.
        for a in 1..=self.journal.progress(i).attempts {
            ok.push(dst_key_of(op.root, &op.partial(a)));
        }
        assert!(ok.iter().any(|k| k == key), "op {} touched {key}", op.id);
    }

    fn group_ok_for_removal(&self, op: &Op) {
        for (i, o) in self.journal.ops().iter().enumerate() {
            if o.group == op.group {
                let s = self.journal.progress(i).state();
                assert!(
                    matches!(
                        s,
                        OpState::Committed | OpState::SourceRemoved | OpState::Done
                    ),
                    "INV-12: op {} removed while op {} is {s}",
                    op.id,
                    o.id
                );
            }
        }
    }
}

pub(super) struct FakeDisk(pub(super) Shared);
pub(super) struct FakeStore(pub(super) Shared);

fn seen_of(r: Option<&FileRec>) -> Option<FileSeen> {
    r.map(|r| FileSeen {
        file: true,
        size: r.size,
        mtime_ns: r.mtime,
    })
}

impl FakeDisk {
    /// Runs `f` under the crash model: `f(world, mid)` does the effect, flushing it unless
    /// `mid`; a crash before skips it, a crash after keeps it.
    fn step<T>(
        &mut self,
        name: &str,
        op: &Op,
        f: impl FnOnce(&mut World, bool) -> Result<T, DiskError>,
    ) -> Result<T, DiskError> {
        let mut w = self.0.borrow_mut();
        let Ok(mode) = w.enter() else {
            return Err(DiskError::VolumeLost);
        };
        w.log.push(format!("{name} {}", op.id));
        let key = w
            .pull
            .iter()
            .find(|(id, n)| *id == op.id && *n == name)
            .copied();
        if let Some(k) = key {
            w.pull.remove(&k);
            return Err(DiskError::VolumeLost);
        }
        let out = match mode {
            Mode::Normal => {
                let r = f(&mut w, false);
                w.check_durable();
                return r;
            }
            Mode::Before => Err(DiskError::VolumeLost),
            Mode::Mid => {
                let _ = f(&mut w, true);
                Err(DiskError::VolumeLost)
            }
            Mode::After => {
                let _ = f(&mut w, false);
                Err(DiskError::VolumeLost)
            }
        };
        w.check_durable();
        w.crash();
        out
    }
}

impl Disk for FakeDisk {
    fn probe(&mut self, op: &Op, partials: &[Vec<String>]) -> Result<Seen, DiskError> {
        self.step("probe", op, |w, _| {
            Ok(Seen {
                rule: TimeRule::Exact,
                source: seen_of(w.files.get(&src_key(op))),
                target: seen_of(w.files.get(&dst_key(op))),
                partials: partials
                    .iter()
                    .map(|p| w.files.contains_key(&dst_key_of(op.root, p)))
                    .collect(),
            })
        })
    }

    fn matches(&mut self, op: &Op, at: At<'_>, hash: &Hash) -> Result<bool, DiskError> {
        self.step("matches", op, |w, _| {
            let k = match at {
                At::Partial(p) => dst_key_of(op.root, p),
                At::Target => dst_key(op),
            };
            Ok(w.files.get(&k).is_some_and(|r| hash_of(r.content) == *hash))
        })
    }

    fn rename(&mut self, op: &Op) -> Result<(), DiskError> {
        self.step("rename", op, |w, _| {
            assert_eq!(op.kind, Kind::Rename);
            if w.locked.contains(&op.id) {
                return Err(DiskError::Locked);
            }
            if w.foreign_late.remove(&op.id) && !w.files.contains_key(&dst_key(op)) {
                w.put_foreign(dst_key(op), 7);
            }
            if w.files.contains_key(&dst_key(op)) {
                return Err(DiskError::Exists);
            }
            let Some(s) = w.files.get(&src_key(op)).copied() else {
                return Err(DiskError::Io(2));
            };
            w.owned(op, &src_key(op));
            w.mutations += 1;
            w.change(vec![(src_key(op), None), (dst_key(op), Some(s))], false);
            Ok(())
        })
    }

    fn copy(&mut self, op: &Op, partial: &[String]) -> Result<Copied, DiskError> {
        self.step("copy", op, |w, mid| {
            assert!(op.kind.has_partial());
            if w.locked.contains(&op.id) {
                return Err(DiskError::Locked);
            }
            let pk = dst_key_of(op.root, partial);
            if w.files.contains_key(&pk) {
                return Err(DiskError::Exists);
            }
            let Some(s) = w.files.get(&src_key(op)).copied() else {
                return Err(DiskError::Io(2));
            };
            w.owned(op, &pk);
            w.mutations += 1;
            if w.full.remove(&op.id) {
                // Half a partial, then the disk is full.
                let torn = FileRec {
                    content: s.content ^ 0xDEAD,
                    ..s
                };
                w.change(vec![(pk, Some(torn))], false);
                return Err(DiskError::DiskFull);
            }
            let mut content = s.content;
            if w.flaky.remove(&op.id) {
                content ^= 0xBAD;
            }
            if mid && w.rng.chance(50) {
                content ^= 0xDEAD;
            }
            let r = FileRec { content, ..s };
            w.change(vec![(pk, Some(r))], mid);
            Ok(Copied {
                hash: hash_of(content),
                size: s.size,
            })
        })
    }

    fn verify(&mut self, op: &Op, partial: &[String], hash: &Hash) -> Result<bool, DiskError> {
        self.step("verify", op, |w, _| {
            let p = w.files.get(&dst_key_of(op.root, partial)).copied();
            let s = w.files.get(&src_key(op)).copied();
            let ok = p.is_some_and(|r| hash_of(r.content) == *hash)
                && s.is_some_and(|r| hash_of(r.content) == *hash);
            if ok && op.kind == Kind::XMove {
                w.proofs.insert(op.id);
            }
            Ok(ok)
        })
    }

    fn commit(&mut self, op: &Op, partial: &[String]) -> Result<(), DiskError> {
        self.step("commit", op, |w, mid| {
            if w.foreign_late.remove(&op.id) && !w.files.contains_key(&dst_key(op)) {
                w.put_foreign(dst_key(op), 7);
            }
            if w.files.contains_key(&dst_key(op)) {
                return Err(DiskError::Exists);
            }
            let pk = dst_key_of(op.root, partial);
            let Some(p) = w.files.get(&pk).copied() else {
                return Err(DiskError::Io(2));
            };
            w.owned(op, &pk);
            w.mutations += 1;
            w.change(vec![(pk, None), (dst_key(op), Some(p))], !mid);
            Ok(())
        })
    }

    fn prove(&mut self, op: &Op, hash: &Hash) -> Result<bool, DiskError> {
        self.step("prove", op, |w, _| {
            assert!(op.kind.removes_source());
            let f = w.files.get(&dst_key(op)).copied();
            let s = w.files.get(&src_key(op)).copied();
            let ok = f.is_some_and(|r| hash_of(r.content) == *hash && r.size == op.expected_size)
                && s.is_some_and(|r| hash_of(r.content) == *hash);
            if ok {
                w.proofs.insert(op.id);
            }
            Ok(ok)
        })
    }

    fn remove_source(&mut self, op: &Op) -> Result<(), DiskError> {
        self.step("remove_source", op, |w, _| {
            assert!(
                w.proofs.remove(&op.id),
                "INV-01: op {} removed without a proof",
                op.id
            );
            let c = w.content[&op.id];
            assert_eq!(w.files.get(&src_key(op)).map(|r| r.content), Some(c));
            assert_eq!(w.files.get(&dst_key(op)).map(|r| r.content), Some(c));
            w.group_ok_for_removal(op);
            w.owned(op, &src_key(op));
            w.mutations += 1;
            w.change(vec![(src_key(op), None)], false);
            Ok(())
        })
    }

    fn remove_partial(&mut self, op: &Op, partial: &[String]) -> Result<(), DiskError> {
        self.step("remove_partial", op, |w, _| {
            let pk = dst_key_of(op.root, partial);
            w.owned(op, &pk);
            assert_ne!(pk, dst_key(op));
            let c = w.content[&op.id];
            assert_eq!(
                w.files.get(&src_key(op)).map(|r| r.content),
                Some(c),
                "INV-01: partial of op {} removed without its source",
                op.id
            );
            if w.files.contains_key(&pk) {
                w.mutations += 1;
                w.change(vec![(pk, None)], false);
            }
            Ok(())
        })
    }

    fn flush_target(&mut self, op: &Op) -> Result<(), DiskError> {
        self.step("flush_target", op, |w, _| {
            let k = dst_key(op);
            w.unflushed
                .retain(|c| !c.paths.iter().any(|(p, _, _)| *p == k));
            Ok(())
        })
    }

    fn release(&mut self, op: &Op) {
        self.0.borrow_mut().proofs.remove(&op.id);
    }
}

impl Store for FakeStore {
    fn append(&mut self, entries: &[Entry]) -> Result<(), StoreError> {
        let mut w = self.0.borrow_mut();
        let Ok(mode) = w.enter() else {
            return Err(StoreError);
        };
        let persist = |w: &mut World| {
            for e in entries {
                w.journal
                    .apply(e)
                    .expect("the store only gets legal entries");
                w.entries.push(e.clone());
            }
            w.appends += 1;
        };
        match mode {
            Mode::Normal => {
                persist(&mut w);
                w.check_durable();
                Ok(())
            }
            Mode::Before => {
                w.crash();
                Err(StoreError)
            }
            Mode::Mid | Mode::After => {
                persist(&mut w);
                w.check_durable();
                w.crash();
                Err(StoreError)
            }
        }
    }
}
