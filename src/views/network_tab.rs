use crate::domain::config::UpstreamMode;
use crate::viewmodels::intent::NetworkIntent;
use crate::viewmodels::network_vm::{NetworkVm, valid_ip};
use crate::views::theme::*;
use eframe::egui::{self, RichText, TextEdit};
use egui_phosphor::regular as ph;

pub fn show(ui: &mut egui::Ui, vm: &NetworkVm) -> Vec<NetworkIntent> {
    let mut out = vec![];
    {
        let mut cfg = vm.config();
        ui.label(RichText::new("DNS overrides").strong());
        ui.label(muted("Host (wildcards ok) → IP for upstream connections. Applies to decrypted/plain requests, not passthrough tunnels."));
        for (i, (host, ip)) in cfg.dns_overrides.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(TextEdit::singleline(host).desired_width(220.0).hint_text("api.example.com"));
                ui.label("→");
                ui.add(TextEdit::singleline(ip).desired_width(140.0).hint_text("127.0.0.1"));
                if !valid_ip(ip) {
                    ui.label(RichText::new("invalid IP").color(danger()));
                }
                if ui.small_button(ph::X).clicked() {
                    out.push(NetworkIntent::RemoveDns(i));
                }
            });
        }
        if ui.button(ico(ph::PLUS, "Add override")).clicked() {
            out.push(NetworkIntent::AddDns);
        }

        ui.separator();
        ui.label(RichText::new("Upstream proxy").strong());
        segmented(ui, &mut cfg.upstream.mode, &[(UpstreamMode::Off, "Direct"), (UpstreamMode::Manual, "HTTP"), (UpstreamMode::Socks, "SOCKS5"), (UpstreamMode::Pac, "PAC file")]);
        match cfg.upstream.mode {
            UpstreamMode::Off => {}
            UpstreamMode::Manual => {
                ui.horizontal(|ui| {
                    ui.label("Proxy");
                    ui.add(TextEdit::singleline(&mut cfg.upstream.url).desired_width(280.0).hint_text("host:port or http://user:pass@host:port"));
                });
            }
            UpstreamMode::Socks => {
                ui.horizontal(|ui| {
                    ui.label("Proxy");
                    ui.add(TextEdit::singleline(&mut cfg.upstream.url).desired_width(280.0).hint_text("host:port or socks5://user:pass@host:port"));
                });
            }
            UpstreamMode::Pac => {
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut cfg.upstream.pac_path).desired_width(280.0).hint_text("/path/to/proxy.pac"));
                    if ui.button("Browse…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("PAC", &["pac", "js", "txt"]).pick_file() {
                            out.push(NetworkIntent::SetPac(p.display().to_string()));
                        }
                    }
                });
            }
        }

        ui.separator();
        ui.label(RichText::new("TLS key log").strong());
        ui.label(muted("Appends NSS-format secrets for both proxy legs (Wireshark: Preferences → TLS → Pre-Master-Secret log). Empty = off."));
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut cfg.keylog_path).desired_width(280.0).hint_text("/tmp/sslkeys.log"));
            if ui.button("Browse…").clicked() {
                if let Some(p) = rfd::FileDialog::new().save_file() {
                    out.push(NetworkIntent::SetKeylog(p.display().to_string()));
                }
            }
        });
    }

    ui.separator();
    ui.label(RichText::new("Custom CA").strong());
    ui.label(muted("Replace the root CA with your own certificate + key (PEM). Old files are backed up; restart to apply, then re-trust the CA."));
    if ui.button(ico(ph::UPLOAD_SIMPLE, "Import CA certificate + key…")).clicked() {
        let cert = rfd::FileDialog::new().set_title("CA certificate (PEM)").pick_file();
        let key = cert.as_ref().and_then(|_| rfd::FileDialog::new().set_title("CA private key (PEM)").pick_file());
        if let (Some(c), Some(k)) = (cert, key) {
            out.push(NetworkIntent::ImportCa(c, k));
        }
    }
    out
}
