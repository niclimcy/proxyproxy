use crate::domain::config::Shared;
use hudsucker::{
    certificate_authority::{CertificateAuthority, RcgenAuthority},
    hyper::http::uri::Authority,
    rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose},
    rustls::{
        KeyLog, ServerConfig,
        crypto::aws_lc_rs::{self, sign::any_supported_type},
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
        sign::CertifiedKey,
    },
};
use std::{
    collections::HashMap,
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn dir() -> PathBuf {
    let d = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("proxyproxy");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn cert_path() -> PathBuf {
    dir().join("ca.pem")
}

pub fn load_or_create() -> Result<RcgenAuthority, Box<dyn Error>> {
    let (cert_p, key_p) = (cert_path(), dir().join("ca.key"));
    if !cert_p.exists() || !key_p.exists() {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "proxyproxy CA");
        dn.push(DnType::OrganizationName, "proxyproxy");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let cert = params.self_signed(&key)?;
        fs::write(&cert_p, cert.pem())?;
        fs::write(&key_p, key.serialize_pem())?;
    }
    let key = KeyPair::from_pem(&fs::read_to_string(&key_p)?)?;
    let issuer = Issuer::from_ca_cert_pem(&fs::read_to_string(&cert_p)?, key)?;
    Ok(RcgenAuthority::new(issuer, 1_000, aws_lc_rs::default_provider()))
}

pub fn export_path(dir: &Path) -> PathBuf {
    dir.join("ca.pem")
}

fn check_pair(cert_pem: &str, key_pem: &str) -> Result<(), Box<dyn Error>> {
    let key = KeyPair::from_pem(key_pem).map_err(|e| format!("invalid private key: {e}"))?;
    Issuer::from_ca_cert_pem(cert_pem, key).map_err(|e| format!("invalid certificate: {e}"))?;
    let cert = CertificateDer::from_pem_slice(cert_pem.as_bytes()).map_err(|e| format!("invalid certificate: {e}"))?;
    let der = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|e| format!("invalid private key: {e}"))?;
    let signer = any_supported_type(&der).map_err(|e| format!("unsupported private key: {e}"))?;
    CertifiedKey::new(vec![cert], signer).keys_match().map_err(|_| "certificate and private key do not match")?;
    Ok(())
}

// Takes effect on next start; previous files are kept as *.bak-<unix-secs>.
pub fn import_custom(dir: &Path, cert_pem: &str, key_pem: &str) -> Result<(), Box<dyn Error>> {
    check_pair(cert_pem, key_pem)?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    for name in ["ca.pem", "ca.key"] {
        let p = dir.join(name);
        if p.exists() {
            fs::rename(&p, dir.join(format!("{name}.bak-{ts}")))?;
        }
    }
    fs::write(export_path(dir), cert_pem)?;
    fs::write(dir.join("ca.key"), key_pem)?;
    Ok(())
}

pub fn import_custom_files(dir: &Path, cert: &Path, key: &Path) -> Result<(), Box<dyn Error>> {
    import_custom(dir, &fs::read_to_string(cert)?, &fs::read_to_string(key)?)
}

// NSS key log format; the path is re-read from config on every handshake secret.
pub struct KeyLogger(pub Shared);

impl std::fmt::Debug for KeyLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyLogger")
    }
}

impl KeyLog for KeyLogger {
    fn log(&self, label: &str, client_random: &[u8], secret: &[u8]) {
        let path = self.0.lock().unwrap().keylog_path.trim().to_string();
        if path.is_empty() {
            return;
        }
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{label} {} {}", hex(client_random), hex(secret));
        }
    }

    fn will_log(&self, _: &str) -> bool {
        !self.0.lock().unwrap().keylog_path.trim().is_empty()
    }
}

// Wraps RcgenAuthority to set key_log on the MITM-side ServerConfig.
pub struct KeyLogCa {
    inner: RcgenAuthority,
    log: Arc<dyn KeyLog>,
    cache: Mutex<HashMap<String, Arc<ServerConfig>>>,
}

impl KeyLogCa {
    pub fn new(inner: RcgenAuthority, log: Arc<dyn KeyLog>) -> Self {
        Self { inner, log, cache: Mutex::new(HashMap::new()) }
    }
}

impl CertificateAuthority for KeyLogCa {
    async fn gen_server_config(&self, authority: &Authority) -> Arc<ServerConfig> {
        let key = authority.to_string();
        if let Some(c) = self.cache.lock().unwrap().get(&key) {
            return c.clone();
        }
        let mut cfg = self.inner.gen_server_config(authority).await;
        Arc::make_mut(&mut cfg).key_log = self.log.clone();
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 1_000 {
            cache.clear();
        }
        cache.insert(key, cfg.clone());
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_pair() -> (String, String) {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        (params.self_signed(&key).unwrap().pem(), key.serialize_pem())
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pp-ca-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn import_valid_pair_backs_up() {
        let d = tmp("ok");
        let (old_c, old_k) = gen_pair();
        fs::write(d.join("ca.pem"), &old_c).unwrap();
        fs::write(d.join("ca.key"), &old_k).unwrap();
        let (c, k) = gen_pair();
        import_custom(&d, &c, &k).unwrap();
        assert_eq!(fs::read_to_string(export_path(&d)).unwrap(), c);
        assert_eq!(fs::read_to_string(d.join("ca.key")).unwrap(), k);
        let baks = fs::read_dir(&d).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains(".bak-")).count();
        assert_eq!(baks, 2);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn import_rejects_bad_input() {
        let d = tmp("bad");
        let (c1, _) = gen_pair();
        let (_, k2) = gen_pair();
        assert!(import_custom(&d, &c1, &k2).unwrap_err().to_string().contains("do not match"));
        assert!(import_custom(&d, "garbage", &k2).is_err());
        assert!(import_custom(&d, &c1, "garbage").is_err());
        assert!(!d.join("ca.pem").exists());
        fs::remove_dir_all(&d).unwrap();
    }
}
