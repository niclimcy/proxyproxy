mod actions;
mod app_cache;
pub mod connector;
mod handler;
pub mod http_util;
mod pipeline;
pub mod script;
pub mod sink;
pub mod tls;

use crate::domain::config::Shared;
use handler::{Capture, WsCap, WsMap};
use hudsucker::{Proxy, rustls::crypto::aws_lc_rs};
use sink::FlowSink;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

pub async fn run(
    port: u16,
    sink: Arc<dyn FlowSink>,
    cfg: Shared,
) -> Result<(), Box<dyn std::error::Error>> {
    use hudsucker::{rustls::ClientConfig, tokio_tungstenite::Connector};
    use hyper_rustls::{ConfigBuilderExt, HttpsConnectorBuilder};
    let keylog: Arc<tls::KeyLogger> = Arc::new(tls::KeyLogger(cfg.clone()));
    let ca = tls::KeyLogCa::new(tls::load_or_create()?, keylog.clone());
    let mut tls_cfg = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_webpki_roots()
        .with_no_client_auth();
    tls_cfg.key_log = keylog;
    let tls_cfg = Arc::new(tls_cfg);
    let https = HttpsConnectorBuilder::new()
        .with_tls_config((*tls_cfg).clone())
        .https_or_http()
        .enable_http1()
        .wrap_connector(connector::upstream::UpstreamConnector::new(cfg.clone()));
    let ws: WsMap = Arc::new(Mutex::new(HashMap::new()));
    let proxy = Proxy::builder()
        .with_addr(SocketAddr::from(([127, 0, 0, 1], port)))
        .with_ca(ca)
        .with_http_connector(https)
        .with_websocket_connector(Connector::Rustls(tls_cfg))
        .with_http_handler(Capture {
            sink: sink.clone(),
            cfg,
            ws: ws.clone(),
            current: None,
            started: None,
            res_actions: vec![],
            bp_response: false,
            apps: Arc::default(),
        })
        .with_websocket_handler(WsCap { sink, map: ws })
        .build()?;
    proxy.start().await?;
    Ok(())
}
