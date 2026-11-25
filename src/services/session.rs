use crate::domain::flow::Flow;
use std::path::{Path, PathBuf};

pub fn autosave_path() -> PathBuf {
    crate::proxy::tls::dir().join("session.pxp")
}

pub fn save(path: &Path, flows: &[Flow]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
    serde_json::to_writer(&mut out, flows).map_err(|e| e.to_string())?;
    std::io::Write::flush(&mut out).map_err(|e| e.to_string())?;
    drop(out);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load(path: &Path) -> Result<Vec<Flow>, String> {
    let flows: Vec<Flow> = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    crate::domain::flow::reserve_ids(flows.iter().map(|f| f.id).max().unwrap_or(0));
    Ok(flows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flow::Message;

    #[test]
    fn save_load_roundtrip_reserves_ids() {
        let f = Flow {
            id: 4242,
            method: "GET".into(),
            scheme: "https".into(),
            host: "a.com".into(),
            path: "/".into(),
            version: "HTTP/1.1".into(),
            started: std::time::SystemTime::now(),
            started_at: std::time::Instant::now(),
            request: Message::default(),
            status: Some(200),
            response: Some(Message {
                headers: vec![],
                body: vec![0, 1, 2, 255],
            }),
            duration_ms: Some(3),
            tag: "t".into(),
            app: "curl".into(),
            app_id: None,
            pinned: true,
            comment: "c".into(),
            color: Some(2),
            is_ws: false,
            ws: vec![],
            error: None,
        };
        let dir = std::env::temp_dir().join(format!("pxp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.pxp");
        save(&path, &[f]).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.len(), 1);
        let b = &back[0];
        assert_eq!(
            (b.id, b.tag.as_str(), b.app.as_str(), b.pinned, b.color),
            (4242, "t", "curl", true, Some(2))
        );
        assert_eq!(b.response.as_ref().unwrap().body, vec![0, 1, 2, 255]);
        assert!(crate::domain::flow::next_id() > 4242);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn flows_saved_before_app_identity_still_load() {
        let mut v = serde_json::to_value(Flow::sample(1)).unwrap();
        v.as_object_mut().unwrap().remove("app_id");
        let f: Flow = serde_json::from_value(v).unwrap();
        assert!(f.app_id.is_none());
    }
}
