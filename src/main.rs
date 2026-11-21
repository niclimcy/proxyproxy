mod app;
mod domain;
mod proxy;
mod services;
mod viewmodels;
mod views;

fn run_mcp(args: &[String]) -> ! {
    let mut port = 9090u16;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                port = args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(port);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }
    match services::mcp::serve_stdio(port.wrapping_add(1)) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("proxyproxy mcp: {error}");
            std::process::exit(1);
        }
    }
}

fn run_cli(args: &[String]) -> ! {
    let mut port = 9090u16;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                port = args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(port);
                i += 2;
            }
            "--" => {
                i += 1;
                break;
            }
            _ => break,
        }
    }
    let Some(cmd) = args.get(i) else {
        eprintln!("usage: proxyproxy run [--port N] -- <command> [args...]");
        std::process::exit(2);
    };
    if std::net::TcpStream::connect_timeout(
        &([127, 0, 0, 1], port).into(),
        std::time::Duration::from_millis(300),
    )
    .is_err()
    {
        eprintln!("warning: nothing is listening on 127.0.0.1:{port} — start proxyproxy first");
    }
    let url = format!("http://127.0.0.1:{port}");
    let cert = proxy::tls::cert_path().display().to_string();
    let mut c = std::process::Command::new(cmd);
    c.args(&args[i + 1..]);
    for k in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        c.env(k, &url);
    }
    c.env("NO_PROXY", "localhost,127.0.0.1,::1")
        .env("no_proxy", "localhost,127.0.0.1,::1")
        .env("NODE_USE_ENV_PROXY", "1");
    for k in [
        "NODE_EXTRA_CA_CERTS",
        "SSL_CERT_FILE",
        "REQUESTS_CA_BUNDLE",
        "CURL_CA_BUNDLE",
        "GIT_SSL_CAINFO",
        "DENO_CERT",
        "AWS_CA_BUNDLE",
        "CARGO_HTTP_CAINFO",
    ] {
        c.env(k, &cert);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = c.exec();
        eprintln!("proxyproxy run: {cmd}: {e}");
        std::process::exit(127);
    }
    #[cfg(not(unix))]
    match c.status() {
        Ok(s) => std::process::exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("proxyproxy run: {cmd}: {e}");
            std::process::exit(127)
        }
    }
}

fn main() -> eframe::Result {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("mcp") {
        run_mcp(&args[1..]);
    }
    if args.first().map(|s| s.as_str()) == Some("run") {
        run_cli(&args[1..]);
    }
    let port: u16 = args.first().and_then(|p| p.parse().ok()).unwrap_or(9090);
    app::run(port)
}
