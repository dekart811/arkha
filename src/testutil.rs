//! Helpers for the unit tests.

use std::path::{Path, PathBuf};

/// A folder for a test's files, removed when dropped. It's in target/, not
/// the system's temp folder, which may be a small RAM disk.
pub struct TestDir(pub PathBuf);

impl TestDir {
    pub fn new(name: &str) -> TestDir {
        TestDir::in_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("target"), name)
    }

    /// One in `base` instead: another file system (/dev/shm), say.
    pub fn in_dir(base: &Path, name: &str) -> TestDir {
        let p = base.join(format!("arkha-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TestDir(p)
    }

    /// Writes `rel` (creating its folders) and returns its path.
    pub fn file(&self, rel: &str, content: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
        p
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `f` on the one thread the tests use GTK on, and gives back what it
/// returns (or its panic). gtk-rs lets GTK be used only by the thread that
/// set it up, and the tests run on many: models and other objects that
/// need no display work there.
pub fn on_gtk<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    type Job = Box<dyn FnOnce() + Send>;
    static JOBS: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("gtk-tests".into())
            .spawn(move || {
                // Models need no display; GTK just has to count as set up.
                unsafe { gtk::set_initialized() };
                for job in rx {
                    job();
                }
            })
            .unwrap();
        tx
    });
    let (done, result) = std::sync::mpsc::channel();
    jobs.send(Box::new(move || {
        let _ = done.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    }))
    .unwrap();
    match result.recv().unwrap() {
        Ok(r) => r,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// A small random generator for the randomized tests: seeded, so a failure
/// comes back the same on every run (the seed and the input are in the
/// message).
pub struct Rng(u64);

/// Characters that mean something to some parser, and some that are hard
/// to handle (multibyte, combining, zero-width, right-to-left).
const TRICKY: &[char] = &[
    '*', '?', '[', ']', '.', '_', '-', ';', ',', '=', ' ', '\t', '\n', '\r', '\\', '%', '$', '~', '/', ':', '"', '\'', '#', '(', ')', '0', '1', '9',
    'a', 'B', 'z', 'ä', 'ß', 'İ', 'ﬁ', 'é', '\u{301}', '\u{200b}', '\u{202e}', '日', '😀', '\0',
];

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    pub fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }

    /// Up to `max` characters, mostly tricky ones.
    pub fn text(&mut self, max: usize) -> String {
        let n = self.below(max + 1);
        (0..n)
            .map(|_| if self.chance(4) { char::from_u32(self.int(0x20, 0x2fff) as u32).unwrap_or('x') } else { *self.pick(TRICKY) })
            .collect()
    }

    /// Like `text`, from `chars` only.
    pub fn text_of(&mut self, chars: &[char], max: usize) -> String {
        let n = self.below(max + 1);
        (0..n).map(|_| *self.pick(chars)).collect()
    }
}

/// Runs `f` for `n` random cases, each with its own generator; a panic
/// names the case's seed, to run it again alone. ARKHA_RANDOM_CASES=k runs
/// k times as many (a longer stability run).
pub fn cases(n: u64, f: impl Fn(&mut Rng)) {
    let factor: u64 = std::env::var("ARKHA_RANDOM_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    for seed in 1..=n * factor.max(1) {
        let mut r = Rng::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut r))).is_err() {
            panic!("random case {seed} failed (see the message above)");
        }
    }
}
