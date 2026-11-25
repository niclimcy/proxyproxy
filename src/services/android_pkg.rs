//! Android binary XML (`AndroidManifest.xml`) and resource table (`resources.arsc`) readers.
//! Only what an icon and label lookup needs. Every offset is bounds-checked, so malformed
//! input yields `None` instead of a panic.
use crate::services::bytes::{le16, le16_usize, le32, le32_usize};

const RES_STRING_POOL: u16 = 0x0001;
const RES_TABLE: u16 = 0x0002;
const RES_XML_START: u16 = 0x0102;
const RES_PACKAGE: u16 = 0x0200;
const RES_TYPE: u16 = 0x0201;
const TYPE_STRING: u8 = 0x03;
const TYPE_REFERENCE: u8 = 0x01;
const TYPE_INT_DEC: u8 = 0x10;
const MAX_STRINGS: usize = 200_000;

/// `(type, header size, total size)` of the chunk at `at`, checked to fit in `b`.
fn chunk(b: &[u8], at: usize) -> Option<(u16, usize, usize)> {
    let (ty, head, size) = (le16(b, at)?, le16_usize(b, at + 2)?, le32_usize(b, at + 4)?);
    (head >= 8 && size >= head && at.checked_add(size)? <= b.len()).then_some((ty, head, size))
}

#[derive(Debug, Default)]
pub struct StringPool(Vec<String>);

impl StringPool {
    fn parse(b: &[u8], at: usize) -> Option<Self> {
        let (ty, head, size) = chunk(b, at)?;
        if ty != RES_STRING_POOL {
            return None;
        }
        let count = le32_usize(b, at + 8)?;
        let (flags, start) = (le32(b, at + 16)?, le32_usize(b, at + 20)?);
        if count > MAX_STRINGS || count.checked_mul(4)? > size {
            return None;
        }
        let end = at + size;
        let utf8 = flags & 0x100 != 0;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let off = le32_usize(b, at + head + i * 4)?;
            let s = at.checked_add(start)?.checked_add(off)?;
            out.push(Self::string(b.get(..end)?, s, utf8).unwrap_or_default());
        }
        Some(Self(out))
    }

    fn string(b: &[u8], at: usize, utf8: bool) -> Option<String> {
        if utf8 {
            let skip_len = |p: usize| -> Option<(usize, usize)> {
                let first = *b.get(p)? as usize;
                if first & 0x80 != 0 {
                    Some((((first & 0x7f) << 8) | *b.get(p + 1)? as usize, 2))
                } else {
                    Some((first, 1))
                }
            };
            let (_, n) = skip_len(at)?;
            let (blen, m) = skip_len(at + n)?;
            let s = at + n + m;
            Some(String::from_utf8_lossy(b.get(s..s.checked_add(blen)?)?).into_owned())
        } else {
            let mut len = le16_usize(b, at)?;
            let mut s = at + 2;
            if len & 0x8000 != 0 {
                len = ((len & 0x7fff) << 16) | le16_usize(b, s)?;
                s += 2;
            }
            let units: Vec<u16> = (0..len)
                .map(|i| le16(b, s + i * 2))
                .collect::<Option<_>>()?;
            Some(String::from_utf16_lossy(&units))
        }
    }

    pub fn get(&self, i: u32) -> Option<&str> {
        self.0.get(i as usize).map(String::as_str)
    }
}

/// An attribute value that is either literal text or a resource id to look up.
#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Text(String),
    Res(u32),
}

#[derive(Debug, Default, PartialEq)]
pub struct Manifest {
    pub package: String,
    pub version_code: u32,
    pub icon: Option<Attr>,
    pub label: Option<Attr>,
}

pub fn parse_manifest(b: &[u8]) -> Option<Manifest> {
    let (ty, head, size) = chunk(b, 0)?;
    if ty != 0x0003 {
        return None;
    }
    let pool = StringPool::parse(b, head)?;
    let mut at = head + chunk(b, head)?.2;
    let mut m = Manifest::default();
    while at < size.min(b.len()) {
        let (ty, _, csize) = chunk(b, at)?;
        if ty == RES_XML_START {
            let name = pool.get(le32(b, at + 20)?)?;
            let (astart, asize, acount) = (
                le16_usize(b, at + 24)?,
                le16_usize(b, at + 26)?,
                le16_usize(b, at + 28)?,
            );
            if asize < 20 || acount > 4096 {
                return None;
            }
            for i in 0..acount {
                let a = at + 16 + astart + i * asize;
                let attr = pool.get(le32(b, a + 4)?).unwrap_or_default();
                let (dtype, data) = (*b.get(a + 15)?, le32(b, a + 16)?);
                let val = match dtype {
                    TYPE_STRING => Some(Attr::Text(pool.get(data)?.to_string())),
                    TYPE_REFERENCE => Some(Attr::Res(data)),
                    _ => None,
                };
                match (name, attr) {
                    ("manifest", "package") => {
                        if let Some(Attr::Text(t)) = val {
                            m.package = t;
                        }
                    }
                    ("manifest", "versionCode") if dtype == TYPE_INT_DEC => m.version_code = data,
                    ("application", "icon") => m.icon = val,
                    ("application", "label") => m.label = val,
                    _ => {}
                }
            }
            if name == "application" {
                break;
            }
        }
        at += csize;
    }
    (!m.package.is_empty()).then_some(m)
}

/// One resolved value of a resource id under a given configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub text: String,
    /// 0 for the default configuration; larger is denser (`xxhdpi` = 480).
    pub density: u16,
}

pub struct Resources<'a> {
    b: &'a [u8],
    global: StringPool,
    /// `(package id, offset)` of each package chunk.
    packages: Vec<(u8, usize)>,
}

impl<'a> Resources<'a> {
    pub fn parse(b: &'a [u8]) -> Option<Self> {
        let (ty, head, size) = chunk(b, 0)?;
        if ty != RES_TABLE {
            return None;
        }
        let global = StringPool::parse(b, head)?;
        let mut at = head + chunk(b, head)?.2;
        let mut packages = vec![];
        while at < size {
            let (ty, _, csize) = chunk(b, at)?;
            if ty == RES_PACKAGE {
                packages.push((u8::try_from(le32(b, at + 8)?).ok()?, at));
            }
            at += csize;
        }
        Some(Self {
            b,
            global,
            packages,
        })
    }

    /// Every string value of `id` (following one level of references), densest first.
    pub fn values(&self, id: u32) -> Vec<Value> {
        let mut out = Vec::new();
        for v in self.lookup(id).unwrap_or_default() {
            match v {
                (Ok(text), density) => out.push(Value { text, density }),
                (Err(r), density) => {
                    out.extend(self.lookup(r).unwrap_or_default().into_iter().filter_map(
                        |(t, d)| {
                            Some(Value {
                                text: t.ok()?,
                                density: d.max(density),
                            })
                        },
                    ));
                }
            }
        }
        out.sort_by_key(|v| std::cmp::Reverse(v.density));
        out
    }

    /// `Ok(string)` or `Err(referenced id)` per configuration.
    fn lookup(&self, id: u32) -> Option<Vec<(Result<String, u32>, u16)>> {
        let [pkg, ty, e1, e0] = id.to_be_bytes();
        let entry = usize::from(u16::from_be_bytes([e1, e0]));
        let &(_, pat) = self.packages.iter().find(|(p, _)| *p == pkg)?;
        let (_, phead, psize) = chunk(self.b, pat)?;
        let (mut at, end) = (pat + phead, pat + psize);
        let mut out = vec![];
        while at < end {
            let (cty, chead, csize) = chunk(self.b, at)?;
            if cty == RES_TYPE
                && *self.b.get(at + 8)? == ty
                && let Some(v) = self.entry(at, chead, entry)
            {
                out.push(v);
            }
            at += csize;
        }
        Some(out)
    }

    fn entry(&self, at: usize, head: usize, idx: usize) -> Option<(Result<String, u32>, u16)> {
        let (count, estart) = (le32_usize(self.b, at + 12)?, le32_usize(self.b, at + 16)?);
        if idx >= count {
            return None;
        }
        let density = le16(self.b, at + 20 + 14).unwrap_or(0);
        let off = le32(self.b, at.checked_add(head)?.checked_add(idx * 4)?)?;
        if off == u32::MAX {
            return None;
        }
        let e = at.checked_add(estart)?.checked_add(off as usize)?;
        // complex (map) entries have no single value
        if le16(self.b, e + 2)? & 1 != 0 {
            return None;
        }
        let (dtype, data) = (*self.b.get(e + 11)?, le32(self.b, e + 12)?);
        match dtype {
            TYPE_STRING => Some((Ok(self.global.get(data)?.to_string()), density)),
            TYPE_REFERENCE => Some((Err(data), density)),
            _ => None,
        }
    }
}

#[cfg(test)]
pub mod fixtures {
    //! Builders for tiny but structurally real manifests and resource tables.
    fn w16(v: &mut Vec<u8>, x: u16) {
        v.extend(x.to_le_bytes());
    }
    fn w32(v: &mut Vec<u8>, x: u32) {
        v.extend(x.to_le_bytes());
    }

    pub fn pool(strings: &[&str], utf8: bool) -> Vec<u8> {
        let (mut offs, mut data) = (vec![], vec![]);
        for s in strings {
            offs.push(data.len() as u32);
            if utf8 {
                data.push(s.chars().count() as u8);
                data.push(s.len() as u8);
                data.extend(s.bytes());
                data.push(0);
            } else {
                let u: Vec<u16> = s.encode_utf16().collect();
                w16(&mut data, u.len() as u16);
                for c in &u {
                    w16(&mut data, *c);
                }
                w16(&mut data, 0);
            }
        }
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let start = 28 + strings.len() * 4;
        let mut v = vec![];
        w16(&mut v, 1);
        w16(&mut v, 28);
        w32(&mut v, (start + data.len()) as u32);
        w32(&mut v, strings.len() as u32);
        w32(&mut v, 0);
        w32(&mut v, if utf8 { 0x100 } else { 0 });
        w32(&mut v, start as u32);
        w32(&mut v, 0);
        for o in &offs {
            w32(&mut v, *o);
        }
        v.extend(data);
        v
    }

    /// (element name string idx, attrs); attrs are (name string idx, data type, data).
    pub type Elem = (u32, Vec<(u32, u8, u32)>);

    pub fn manifest(strings: &[&str], elems: &[Elem]) -> Vec<u8> {
        let mut body = pool(strings, true);
        for (name, attrs) in elems {
            let size = 36 + attrs.len() * 20;
            w16(&mut body, 0x102);
            w16(&mut body, 16);
            w32(&mut body, size as u32);
            w32(&mut body, 1);
            w32(&mut body, u32::MAX);
            w32(&mut body, u32::MAX);
            w32(&mut body, *name);
            w16(&mut body, 20);
            w16(&mut body, 20);
            w16(&mut body, attrs.len() as u16);
            body.extend([0; 6]);
            for (an, ty, data) in attrs {
                w32(&mut body, u32::MAX);
                w32(&mut body, *an);
                w32(&mut body, if *ty == 3 { *data } else { u32::MAX });
                w16(&mut body, 8);
                body.push(0);
                body.push(*ty);
                w32(&mut body, *data);
            }
        }
        let mut v = vec![];
        w16(&mut v, 3);
        w16(&mut v, 8);
        w32(&mut v, (8 + body.len()) as u32);
        v.extend(body);
        v
    }

    /// One package (id 0x7f) with one type (id 1) holding `entries[config]`, each a
    /// `(density, [value per entry: (data type, data)])`.
    pub fn table(global: &[&str], configs: &[(u16, Vec<(u8, u32)>)]) -> Vec<u8> {
        let mut pkg = vec![];
        w16(&mut pkg, 0x200);
        w16(&mut pkg, 288);
        w32(&mut pkg, 0); // size, patched below
        w32(&mut pkg, 0x7f);
        pkg.resize(288, 0);
        for (density, vals) in configs {
            let head = 20 + 64;
            let mut t = vec![];
            w16(&mut t, 0x201);
            w16(&mut t, head as u16);
            w32(&mut t, 0);
            t.push(1);
            t.extend([0; 3]);
            w32(&mut t, vals.len() as u32);
            w32(&mut t, (head + vals.len() * 4) as u32);
            w32(&mut t, 64);
            t.extend([0; 10]);
            w16(&mut t, *density);
            t.resize(head, 0);
            for i in 0..vals.len() {
                w32(&mut t, (i * 16) as u32);
            }
            for (ty, data) in vals {
                w16(&mut t, 8);
                w16(&mut t, 0);
                w32(&mut t, 0);
                w16(&mut t, 8);
                t.push(0);
                t.push(*ty);
                w32(&mut t, *data);
            }
            let n = t.len() as u32;
            t[4..8].copy_from_slice(&n.to_le_bytes());
            pkg.extend(t);
        }
        let n = pkg.len() as u32;
        pkg[4..8].copy_from_slice(&n.to_le_bytes());
        let gp = pool(global, true);
        let mut v = vec![];
        w16(&mut v, 2);
        w16(&mut v, 12);
        w32(&mut v, (12 + gp.len() + pkg.len()) as u32);
        w32(&mut v, 1);
        v.extend(gp);
        v.extend(pkg);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn sample_manifest() -> Vec<u8> {
        let strings = [
            "manifest",
            "package",
            "versionCode",
            "application",
            "icon",
            "label",
            "com.foo.bar",
            "Foo",
        ];
        manifest(
            &strings,
            &[
                (0, vec![(1, 3, 6), (2, 0x10, 42)]),
                (3, vec![(4, 1, 0x7f01_0000), (5, 3, 7)]),
            ],
        )
    }

    #[test]
    fn manifest_package_version_icon_label() {
        let m = parse_manifest(&sample_manifest()).unwrap();
        assert_eq!(m.package, "com.foo.bar");
        assert_eq!(m.version_code, 42);
        assert_eq!(m.icon, Some(Attr::Res(0x7f01_0000)));
        assert_eq!(m.label, Some(Attr::Text("Foo".into())));
    }

    #[test]
    fn resources_pick_densest_and_follow_references() {
        let g = [
            "res/mipmap-mdpi/ic.png",
            "res/mipmap-xxhdpi/ic.png",
            "My App",
        ];
        let t = table(
            &g,
            &[
                (160, vec![(3, 0), (3, 2)]),
                (480, vec![(3, 1), (1, 0x7f01_0001)]),
            ],
        );
        let r = Resources::parse(&t).unwrap();
        let v = r.values(0x7f01_0000);
        assert_eq!(
            v[0],
            Value {
                text: "res/mipmap-xxhdpi/ic.png".into(),
                density: 480
            }
        );
        assert_eq!(v[1].density, 160);
        // entry 1 is a string in mdpi and a reference to itself in xxhdpi; the reference resolves
        let l = r.values(0x7f01_0001);
        assert!(l.iter().any(|x| x.text == "My App"));
        assert!(r.values(0x7f01_0009).is_empty());
        assert!(r.values(0x7e01_0000).is_empty());
    }

    #[test]
    fn malformed_input_never_panics() {
        let m = sample_manifest();
        let g = ["a.png", "b"];
        let t = table(&g, &[(160, vec![(3, 0)])]);
        for data in [&m, &t] {
            for n in 0..data.len() {
                let _ = parse_manifest(&data[..n]);
                if let Some(r) = Resources::parse(&data[..n]) {
                    let _ = r.values(0x7f01_0000);
                }
            }
            for i in 0..data.len() {
                for x in [0xFFu8, 0x00, 0x80] {
                    let mut bad = data.clone();
                    bad[i] = x;
                    let _ = parse_manifest(&bad);
                    if let Some(r) = Resources::parse(&bad) {
                        let _ = r.values(0x7f01_0000);
                    }
                }
            }
        }
    }
}
