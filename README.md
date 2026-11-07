# Kota

AI hizmetlerinin kullanım limitlerini GNOME üst panelinde gösterir: orijinal
logolarla yüzde, halka göstergeler, sıfırlanma ve "bu hızla dolar" tahmini, son
24 saatin token grafiği ve çalışan AI ajanları (proje, bellek, portlar, durdur).

| Sağlayıcı | Giriş | Gösterilen |
| --- | --- | --- |
| Claude, Codex | CLI girişi (otomatik bulunur) | plan limitleri (%) |
| Gemini | Gemini CLI Google girişi | model başına günlük kota (%) |
| GitHub Copilot | GitHub token veya Copilot eklenti girişi | premium istekler (%) |
| Z.ai (GLM Coding Plan) | API anahtarı | 5 saatlik / aylık limit (%) |
| OpenRouter, DeepSeek, Kimi, SiliconFlow | API anahtarı | bakiye |
| OpenAI API, Anthropic API | yönetici anahtarı | bu ayki harcama (+ bütçe %) |

**Nerede görünür** (Ayarlar → Yerleşim; her biri ayrı açılır/kapanır):

- **Üst panel**: logo + yüzde (sol/orta/sağ ve sıra seçilebilir); tıklayınca ana pencere.
  Birden fazla limit varsa dile göre baş harfle hepsi: `G %14 · H %22` (TR: Gün/Hafta/Ay,
  EN: D/W/M). Görünüm → Üst panel → Pencereler.
- **Hızlı Ayarlar**: tek "Kota" kutucuğu (tıklama takibi duraklatır, ok ayrıntıları açar)
  veya sağlayıcı başına kutucuk; istenirse pil/Wi-Fi simgelerinin yanında logo + yüzde.
- **Tarih ve bildirim penceresi**: saate tıklayınca açılan pencerede kullanım kartı
  (sağ sütun üst/alt, sol sütun üst/alt).

**Canlı** bölümü klasör → alt proje → ajan ağacıdır: bir klasörde (ör. `~/YAZILIM`)
birden fazla proje varsa ajanın üzerinde çalıştığı proje, alt süreçlerin dizinlerinden
ve Claude Code oturum dökümündeki dosya yollarından bulunur.

**Diğer özellikler**

- **Kullanım geçmişi**: 24 saat / 7 gün / 30 gün grafiği ve "geçen haftadan %X fazla" karşılaştırması.
- **Ajan başına token**: her Claude Code / Codex oturumunun harcadığı token.
- **Tükenme uyarısı**: bu hızla limit, yenilenmeden önce bitecekse (varsayılan 60 dk önce) bildirim.
- **Hesap profilleri**: birden fazla Claude/Codex hesabı arasında geçiş
  (Ayarlar → Sağlayıcılar → Hesaplar ya da `kota account save|use|list`). Kimlik
  bilgileri GNOME Anahtarlığı'nda; geçişte etkin hesabın güncel token'ları önce geri kaydedilir.
- **Limite duyarlı renkler**: çubuk ve halkalar doldukça yeşil → sarı → turuncu → kırmızı.
- **Panel simgesi**: AI logosu, Kota logosu ya da Kota + AI logoları.
- **Açık / koyu tema** (sistemi izler), ayarlarda arama.

İlerleme çizgileri tempo işaretli: işaret, pencerenin geçen süresine göre eşit hızla
kullanılsaydı nerede olunacağını gösterir. Arka plan **Blur My Shell** ile uyumludur:
BMS açılır pencere bulanıklığı açıksa Kota yarı saydamlaşır ve BMS'in köşe
yarıçapını kullanır (Ayarlar → Görünüm → Arka plan).

Sağlayıcılar **Ayarlar → Sağlayıcılar**'dan açılır; anahtarlar GNOME
Anahtarlığı'nda saklanır. Hangi sağlayıcının ve hangi pencerenin (5 saat,
haftalık, model…) panelde görüneceği yine oradan seçilir.

```
Claude · Pro  ● bağlı · 23:56
  5 saatlik oturum  ███░░░░░░░░░░░░░░░░░    %14  sıfırlanma yarın 04:50 · 4 sa 54 dk sonra
  Haftalık          █▋░░░░░░░░░░░░░░░░░░     %8  sıfırlanma 13 Kas 08:00 · 6 g 8 sa sonra
  haftalık pay: Claude Code %87 · Cowork %13
  yerel log: bugün 18,9M token · bu oturumda 2,2M

Codex · Free  ● bağlı · 23:56
  30 günlük         ████████████████████   %100  sıfırlanma 3 Ara 23:11 · 26 g 23 sa sonra
```

## Mimari

```
 Claude API ─┐                         ┌─ GNOME eklentisi (panel + menü)
 Codex API  ─┤   kota daemon           │
 ~/.claude   ├─► (kota-core, Rust) ──D-Bus─┤
 ~/.codex    ┘   io.github.ensignalp.Kota  ├─ kota / kota --json
                                       └─ kota --waybar
```

- `crates/kota-core`: platformdan bağımsız çekirdek (D-Bus/GNOME bağımlılığı yok,
  ileride macOS'a taşınabilir). Sağlayıcı eklemek: `src/providers/` altına bir
  modül + `providers::build` içine bir satır.
- `crates/kota`: CLI ve D-Bus servisi (tek binary).
- `gnome-extension/kota@ensignalp`: GNOME 48–50 eklentisi. **Binary içermez**;
  yalnızca servisi dinler (extensions.gnome.org kuralı).

## Kurulum

Kota her Linux dağıtımında çalışır: servis **statik** bir ikilidir (glibc/OpenSSL
bağımlılığı yok, x86_64 ve aarch64), eklenti GNOME Shell 48–50 ister. GNOME
kullanmıyorsanız CLI ve Waybar modülü yine çalışır.

**Tek komut (her dağıtım, sudo gerekmez):**

```sh
curl -fsSL https://raw.githubusercontent.com/ensignalp/kota/main/install.sh | sh
```

`~/.local/bin/kota`, D-Bus etkinleştirme dosyası, systemd kullanıcı birimi (systemd
varsa; yoksa servis D-Bus ile kendiliğinden başlar) ve GNOME eklentisini kurar.
Kaldırmak: `… | sh -s -- --uninstall`.

**Paketler** ([Sürümler](https://github.com/ensignalp/kota/releases)):

| Dağıtım | Komut |
| --- | --- |
| Debian, Ubuntu, Mint, Pop!_OS, Zorin, elementary | `sudo apt install ./kota_<sürüm>_amd64.deb` |
| Fedora, RHEL, openSUSE | `sudo dnf install ./kota-<sürüm>-1.*.rpm ./gnome-shell-extension-kota-*.rpm` |
| Arch, Manjaro, EndeavourOS | `packaging/arch/PKGBUILD` ile `makepkg -si` |
| Diğer (Alpine, Void, Gentoo, NixOS…) | `kota-<sürüm>-<mimari>-linux.tar.gz` içinden `./install.sh` |

Paketle kurduktan sonra: `systemctl --user enable --now kota`, ardından oturumu
kapatıp açın (Wayland'da yeni eklenti ancak böyle görünür).

İsteğe bağlı: API anahtarlarını GNOME Anahtarlığı'nda saklamak için `secret-tool`
(`libsecret-tools` / `libsecret`). Claude, Codex, Gemini ve Copilot girişleri bunsuz da çalışır.

**Kaynaktan** (Rust gerekir): `make install` · `make uninstall`

## Kullanım

```sh
kota                  # özet (servis çalışıyorsa ondan, yoksa doğrudan)
kota --json           # eklentiyle aynı JSON şeması
kota --waybar         # Waybar custom modülü (return-type: json)
kota --refresh        # şimdi yeniden sorgula
kota connect claude   # giriş için terminal aç (claude auth login / codex login)
kota providers        # desteklenen sağlayıcılar (● açık, 🔑 anahtar kayıtlı)
kota enable deepseek  # / kota disable codex
kota key set deepseek # anahtarı GNOME Anahtarlığı'na kaydet (stdin'den)
kota budget openai 50 # aylık bütçe → yüzde
kota pause | resume   # düzenli sorguyu duraklat
kota agents           # çalışan AI ajanları
kota account save claude        # etkin hesabı profil olarak kaydet (ad: e-posta)
kota account use claude is      # profile geç · kota account list
kota config --example # yorumlu örnek yapılandırma
systemctl --user status kota
journalctl --user -u kota -f
```

Yapılandırma: `~/.config/kota/config.toml` (bkz. `config.example.toml`):
sorgu aralığı (varsayılan 120 sn), eşikler (%80 / %90), sağlayıcı aç/kapa,
Claude token yenileme, yerel log taraması.

Eklenti ayarları (Uzantılar → Kota): panel konumu, "her sağlayıcı / en yüksek",
kalan yüzde, panelde gizlenen sağlayıcılar, eşik bildirimleri, dil (TR/EN).

## Giriş

- **Sıfır giriş:** Claude Code / Codex zaten girişliyse Kota onların giriş
  dosyalarını (`~/.claude/.credentials.json`, `~/.codex/auth.json`) bulur.
- **Bağlı değilse:** menüdeki "Bağlan" düğmesi terminalde sağlayıcının kendi giriş
  komutunu açar. Servis dosyayı izler (5 sn) ve bağlantıyı kendiliğinden algılar.
- **Oturum süresi dolunca:** kart "Yeniden bağlan" gösterir, son bilinen değerler
  "eski veri" olarak kalır; sıfırlanma zamanı geçen pencere ~%0 olarak tahmin edilir.

## Gizlilik ve güvenlik

- Token'lar **hiçbir zaman loglanmaz, önbelleğe yazılmaz, başka yere gönderilmez.**
  Yalnızca sağlayıcının kendi API'sine (`api.anthropic.com`,
  `platform.claude.com`, `chatgpt.com`) gider.
- `~/.cache/kota/snapshot.json` yalnızca yüzdeleri ve zamanları içerir.
- Eklenti token görmez; veriyi D-Bus üzerinden JSON olarak alır.
- **Claude token yenileme:** erişim token'ı 8 saatte bir dolar. Kota, Claude
  Code'un kendi kilit protokolünü (`~/.claude/.oauth_refresh.lock` + eski kilit,
  kilit altında yeniden okuma, compare-and-swap yazma) birebir izleyerek yeniler
  ve dosyaya geri yazar; böylece Claude Code oturumu bozulmaz. Kapatmak için
  `[claude] refresh_tokens = false`. Ayrıntı: `docs/kesif.md`.
- Codex token'ı yenilenmez (10 gün geçerli, Codex kendisi yeniler).

## Uyarı

Kullanılan endpoint'ler **belgelenmemiştir** ve haber vermeden değişebilir.
Ayrıştırma toleranslıdır (bilinmeyen alanlar yok sayılır, yeni pencere türleri
otomatik gösterilir); API bozulursa yerel loglara ve son bilinen değere düşülür.

## Geliştirme

```sh
make test          # 101 test: gerçek yanıt fixture'ları, sahte HTTP sunucusuyla token yenileme/kilit/CAS
cargo clippy --workspace --all-targets
make pack          # extensions.gnome.org için zip (dist/)
make rpm           # Fedora RPM (packaging/rpm/kota.spec) · yayın: docs/YAYIN.md
```

## Yol haritası

Bitti: keşif, MVP, 11 sağlayıcı, libsecret anahtarları, ayarlardan sağlayıcı
ekleme, panel seçimi (sağlayıcı + pencere), logo/halka/yazı panel biçimleri,
tükenme tahmini, son 24 saat grafiği, çalışan ajanlar (durdur, klasör, portlar),
eşik/sıfırlanma/oturum bildirimleri, duraklatma, klavye kısayolu, TR/EN, Waybar.

Bitti (0.5): kullanım geçmişi, ajan başına token, hesap profilleri, tükenme uyarısı,
açık tema, RPM paketi.

Sırada: Antigravity ve Cursor (doğrulanabilir bir kurulumla), Flatpak.

Kota logosu: `assets/logo/` (SVG + 256/512/1024 PNG; `kota-app` yuvarlak kare uygulama
simgesi, `kota-symbolic` tek renkli panel simgesi).

Sağlayıcı logoları: [Lobe Icons](https://github.com/lobehub/lobe-icons) (MIT). Marka adları ve
logolar sahiplerinindir; yalnızca hizmeti tanıtmak için kullanılır.
