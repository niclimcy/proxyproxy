//! Best-effort reader for the compiled asset catalog (`Assets.car`) of an iOS or macOS
//! app: finds the app icon facet and returns its largest rendition as PNG bytes.
//!
//! The file is a BOM container (big-endian) of named B-trees (`FACETKEYS`, `KEYFORMAT`,
//! `RENDITIONS`); each rendition is a little-endian `CTSI` record. Renditions stored as
//! PNG, or as raw, zlib or LZFSE/LZVN-compressed BGRA bitmaps (Xcode's `KCBC` blocks), are decoded.
//! RLE and palette bitmaps are skipped, so those apps fall back to loose icons.
//! The format is undocumented, so every read is bounds-checked and returns `None` on doubt.
use flate2::read::ZlibDecoder;
use std::collections::HashSet;
use std::io::Read;

const MAX_SIDE: u32 = 4096;
const MAX_NODES: usize = 100_000;
const ATTR_IDENTIFIER: u16 = 17;
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

struct Bom<'a> {
    d: &'a [u8],
    index: usize,
    count: usize,
}

impl<'a> Bom<'a> {
    fn open(d: &'a [u8]) -> Option<Self> {
        if d.get(..8)? != b"BOMStore" {
            return None;
        }
        let (index, ilen) = (be32(d, 16)? as usize, be32(d, 20)? as usize);
        d.get(index..index.checked_add(ilen)?)?;
        Some(Self { d, index, count: be32(d, index)? as usize })
    }

    fn block(&self, i: u32) -> Option<&'a [u8]> {
        let i = i as usize;
        if i >= self.count {
            return None;
        }
        let e = self.index.checked_add(4)?.checked_add(i.checked_mul(8)?)?;
        let (off, len) = (be32(self.d, e)? as usize, be32(self.d, e + 4)? as usize);
        self.d.get(off..off.checked_add(len)?)
    }

    /// Block index of the named variable (`FACETKEYS`, ...).
    fn var(&self, name: &str) -> Option<u32> {
        let (off, len) = (be32(self.d, 24)? as usize, be32(self.d, 28)? as usize);
        let v = self.d.get(off..off.checked_add(len)?)?;
        let n = be32(v, 0)? as usize;
        let mut at = 4;
        for _ in 0..n.min(1024) {
            let idx = be32(v, at)?;
            let l = *v.get(at + 4)? as usize;
            if v.get(at + 5..at + 5 + l)? == name.as_bytes() {
                return Some(idx);
            }
            at += 5 + l;
        }
        None
    }

    /// All `(key, value)` blocks of a tree, in leaf order.
    fn tree(&self, name: &str) -> Option<Vec<(&'a [u8], &'a [u8])>> {
        let head = self.block(self.var(name)?)?;
        if head.get(..4)? != b"tree" {
            return None;
        }
        let mut node = self.block(be32(head, 8)?)?;
        // descend to the leftmost leaf, then follow the forward links
        for _ in 0..64 {
            if be16(node, 0)? != 0 {
                break;
            }
            node = self.block(be32(node, 12)?)?;
        }
        let (mut out, mut hops) = (vec![], 0);
        loop {
            if be16(node, 0)? == 0 {
                return None;
            }
            for i in 0..be16(node, 2)? as usize {
                let (v, k) = (be32(node, 12 + i * 8)?, be32(node, 16 + i * 8)?);
                out.push((self.block(k)?, self.block(v)?));
                if out.len() > MAX_NODES {
                    return None;
                }
            }
            hops += 1;
            let next = be32(node, 4)?;
            if next == 0 || hops > MAX_NODES {
                break;
            }
            node = self.block(next)?;
        }
        Some(out)
    }
}

/// Rendition key attribute ids, in key order.
fn key_format(b: &[u8]) -> Option<Vec<u16>> {
    if b.get(..4)? != b"tmfk" {
        return None;
    }
    let n = le32(b, 8)? as usize;
    (0..n.min(64)).map(|i| le32(b, 12 + i * 4).map(|v| v as u16)).collect()
}

/// Identifier value of each facet whose name is in `names`.
fn facet_ids(facets: &[(&[u8], &[u8])], names: &[&str]) -> HashSet<u16> {
    let mut ids = HashSet::new();
    for (k, v) in facets {
        let Ok(name) = std::str::from_utf8(k) else { continue };
        if !names.contains(&name) {
            continue;
        }
        let n = le16(v, 4).unwrap_or(0) as usize;
        for i in 0..n.min(64) {
            if le16(v, 6 + i * 4) == Some(ATTR_IDENTIFIER)
                && let Some(id) = le16(v, 8 + i * 4)
            {
                ids.insert(id);
            }
        }
    }
    ids
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

/// PNG bytes of the largest rendition of the first of `names` that has any (try the icon
/// set name from Info.plist, then `AppIcon`).
pub fn icon_png(car: &[u8], names: &[&str]) -> Option<Vec<u8>> {
    let bom = Bom::open(car)?;
    let fmt = key_format(bom.block(bom.var("KEYFORMAT")?)?)?;
    let ident_pos = fmt.iter().position(|a| *a == ATTR_IDENTIFIER)?;
    let facets = bom.tree("FACETKEYS")?;
    let renditions = bom.tree("RENDITIONS")?;
    for name in names {
        let ids = facet_ids(&facets, &[name]);
        let mut cands: Vec<(u32, &[u8])> = renditions
            .iter()
            .filter(|(k, _)| le16(k, ident_pos * 2).is_some_and(|id| ids.contains(&id)))
            .filter_map(|(_, v)| Some((le32(v, 12)?.saturating_mul(le32(v, 16)?), *v)))
            .collect();
        cands.sort_by_key(|c| std::cmp::Reverse(c.0));
        if let Some(png) = cands.into_iter().find_map(|(_, v)| rendition_png(v)) {
            return Some(png);
        }
    }
    None
}

/// Decodes one `CTSI` record.
fn rendition_png(v: &[u8]) -> Option<Vec<u8>> {
    if v.get(..4)? != b"ISTC" {
        return None;
    }
    let (w, h) = (le32(v, 12)?, le32(v, 16)?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let tlv = le32(v, 168)? as usize;
    let data = v.get(180usize.checked_add(tlv)?..)?;
    if let Some(png) = embedded_png(data) {
        return Some(png.to_vec());
    }
    let pixels = bitmap(data, (w * h * 4) as usize)?;
    // BOM stores premultiplied BGRA
    let rgba: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u32::from(p[3]);
            let un = |c: u8| (u32::from(c) * 255).checked_div(a).map_or(0, |v| v.min(255) as u8);
            [un(p[2]), un(p[1]), un(p[0]), p[3]]
        })
        .collect();
    let img = image::RgbaImage::from_raw(w, h, rgba)?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn embedded_png(d: &[u8]) -> Option<&[u8]> {
    let start = d.windows(8).position(|w| w == PNG_MAGIC)?;
    let rest = &d[start..];
    let end = rest.windows(8).position(|w| &w[..4] == b"IEND")? + 8;
    rest.get(..end)
}

/// Pixel bytes of a `MLEC` bitmap block, `want` bytes exactly (which also bounds the
/// allocation). The payload is raw (0), zlib (2) or `KCBC` row blocks of LZFSE/LZVN (4).
fn bitmap(d: &[u8], want: usize) -> Option<Vec<u8>> {
    let p = d[..d.len().min(64)].windows(4).position(|w| w == b"MLEC")?;
    let (kind, payload) = (le32(d, p + 8)?, d.get(p + 16..)?);
    let out = if payload.starts_with(b"KCBC") {
        kcbc(payload, want)?
    } else {
        let body = payload.get(..le32(d, p + 12)? as usize)?;
        match kind {
            0 => body.to_vec(),
            2 => read_capped(ZlibDecoder::new(body), want)?,
            _ => return None,
        }
    };
    (out.len() == want).then_some(out)
}

fn read_capped(r: impl Read, want: usize) -> Option<Vec<u8>> {
    let mut o = Vec::with_capacity(want.min(1 << 20));
    r.take(want as u64 + 1).read_to_end(&mut o).ok()?;
    Some(o)
}

/// A run of blocks, each `KCBC`, two reserved words, the row count, the stream length and
/// an LZFSE (or LZVN) stream holding that many rows.
fn kcbc(b: &[u8], want: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(want.min(1 << 20));
    let mut at = 0;
    let mut dec = lzfse_rust::LzfseRingDecoder::default();
    while out.len() < want && b.get(at..at + 4)? == b"KCBC" {
        let len = le32(b, at + 16)? as usize;
        let block = b.get(at + 20..(at + 20).checked_add(len)?)?;
        let room = (want - out.len()) as u64 + 1;
        std::io::Read::take(dec.reader_bytes(block), room).read_to_end(&mut out).ok()?;
        at += 20 + len;
    }
    Some(out)
}

#[cfg(test)]
pub mod fixtures {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn w32(v: &mut Vec<u8>, x: u32) {
        v.extend(x.to_be_bytes());
    }

    /// Minimal CTSI record. `body` goes after the (empty) TLV block.
    pub fn csi(w: u32, h: u32, body: &[u8]) -> Vec<u8> {
        let mut v = b"ISTC".to_vec();
        v.resize(12, 0);
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.resize(168, 0);
        v.extend(0u32.to_le_bytes());
        v.extend([0; 4]);
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        v
    }

    pub fn bitmap_block(kind: u32, w: u32, pixels: &[u8]) -> Vec<u8> {
        if kind == 4 {
            // like Xcode: one `KCBC` block per run of rows
            let mut v = (w * 4).to_le_bytes().to_vec();
            v.extend(b"MLEC");
            v.extend(1u32.to_le_bytes());
            v.extend(4u32.to_le_bytes());
            v.extend(4u32.to_le_bytes());
            for part in pixels.chunks(pixels.len().div_ceil(2).max(1)) {
                let mut z = Vec::new();
                lzfse_rust::encode_bytes(part, &mut z).unwrap();
                v.extend(b"KCBC");
                v.extend([0; 8]);
                v.extend(((part.len() / (w as usize * 4)) as u32).to_le_bytes());
                v.extend((z.len() as u32).to_le_bytes());
                v.extend(z);
            }
            return v;
        }
        let data = if kind == 2 {
            let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
            e.write_all(pixels).unwrap();
            e.finish().unwrap()
        } else {
            pixels.to_vec()
        };
        let mut v = (w * 4).to_le_bytes().to_vec();
        v.extend(b"MLEC");
        v.extend(1u32.to_le_bytes());
        v.extend(kind.to_le_bytes());
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }

    fn leaf(entries: &[(u32, u32)]) -> Vec<u8> {
        let mut n = vec![0, 1];
        n.extend((entries.len() as u16).to_be_bytes());
        n.extend([0; 8]);
        for (v, k) in entries {
            w32(&mut n, *v);
            w32(&mut n, *k);
        }
        n
    }

    /// A BOM with KEYFORMAT `[identifier]`, an icon facet `name` with id 7 and the given
    /// renditions (each stored under identifier 7, or 9 when `other`).
    pub fn car(name: &str, renditions: &[(Vec<u8>, bool)]) -> Vec<u8> {
        let mut blocks: Vec<Vec<u8>> = vec![vec![]];
        let mut add = |b: Vec<u8>| {
            blocks.push(b);
            (blocks.len() - 1) as u32
        };
        let mut kf = b"tmfk".to_vec();
        kf.extend([0; 4]);
        kf.extend(1u32.to_le_bytes());
        kf.extend(17u32.to_le_bytes());
        let kf = add(kf);
        let mut facet = vec![0, 0, 0, 0];
        facet.extend(1u16.to_le_bytes());
        facet.extend(17u16.to_le_bytes());
        facet.extend(7u16.to_le_bytes());
        let (fk, fv) = (add(name.as_bytes().to_vec()), add(facet));
        let fleaf = add(leaf(&[(fv, fk)]));
        let mut ents = vec![];
        for (csi, other) in renditions {
            let k = add((if *other { 9u16 } else { 7 }).to_le_bytes().to_vec());
            ents.push((add(csi.clone()), k));
        }
        let rleaf = add(leaf(&ents));
        let tree = |root: u32| {
            let mut t = b"tree".to_vec();
            w32(&mut t, 1);
            w32(&mut t, root);
            t
        };
        let (ft, rt) = (add(tree(fleaf)), add(tree(rleaf)));
        let vars: Vec<(&str, u32)> = vec![("KEYFORMAT", kf), ("FACETKEYS", ft), ("RENDITIONS", rt)];
        let mut vbytes = vec![];
        w32(&mut vbytes, vars.len() as u32);
        for (n, i) in &vars {
            w32(&mut vbytes, *i);
            vbytes.push(n.len() as u8);
            vbytes.extend(n.as_bytes());
        }
        let mut out = b"BOMStore".to_vec();
        w32(&mut out, 1);
        w32(&mut out, blocks.len() as u32);
        let index_off = 512usize;
        let mut index = vec![];
        w32(&mut index, blocks.len() as u32);
        let mut data: Vec<u8> = vec![];
        let data_off = index_off + 4 + blocks.len() * 8;
        let vars_off = data_off;
        data.extend(&vbytes);
        for b in &blocks {
            w32(&mut index, if b.is_empty() { 0 } else { (data_off + data.len()) as u32 });
            w32(&mut index, b.len() as u32);
            data.extend(b);
        }
        w32(&mut out, index_off as u32);
        w32(&mut out, index.len() as u32);
        w32(&mut out, vars_off as u32);
        w32(&mut out, vbytes.len() as u32);
        out.resize(index_off, 0);
        out.extend(index);
        out.extend(data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut o = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgba8(w, h).write_to(&mut o, image::ImageFormat::Png).unwrap();
        o.into_inner()
    }

    #[test]
    fn picks_largest_png_rendition_of_the_icon_facet() {
        let small = csi(2, 2, &png(2, 2));
        let big = csi(8, 8, &png(8, 8));
        let other = csi(32, 32, &png(32, 32));
        let car = car("AppIcon", &[(small, false), (big, false), (other, true)]);
        let out = icon_png(&car, &["AppIcon"]).unwrap();
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (8, 8));
        assert!(icon_png(&car, &["Missing"]).is_none());
        assert!(icon_png(&car, &["Missing", "AppIcon"]).is_some(), "later names are tried");
    }

    #[test]
    fn decodes_raw_zlib_and_lzfse_bitmaps_unpremultiplying_bgra() {
        // premultiplied half-alpha blue-ish pixel in BGRA
        let px: Vec<u8> = [64u8, 0, 0, 128].repeat(4);
        for kind in [0, 2, 4] {
            let c = csi(2, 2, &bitmap_block(kind, 2, &px));
            let car = car("AppIcon", &[(c, false)]);
            let img = image::load_from_memory(&icon_png(&car, &["AppIcon"]).unwrap()).unwrap().to_rgba8();
            assert_eq!(img.get_pixel(0, 0).0, [0, 0, 127, 128], "kind {kind}");
        }
        // RLE has no decoder here and degrades to None
        let c = csi(2, 2, &bitmap_block(1, 2, &px));
        assert!(icon_png(&car("AppIcon", &[(c, false)]), &["AppIcon"]).is_none());
    }

    #[test]
    fn oversized_or_mismatched_bitmaps_are_rejected() {
        let c = csi(MAX_SIDE + 1, 1, &bitmap_block(0, 2, &[0; 16]));
        assert!(icon_png(&car("AppIcon", &[(c, false)]), &["AppIcon"]).is_none());
        let c = csi(4, 4, &bitmap_block(0, 2, &[0; 16]));
        assert!(icon_png(&car("AppIcon", &[(c, false)]), &["AppIcon"]).is_none());
    }

    #[test]
    fn truncated_and_corrupt_input_never_panics() {
        let car = car("AppIcon", &[(csi(4, 4, &png(4, 4)), false), (csi(2, 2, &bitmap_block(4, 2, &[1; 16])), false)]);
        for n in 0..car.len() {
            let _ = icon_png(&car[..n], &["AppIcon"]);
        }
        for i in (0..car.len()).filter(|i| *i < 700 || i % 7 == 0) {
            for x in [0x00u8, 0xFF, 0x80] {
                let mut bad = car.clone();
                bad[i] = x;
                let _ = icon_png(&bad, &["AppIcon"]);
            }
        }
        assert!(icon_png(&[], &["AppIcon"]).is_none());
        assert!(icon_png(&[0x42; 4096], &["AppIcon"]).is_none());
        assert!(icon_png(b"BOMStore", &["AppIcon"]).is_none());
    }
}
