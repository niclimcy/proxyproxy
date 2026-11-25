//! Minimal ZIP reader (stored and deflate) over any `Read + Seek`, for APKs. Never panics
//! on malformed input and bounds every allocation.
use crate::services::bytes::{le16, le16_usize, le32};
use flate2::read::DeflateDecoder;
use std::io::{Read, Seek, SeekFrom};

const EOCD_SIG: u32 = 0x0605_4b50;
const CEN_SIG: u32 = 0x0201_4b50;
const LOC_SIG: u32 = 0x0403_4b50;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    method: u16,
    comp_size: u64,
    pub size: u64,
    offset: u64,
}

pub struct Zip<R> {
    r: R,
    pub entries: Vec<Entry>,
}

impl<R: Read + Seek> Zip<R> {
    pub fn open(mut r: R) -> Option<Self> {
        let len = r.seek(SeekFrom::End(0)).ok()?;
        // the end record sits within the last 64 KiB (comment is at most that long)
        let tail_len = len.min(22 + 65_535);
        r.seek(SeekFrom::Start(len - tail_len)).ok()?;
        let mut tail = vec![0; tail_len as usize];
        r.read_exact(&mut tail).ok()?;
        let eocd = (0..tail.len().checked_sub(21)?)
            .rev()
            .find(|&i| le32(&tail, i) == Some(EOCD_SIG))?;
        let count = le16_usize(&tail, eocd + 10)?;
        let cd_size = u64::from(le32(&tail, eocd + 12)?);
        let cd_off = u64::from(le32(&tail, eocd + 16)?);
        if count > MAX_ENTRIES || cd_size > 64 << 20 || cd_off.checked_add(cd_size)? > len {
            return None;
        }
        r.seek(SeekFrom::Start(cd_off)).ok()?;
        let mut cd = vec![0; usize::try_from(cd_size).ok()?];
        r.read_exact(&mut cd).ok()?;
        let (mut at, mut entries) = (0usize, Vec::new());
        for _ in 0..count {
            if le32(&cd, at)? != CEN_SIG {
                break;
            }
            let (nlen, xlen, clen) = (
                le16_usize(&cd, at + 28)?,
                le16_usize(&cd, at + 30)?,
                le16_usize(&cd, at + 32)?,
            );
            let name = String::from_utf8_lossy(cd.get(at + 46..at + 46 + nlen)?).into_owned();
            entries.push(Entry {
                name,
                method: le16(&cd, at + 10)?,
                comp_size: u64::from(le32(&cd, at + 20)?),
                size: u64::from(le32(&cd, at + 24)?),
                offset: u64::from(le32(&cd, at + 42)?),
            });
            at += 46 + nlen + xlen + clen;
        }
        Some(Self { r, entries })
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Contents of `e`, or `None` if it is larger than `max` or can't be decoded.
    pub fn read(&mut self, e: &Entry, max: u64) -> Option<Vec<u8>> {
        if e.size > max || e.comp_size > max.max(e.size) {
            return None;
        }
        self.r.seek(SeekFrom::Start(e.offset)).ok()?;
        let mut head = [0u8; 30];
        self.r.read_exact(&mut head).ok()?;
        if le32(&head, 0)? != LOC_SIG {
            return None;
        }
        let skip = i64::from(le16(&head, 26)?) + i64::from(le16(&head, 28)?);
        self.r.seek(SeekFrom::Current(skip)).ok()?;
        let raw = (&mut self.r).take(e.comp_size);
        let mut out = Vec::with_capacity(e.size.min(1 << 20) as usize);
        match e.method {
            0 => raw.take(max).read_to_end(&mut out).ok()?,
            8 => DeflateDecoder::new(raw)
                .take(max + 1)
                .read_to_end(&mut out)
                .ok()?,
            _ => return None,
        };
        (out.len() as u64 <= max).then_some(out)
    }
}

/// Test helper: builds a zip with stored or deflated entries.
#[cfg(test)]
pub fn build(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
    use flate2::{Compression, write::DeflateEncoder};
    use std::io::Write;
    let (mut out, mut cd) = (Vec::new(), Vec::new());
    for (name, data, deflate) in files {
        let comp = if *deflate {
            let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        } else {
            data.to_vec()
        };
        let off = out.len() as u32;
        let method: u16 = if *deflate { 8 } else { 0 };
        out.extend(LOC_SIG.to_le_bytes());
        out.extend([20, 0, 0, 0]);
        out.extend(method.to_le_bytes());
        out.extend([0; 8]);
        out.extend((comp.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0, 0]);
        out.extend(name.as_bytes());
        out.extend(&comp);
        cd.extend(CEN_SIG.to_le_bytes());
        cd.extend([20, 0, 20, 0, 0, 0]);
        cd.extend(method.to_le_bytes());
        cd.extend([0; 8]);
        cd.extend((comp.len() as u32).to_le_bytes());
        cd.extend((data.len() as u32).to_le_bytes());
        cd.extend((name.len() as u16).to_le_bytes());
        cd.extend([0; 12]);
        cd.extend(off.to_le_bytes());
        cd.extend(name.as_bytes());
    }
    let cd_off = out.len() as u32;
    out.extend(&cd);
    out.extend(EOCD_SIG.to_le_bytes());
    out.extend([0; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((cd.len() as u32).to_le_bytes());
    out.extend(cd_off.to_le_bytes());
    out.extend([0, 0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_stored_and_deflated_entries() {
        let big = vec![7u8; 5000];
        let z = build(&[("a.txt", b"hello", false), ("dir/b.bin", &big, true)]);
        let mut zip = Zip::open(Cursor::new(z)).unwrap();
        let a = zip.find("a.txt").unwrap().clone();
        assert_eq!(zip.read(&a, 100).unwrap(), b"hello");
        let b = zip.find("dir/b.bin").unwrap().clone();
        assert_eq!(zip.read(&b, 10_000).unwrap(), big);
        assert!(zip.read(&b, 100).is_none(), "size cap");
        assert!(zip.find("nope").is_none());
    }

    #[test]
    fn malformed_input_never_panics() {
        let z = build(&[("a", b"hello world", true), ("b", b"x", false)]);
        for n in 0..z.len() {
            if let Some(mut zip) = Zip::open(Cursor::new(&z[..n])) {
                for e in zip.entries.clone() {
                    let _ = zip.read(&e, 1 << 10);
                }
            }
        }
        for i in 0..z.len() {
            let mut bad = z.clone();
            bad[i] ^= 0xFF;
            if let Some(mut zip) = Zip::open(Cursor::new(bad)) {
                for e in zip.entries.clone() {
                    let _ = zip.read(&e, 1 << 10);
                }
            }
        }
        assert!(Zip::open(Cursor::new(vec![0xAB; 300])).is_none());
    }
}
