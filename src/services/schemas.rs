use crate::domain::flow::Message;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

const MAX_BODY: usize = 8 << 20;
const MAX_TRY: usize = 3000;

#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub type_name: String,
    pub text: String,
}

#[derive(Default)]
struct State {
    paths: Vec<String>,
    pool: Option<DescriptorPool>,
    errors: Vec<(String, String)>,
    generation: u64,
    cache: HashMap<u64, Option<Decoded>>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn load(paths: &[String]) -> (Option<DescriptorPool>, Vec<(String, String)>) {
    let mut pool = DescriptorPool::new();
    let (mut errors, mut any) = (vec![], false);
    for p in paths {
        let r = std::fs::read(p)
            .map_err(|e| e.to_string())
            .and_then(|b| prost_types::FileDescriptorSet::decode(b.as_slice()).map_err(|e| e.to_string()))
            .and_then(|set| pool.add_file_descriptor_set(set).map_err(|e| e.to_string()));
        match r {
            Ok(()) => any = true,
            Err(e) => errors.push((p.clone(), e)),
        }
    }
    (any.then_some(pool), errors)
}

// Rebuilds only when the path list changes
pub fn sync(paths: &[String]) {
    let mut s = lock();
    if s.paths != paths {
        rebuild(&mut s, paths.to_vec());
    }
}

pub fn reload() {
    let mut s = lock();
    let p = s.paths.clone();
    rebuild(&mut s, p);
}

fn rebuild(s: &mut State, paths: Vec<String>) {
    let (pool, errors) = load(&paths);
    s.paths = paths;
    s.pool = pool;
    s.errors = errors;
    s.generation += 1;
    s.cache.clear();
}

pub fn errors() -> Vec<(String, String)> {
    lock().errors.clone()
}

pub fn is_loaded() -> bool {
    lock().pool.is_some()
}

pub fn message_types() -> Vec<String> {
    let mut v: Vec<String> = lock().pool.as_ref().map(|p| p.all_messages().filter(|m| !m.is_map_entry()).map(|m| m.full_name().to_string()).collect()).unwrap_or_default();
    v.sort();
    v
}

pub fn stats() -> (usize, usize) {
    lock().pool.as_ref().map(|p| (p.all_messages().count(), p.services().count())).unwrap_or((0, 0))
}

fn has_unknown(m: &DynamicMessage) -> bool {
    use prost_reflect::Value;
    if m.unknown_fields().next().is_some() {
        return true;
    }
    let nested = |v: &Value| match v {
        Value::Message(c) => has_unknown(c),
        _ => false,
    };
    m.fields().any(|(_, v)| match v {
        Value::List(l) => l.iter().any(nested),
        Value::Map(mp) => mp.values().any(nested),
        v => nested(v),
    })
}

fn decode_as(desc: &MessageDescriptor, data: &[u8]) -> Option<DynamicMessage> {
    DynamicMessage::decode(desc.clone(), data).ok()
}

// Strict: no unknown fields anywhere, at least one field set; higher score = better fit
fn best_guess(pool: &DescriptorPool, data: &[u8]) -> Option<(MessageDescriptor, DynamicMessage)> {
    if data.is_empty() {
        return None;
    }
    let mut best: Option<(usize, MessageDescriptor, DynamicMessage)> = None;
    for d in pool.all_messages().filter(|d| !d.is_map_entry()).take(MAX_TRY) {
        let Some(m) = decode_as(&d, data) else { continue };
        let score = m.fields().count();
        if score == 0 || has_unknown(&m) {
            continue;
        }
        if best.as_ref().is_none_or(|(s, ..)| score > *s) {
            best = Some((score, d, m));
        }
    }
    best.map(|(_, d, m)| (d, m))
}

fn to_text(m: &DynamicMessage) -> Option<String> {
    serde_json::to_string_pretty(m).ok()
}

pub fn grpc_frames(body: &[u8]) -> Option<Vec<(u8, &[u8])>> {
    let (mut i, mut out) = (0, vec![]);
    while i < body.len() {
        let hdr = body.get(i..i + 5)?;
        let n = u32::from_be_bytes(hdr[1..5].try_into().ok()?) as usize;
        let data = body.get(i + 5..i + 5usize.checked_add(n)?)?;
        out.push((hdr[0], data));
        i += 5 + n;
    }
    (!out.is_empty()).then_some(out)
}

fn grpc_method(pool: &DescriptorPool, path: &str, response: bool) -> Option<MessageDescriptor> {
    let path = path.split('?').next()?.strip_prefix('/')?;
    let (svc, method) = path.split_once('/')?;
    let m = pool.get_service_by_name(svc)?.methods().find(|m| m.name() == method)?;
    Some(if response { m.output() } else { m.input() })
}

fn decode_with(pool: &DescriptorPool, ct: &str, body: &[u8], path: &str, response: bool, forced: Option<&str>) -> Option<Decoded> {
    let forced = forced.and_then(|n| pool.get_message_by_name(n));
    let grpc = ct.contains("grpc");
    let frames: Vec<&[u8]> = match grpc.then(|| grpc_frames(body)).flatten() {
        Some(f) => {
            if f.iter().any(|(flag, _)| flag & 1 != 0) {
                return None;
            }
            f.into_iter().filter(|(flag, _)| flag & 0x80 == 0).map(|(_, d)| d).collect()
        }
        None => vec![body],
    };
    let first = frames.first()?;
    let desc = forced
        .or_else(|| grpc.then(|| grpc_method(pool, path, response)).flatten())
        .or_else(|| best_guess(pool, first).map(|(d, _)| d))?;
    let mut parts = vec![];
    for f in &frames {
        parts.push(to_text(&decode_as(&desc, f)?)?);
    }
    let text = if parts.len() == 1 { parts.remove(0) } else { parts.iter().enumerate().map(|(i, p)| format!("// message {}\n{p}", i + 1)).collect::<Vec<_>>().join("\n") };
    Some(Decoded { type_name: desc.full_name().to_string(), text })
}

// Cached per (body, direction, path, forced type, pool generation)
pub fn decode(m: &Message, path: &str, response: bool, forced: Option<&str>) -> Option<Decoded> {
    if m.body.is_empty() || m.body.len() > MAX_BODY {
        return None;
    }
    let s = lock();
    let pool = s.pool.clone()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&m.body, m.content_type(), path.split('?').next(), response, forced, s.generation).hash(&mut h);
    let key = h.finish();
    if let Some(c) = s.cache.get(&key) {
        return c.clone();
    }
    drop(s);
    let r = decode_with(&pool, &m.content_type(), &m.body, path, response, forced);
    let mut s = lock();
    if s.cache.len() > 256 {
        s.cache.clear();
    }
    s.cache.insert(key, r.clone());
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_reflect::Value;
    use prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };

    fn field(name: &str, n: i32, t: Type) -> FieldDescriptorProto {
        FieldDescriptorProto { name: Some(name.into()), number: Some(n), label: Some(Label::Optional as i32), r#type: Some(t as i32), json_name: Some(name.into()), ..Default::default() }
    }

    fn pool() -> DescriptorPool {
        let req = DescriptorProto { name: Some("HelloRequest".into()), field: vec![field("name", 1, Type::String), field("age", 2, Type::Int32)], ..Default::default() };
        let rep = DescriptorProto { name: Some("HelloReply".into()), field: vec![field("greeting", 1, Type::String), field("ok", 2, Type::Bool), field("code", 3, Type::Int64)], ..Default::default() };
        let svc = ServiceDescriptorProto {
            name: Some("Greeter".into()),
            method: vec![MethodDescriptorProto { name: Some("SayHello".into()), input_type: Some(".demo.HelloRequest".into()), output_type: Some(".demo.HelloReply".into()), ..Default::default() }],
            ..Default::default()
        };
        let file = FileDescriptorProto { name: Some("demo.proto".into()), package: Some("demo".into()), syntax: Some("proto3".into()), message_type: vec![req, rep], service: vec![svc], ..Default::default() };
        let set = FileDescriptorSet { file: vec![file] };
        DescriptorPool::decode(set.encode_to_vec().as_slice()).unwrap()
    }

    fn encode(pool: &DescriptorPool, ty: &str, f: &[(&str, Value)]) -> Vec<u8> {
        let mut m = DynamicMessage::new(pool.get_message_by_name(ty).unwrap());
        for (k, v) in f {
            m.set_field_by_name(k, v.clone());
        }
        m.encode_to_vec()
    }

    fn frame(d: &[u8]) -> Vec<u8> {
        let mut v = vec![0];
        v.extend((d.len() as u32).to_be_bytes());
        v.extend(d);
        v
    }

    #[test]
    fn raw_forced_and_auto() {
        let p = pool();
        let body = encode(&p, "demo.HelloRequest", &[("name", Value::String("bob".into())), ("age", Value::I32(7))]);
        let d = decode_with(&p, "application/x-protobuf", &body, "/", false, Some("demo.HelloRequest")).unwrap();
        assert!(d.text.contains("\"name\": \"bob\"") && d.text.contains("\"age\": 7"), "{}", d.text);
        let d = decode_with(&p, "application/x-protobuf", &body, "/", false, None).unwrap();
        assert_eq!(d.type_name, "demo.HelloRequest");
        assert!(decode_with(&p, "application/x-protobuf", &[0xff, 0xff], "/", false, None).is_none());
    }

    #[test]
    fn grpc_direction_aware() {
        let p = pool();
        let req = frame(&encode(&p, "demo.HelloRequest", &[("name", Value::String("a".into()))]));
        let res = frame(&encode(&p, "demo.HelloReply", &[("greeting", Value::String("hi".into())), ("ok", Value::Bool(true))]));
        let path = "/demo.Greeter/SayHello";
        let d = decode_with(&p, "application/grpc", &req, path, false, None).unwrap();
        assert_eq!(d.type_name, "demo.HelloRequest");
        let d = decode_with(&p, "application/grpc", &res, path, true, None).unwrap();
        assert_eq!(d.type_name, "demo.HelloReply");
        assert!(d.text.contains("\"greeting\": \"hi\""), "{}", d.text);
        let two = [res.clone(), res].concat();
        let d = decode_with(&p, "application/grpc+proto", &two, path, true, None).unwrap();
        assert!(d.text.contains("// message 2"), "{}", d.text);
        let d = decode_with(&p, "application/grpc", &req, "/unknown/Method", false, None).unwrap();
        assert_eq!(d.type_name, "demo.HelloRequest");
    }

    #[test]
    fn load_files() {
        let dir = std::env::temp_dir().join(format!("pp-desc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("a.desc");
        let file = prost_types::FileDescriptorProto { name: Some("x.proto".into()), message_type: vec![prost_types::DescriptorProto { name: Some("X".into()), ..Default::default() }], ..Default::default() };
        std::fs::write(&ok, prost_types::FileDescriptorSet { file: vec![file] }.encode_to_vec()).unwrap();
        let bad = dir.join("b.desc");
        std::fs::write(&bad, b"\xff\xff\xff").unwrap();
        let (pool, errs) = load(&[ok.display().to_string(), bad.display().to_string(), "/nonexistent.desc".into()]);
        assert!(pool.unwrap().get_message_by_name("X").is_some());
        assert_eq!(errs.len(), 2);
        std::fs::remove_dir_all(dir).ok();
    }
}
