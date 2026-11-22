//! Checksums of a file's contents: for the Checksums tab of Properties, and
//! for checked copies (ops.rs), which compare a copy's CRC32 with its
//! original's.

use gio::prelude::*;

/// A checksum Arkha can work out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algo {
    Md5,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
    Crc32,
    Blake2b,
}

/// Every checksum, in the order they're offered until arranged otherwise.
pub const ALL: [Algo; 7] = [Algo::Crc32, Algo::Md5, Algo::Sha1, Algo::Sha256, Algo::Sha384, Algo::Sha512, Algo::Blake2b];

/// Those not offered until turned on in Preferences.
pub const HIDDEN_AT_FIRST: [&str; 3] = ["sha384", "sha512", "blake2b"];

impl Algo {
    /// Its name in the settings.
    pub fn id(self) -> &'static str {
        match self {
            Algo::Md5 => "md5",
            Algo::Sha1 => "sha1",
            Algo::Sha256 => "sha256",
            Algo::Sha384 => "sha384",
            Algo::Sha512 => "sha512",
            Algo::Crc32 => "crc32",
            Algo::Blake2b => "blake2b",
        }
    }

    /// Its name as people know it.
    pub fn name(self) -> &'static str {
        match self {
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA-1",
            Algo::Sha256 => "SHA-256",
            Algo::Sha384 => "SHA-384",
            Algo::Sha512 => "SHA-512",
            Algo::Crc32 => "CRC32",
            Algo::Blake2b => "BLAKE2b",
        }
    }

    pub fn from_id(id: &str) -> Option<Algo> {
        ALL.into_iter().find(|a| a.id() == id)
    }

    fn hasher(self) -> Hasher {
        let glib = |t| Hasher::Glib(glib::Checksum::new(t).expect("GLib knows its own checksum types"));
        match self {
            Algo::Md5 => glib(glib::ChecksumType::Md5),
            Algo::Sha1 => glib(glib::ChecksumType::Sha1),
            Algo::Sha256 => glib(glib::ChecksumType::Sha256),
            Algo::Sha384 => glib(glib::ChecksumType::Sha384),
            Algo::Sha512 => glib(glib::ChecksumType::Sha512),
            Algo::Crc32 => Hasher::Crc32(crc32fast::Hasher::new()),
            Algo::Blake2b => Hasher::Blake2b(Box::default()),
        }
    }
}

enum Hasher {
    Glib(glib::Checksum),
    Crc32(crc32fast::Hasher),
    Blake2b(Box<blake2::Blake2b512>),
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Glib(c) => c.update(data),
            Hasher::Crc32(h) => h.update(data),
            Hasher::Blake2b(h) => blake2::Digest::update(h.as_mut(), data),
        }
    }

    /// In lower-case hex, as sha256sum, b2sum and the like print it.
    fn finish(self) -> String {
        match self {
            Hasher::Glib(c) => c.string().unwrap_or_default(),
            Hasher::Crc32(h) => format!("{:08x}", h.finalize()),
            Hasher::Blake2b(h) => blake2::Digest::finalize(*h).iter().map(|b| format!("{b:02x}")).collect(),
        }
    }
}

/// The checksum `algo` of `f`'s contents, in hex. `from_disk`: what the
/// system still holds of the file in memory (one just written) is let go
/// first, so it's read from the drive or the share. `progress` gets the
/// bytes read so far, now and then; reading stops (Cancelled) once
/// `cancel` is. Local files (and shares the system mounts) are read
/// directly, others (smb://, sftp://, phones) through GIO.
pub fn compute(f: &gio::File, algo: Algo, from_disk: bool, cancel: &gio::Cancellable, progress: &mut dyn FnMut(u64)) -> Result<String, glib::Error> {
    compute_many(f, &[algo], from_disk, cancel, progress).map(|mut v| v.remove(0))
}

/// Several checksums of `f` at once, in the order of `algos`, the file read
/// only once (see compute).
pub fn compute_many(f: &gio::File, algos: &[Algo], from_disk: bool, cancel: &gio::Cancellable, progress: &mut dyn FnMut(u64)) -> Result<Vec<String>, glib::Error> {
    let mut hashers: Vec<Hasher> = algos.iter().map(|a| a.hasher()).collect();
    let mut buf = vec![0u8; 1 << 20];
    let mut read: u64 = 0;
    let mut take = |n: usize, buf: &[u8]| -> Result<(), glib::Error> {
        for h in hashers.iter_mut() {
            h.update(&buf[..n]);
        }
        read += n as u64;
        progress(read);
        if cancel.is_cancelled() {
            return Err(glib::Error::new(gio::IOErrorEnum::Cancelled, "Cancelled"));
        }
        Ok(())
    };
    match f.path().filter(|_| f.is_native()) {
        Some(p) => {
            use std::io::Read;
            use std::os::unix::io::AsRawFd;
            let io = |e: std::io::Error| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string());
            let mut file = std::fs::File::open(&p).map_err(io)?;
            if from_disk {
                // Written out first: only then can it be let go.
                let _ = file.sync_data();
                // SAFETY: a valid descriptor; advice only.
                unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
            }
            loop {
                let n = file.read(&mut buf).map_err(io)?;
                if n == 0 {
                    break;
                }
                take(n, &buf)?;
            }
        }
        None => {
            let input = f.read(Some(cancel))?;
            loop {
                let n = input.read(&mut buf[..], Some(cancel))?;
                if n == 0 {
                    break;
                }
                take(n, &buf)?;
            }
            let _ = input.close(gio::Cancellable::NONE);
        }
    }
    Ok(hashers.into_iter().map(Hasher::finish).collect())
}

/// The checksums to offer, in their order: those not hidden in `order`.
pub fn offered(order: &[String], hidden: &[String]) -> Vec<Algo> {
    order.iter().filter(|id| !hidden.contains(id)).filter_map(|id| Algo::from_id(id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids() {
        let names: Vec<&str> = ALL.iter().map(|a| a.name()).collect();
        assert_eq!(names, ["CRC32", "MD5", "SHA-1", "SHA-256", "SHA-384", "SHA-512", "BLAKE2b"]);
        for a in ALL {
            assert_eq!(Algo::from_id(a.id()), Some(a));
        }
        assert_eq!(Algo::from_id("sha3"), None);
    }

    #[test]
    fn known_checksums() {
        let t = crate::testutil::TestDir::new("checksums");
        let f = gio::File::for_path(t.file("abc", "abc"));
        let none = gio::Cancellable::new();
        let sum = |a| compute(&f, a, false, &none, &mut |_| {}).unwrap();
        assert_eq!(sum(Algo::Md5), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(sum(Algo::Sha1), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(sum(Algo::Sha256), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sum(Algo::Crc32), "352441c2");
        assert!(sum(Algo::Sha512).starts_with("ddaf35a193617aba"));
        assert!(sum(Algo::Sha384).starts_with("cb00753f45a35e8b"));
        assert!(sum(Algo::Blake2b).starts_with("ba80a53f981c4d0d"));
        // Read again from the disk: the same.
        assert_eq!(compute(&f, Algo::Crc32, true, &none, &mut |_| {}).unwrap(), "352441c2");
    }

    #[test]
    fn several_in_one_read() {
        let t = crate::testutil::TestDir::new("checksums-many");
        let f = gio::File::for_path(t.file("abc", "abc"));
        let none = gio::Cancellable::new();
        let mut reads = 0;
        let all = compute_many(&f, &[Algo::Crc32, Algo::Md5], false, &none, &mut |_| reads += 1).unwrap();
        assert_eq!(all, ["352441c2", "900150983cd24fb0d6963f7d28e17f72"]);
        assert_eq!(reads, 1, "read once");
    }

    #[test]
    fn cancelled_and_progress() {
        let t = crate::testutil::TestDir::new("checksums-cancel");
        let data = vec![7u8; 3 << 20];
        let p = t.0.join("big");
        std::fs::write(&p, &data).unwrap();
        let f = gio::File::for_path(&p);
        let cancel = gio::Cancellable::new();
        let mut seen = vec![];
        let r = compute(&f, Algo::Sha256, false, &cancel, &mut |n| {
            seen.push(n);
            if n >= 1 << 20 {
                cancel.cancel();
            }
        });
        assert!(r.is_err_and(|e| e.matches(gio::IOErrorEnum::Cancelled)));
        assert_eq!(seen, [1 << 20], "stopped after the first piece");
    }

    #[test]
    fn offered_in_order_without_the_hidden() {
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(offered(&v(&["sha256", "md5", "crc32", "nope"]), &v(&["md5"])), [Algo::Sha256, Algo::Crc32]);
        let ids: Vec<&str> = ALL.iter().map(|a| a.id()).collect();
        assert_eq!(ids.len(), 7);
        assert!(HIDDEN_AT_FIRST.iter().all(|h| ids.contains(h)));
    }
}
