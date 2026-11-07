//! `kota`: AI kullanım limitleri için CLI ve D-Bus servisi.
//!
//! ```text
//! kota                 insan okunur özet
//! kota --json          JSON anlık görüntü (GNOME eklentisiyle aynı şema)
//! kota --waybar        Waybar özel modülü
//! kota daemon          D-Bus servisi (systemd kullanıcı birimi çalıştırır)
//! kota connect claude  giriş için terminal aç
//! kota config          yapılandırma yolu ve geçerli ayarlar
//! kota providers       desteklenen sağlayıcılar ve durumları
//! kota enable deepseek · kota disable codex
//! kota key set deepseek · kota key clear deepseek
//! kota budget openai 50 · kota pause · kota resume · kota agents
//! kota account list · save claude [ad] · use claude <ad> · remove claude <ad>
//! ```

mod accounts;
mod agents;
mod connect;
mod daemon;
mod dbus;
mod focus;
mod i18n;
mod keyring;
mod render;

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use kota_core::config::Config;
use kota_core::engine::Engine;
use kota_core::model::Snapshot;

use crate::i18n::Lang;

#[derive(Parser)]
#[command(
    name = "kota",
    version,
    about = "AI kullanım limitleri: Claude, Codex, Gemini, Copilot, DeepSeek, OpenRouter… yüzde, sıfırlanma, bakiye ve çalışan ajanlar"
)]
struct Cli {
    /// Çıktıyı JSON olarak yaz.
    #[arg(long, conflicts_with = "waybar")]
    json: bool,
    /// Waybar `custom` modülü için JSON yaz.
    #[arg(long)]
    waybar: bool,
    /// Servisi kullanma; sağlayıcıları doğrudan sorgula.
    #[arg(long)]
    direct: bool,
    /// Servisin son değeri yerine şimdi yeniden sorgula.
    #[arg(long)]
    refresh: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// D-Bus servisini çalıştır (genellikle systemd başlatır).
    Daemon,
    /// Sağlayıcıya giriş için terminal aç: claude | codex
    Connect { provider: String },
    /// Yapılandırma dosyasının yolunu ve geçerli ayarları göster.
    Config {
        /// Yorumlu örnek yapılandırmayı yaz.
        #[arg(long)]
        example: bool,
    },
    /// Desteklenen sağlayıcıları listele.
    Providers,
    /// Sağlayıcıyı aç.
    Enable { provider: String },
    /// Sağlayıcıyı kapat.
    Disable { provider: String },
    /// API anahtarını anahtarlıkta sakla / sil (anahtar stdin'den okunur).
    Key {
        #[arg(value_parser = ["set", "clear"])]
        action: String,
        provider: String,
    },
    /// Aylık bütçe (harcama gösteren sağlayıcılar; 0 kaldırır).
    Budget { provider: String, amount: f64 },
    /// Düzenli sorguyu duraklat.
    Pause,
    /// Düzenli sorguyu sürdür.
    Resume,
    /// Çalışan AI ajanlarını listele.
    Agents,
    /// Hesap profilleri (Claude, Codex): list | save <sağlayıcı> [ad] | use <sağlayıcı> <ad> | remove <sağlayıcı> <ad>
    Account {
        #[arg(value_parser = ["list", "save", "use", "remove"])]
        action: String,
        provider: Option<String>,
        name: Option<String>,
    },
}

fn init_logging(default: &str) {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default))
        .format_timestamp(None)
        .init();
}

#[tokio::main]
async fn main() -> ExitCode {
    // `kota providers | head` gibi kapanan borularda panik yerine sessizce çık.
    // SAFETY: tek iş parçacıklı başlangıçta sinyal davranışını varsayılana döndürür.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    match cli.command {
        Some(Cmd::Daemon) => {
            init_logging("info");
            match daemon::run().await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    log::error!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Cmd::Connect { ref provider }) => {
            init_logging("warn");
            match connect::open_login_terminal(provider) {
                Ok(()) => {
                    eprintln!("{}", Lang::detect().terminal_opened(provider));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("kota: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Cmd::Config { example }) => {
            if example {
                print!("{}", Config::example());
                return ExitCode::SUCCESS;
            }
            let path = kota_core::paths::config_file();
            let state = if path.exists() {
                ""
            } else {
                "  (yok; varsayılanlar kullanılıyor)"
            };
            println!("# {}{state}", path.display());
            match Config::load_from(&path) {
                Ok(c) => {
                    print!("{}", c.to_toml());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("kota: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Cmd::Providers) => {
            let cfg = Config::load().0;
            let lang = Lang::detect();
            for e in kota_core::providers::catalog() {
                let on = cfg.is_enabled(e.id);
                let key = match e.auth {
                    kota_core::providers::AuthKind::CliLogin => String::new(),
                    _ if keyring::has_key(e.id) => "  🔑".into(),
                    _ => String::new(),
                };
                println!(
                    "{} {:<12} {:<18} {}{key}",
                    if on { "●" } else { "○" },
                    e.id,
                    e.name,
                    lang.shows(e.shows)
                );
            }
            ExitCode::SUCCESS
        }
        Some(Cmd::Enable { ref provider }) => manage(|c| c.set_enabled(provider, true), provider),
        Some(Cmd::Disable { ref provider }) => manage(|c| c.set_enabled(provider, false), provider),
        Some(Cmd::Budget { ref provider, amount }) => manage(|c| c.set_budget(provider, Some(amount)), provider),
        Some(Cmd::Pause) => manage(|c| c.paused = true, "claude"),
        Some(Cmd::Resume) => manage(|c| c.paused = false, "claude"),
        Some(Cmd::Key {
            ref action,
            ref provider,
        }) => {
            if !kota_core::providers::catalog().iter().any(|e| e.id == provider) {
                eprintln!("kota: unknown provider {provider} (see `kota providers`)");
                return ExitCode::FAILURE;
            }
            let key = if action == "set" {
                if std::io::stdin().is_terminal() {
                    eprint!("{provider} API key: ");
                }
                let mut line = String::new();
                if std::io::stdin().read_line(&mut line).is_err() || line.trim().is_empty() {
                    eprintln!("kota: no key given");
                    return ExitCode::FAILURE;
                }
                line.trim().to_string()
            } else {
                String::new()
            };
            // Servis çalışıyorsa onun üzerinden (önbelleğini de temizler).
            let res = match dbus::proxy().await {
                Some(p) => p.set_api_key(provider, &key).await.map_err(|e| e.to_string()),
                None => keyring::store(provider, &key),
            };
            match res {
                Ok(()) => {
                    eprintln!("ok");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("kota: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Cmd::Account { ref action, ref provider, ref name }) => {
            let res = match (action.as_str(), provider.as_deref(), name.as_deref()) {
                ("list", _, _) => {
                    let list = accounts::list();
                    if list.is_empty() {
                        println!("—  (kota account save claude)");
                    }
                    for p in list {
                        println!("{} {:<8} {}", if p.active { "●" } else { " " }, p.provider, p.name);
                    }
                    Ok(())
                }
                ("save", Some(p), n) => accounts::save(p, n).map(|n| println!("saved: {p}/{n}")),
                ("use", Some(p), Some(n)) => accounts::switch(p, n).map(|_| println!("active: {p}/{n}")),
                ("remove", Some(p), Some(n)) => accounts::remove(p, n),
                _ => Err("usage: kota account list | save <provider> [name] | use <provider> <name> | remove <provider> <name>".into()),
            };
            match res {
                Ok(()) => {
                    if action == "use"
                        && let Some(proxy) = dbus::proxy().await
                    {
                        let _ = proxy.refresh().await;
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("kota: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Cmd::Agents) => {
            let agents = match dbus::fetch(false).await {
                Ok(s) if !s.agents.is_empty() => s.agents,
                _ => agents::AgentScanner::new().scan(chrono::Utc::now()),
            };
            if agents.is_empty() {
                println!("—");
            }
            for a in agents {
                let ports = if a.ports.is_empty() {
                    String::new()
                } else {
                    format!(
                        "  :{}",
                        a.ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(" :")
                    )
                };
                println!(
                    "{:>7}  {:<14} {:<22} {:>7}{ports}",
                    a.pid,
                    a.name,
                    a.project_path,
                    format!("{} MB", a.rss_bytes / 1024 / 1024)
                );
            }
            ExitCode::SUCCESS
        }
        None => {
            init_logging("warn");
            show(&cli).await
        }
    }
}

/// config.toml'u değiştirir; çalışan servis dosyayı izlediği için 5 sn içinde uygular.
fn manage(f: impl FnOnce(&mut Config), provider: &str) -> ExitCode {
    if !kota_core::providers::catalog().iter().any(|e| e.id == provider) {
        eprintln!("kota: unknown provider {provider} (see `kota providers`)");
        return ExitCode::FAILURE;
    }
    let path = kota_core::paths::config_file();
    let mut cfg = match Config::load_from(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("kota: {e}");
            return ExitCode::FAILURE;
        }
    };
    f(&mut cfg);
    match cfg.save_to(&path) {
        Ok(()) => {
            eprintln!("ok ({})", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("kota: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn show(cli: &Cli) -> ExitCode {
    let lang = Lang::detect();
    let snap = obtain(cli.direct, cli.refresh).await;
    let now = chrono::Local::now().fixed_offset();
    if cli.json {
        match serde_json::to_string_pretty(&snap) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("kota: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else if cli.waybar {
        println!("{}", render::waybar(&snap, lang, now));
    } else {
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        print!("{}", render::human(&snap, lang, color, now));
    }
    ExitCode::SUCCESS
}

async fn obtain(direct: bool, refresh: bool) -> Snapshot {
    if !direct {
        match dbus::fetch(refresh).await {
            Ok(s) => return s,
            Err(e) => log::debug!("daemon unavailable, querying directly: {e}"),
        }
    }
    let (config, warning) = Config::load();
    if let Some(w) = warning {
        eprintln!("kota: config ignored: {w}");
    }
    let mut snap = Engine::with_keys(config, std::sync::Arc::new(keyring::SecretToolKeys::new()))
        .refresh(None)
        .await;
    snap.agents = agents::AgentScanner::new().scan(chrono::Utc::now());
    snap
}
