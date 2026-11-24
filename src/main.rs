mod app;
mod domain;
mod proxy;
mod services;
mod viewmodels;
mod views;

// 0 would bind a random port, and 65535 leaves no room for the API on port + 1
fn valid_port(s: &str) -> Option<u16> {
    s.parse().ok().filter(|p| (1..u16::MAX).contains(p))
}

// Parses leading `--port N` flags; returns the port and the index of the first other argument.
fn port_flag(args: &[String]) -> (u16, usize) {
    let mut port = 9090u16;
    let mut i = 0;
    while let Some("--port" | "-p") = args.get(i).map(String::as_str) {
        port = args.get(i + 1).and_then(|p| valid_port(p)).unwrap_or(port);
        i += 2;
    }
    (port, i)
}

fn run_mcp(args: &[String]) -> ! {
    let (port, _) = port_flag(args);
    match services::mcp::serve_stdio(services::api::port_for(port)) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("proxyproxy mcp: {error}");
            std::process::exit(1);
        }
    }
}

fn run_cli(args: &[String]) -> ! {
    let (port, mut i) = port_flag(args);
    if args.get(i).map(String::as_str) == Some("--") {
        i += 1;
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
    let port = args.first().and_then(|p| valid_port(p)).unwrap_or(9090);
    app::run(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_without_room_for_the_api_are_rejected() {
        assert_eq!(valid_port("19090"), Some(19090));
        assert_eq!(valid_port("65534"), Some(65534));
        assert_eq!(valid_port("65535"), None);
        assert_eq!(valid_port("0"), None);
        let args: Vec<String> = ["--port", "65535"].map(String::from).to_vec();
        assert_eq!(port_flag(&args), (9090, 2));
    }
}
