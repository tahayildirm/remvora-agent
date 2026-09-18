# Remvora Agent — uzak cihazınızı tarayıcıya bağlayın / connect your device to your browser

**TR:** Windows, Linux, macOS ve Raspberry Pi için Rust tabanlı Remvora cihaz bileşeni. Yetkili kullanıcılar kendi Remvora panelinizden uzak masaüstü, terminal, dosya aktarımı ve desteklenen sistem sesine erişir. MIT lisanslı, kendi altyapınızda çalışır.

**EN:** Remvora's Rust device component for Windows, Linux, macOS and Raspberry Pi. Authorized users access remote desktop, terminal, file transfer and supported system audio through your own Remvora panel. MIT-licensed and self-hosted.

[Server / API](https://github.com/tahayildirm/remvora-server) · [Web panel](https://github.com/tahayildirm/remvora-web) · [Agent](https://github.com/tahayildirm/remvora-agent)

| Başlangıç / Start | Türkçe | English |
|---|---|---|
| Cihaz bağımlılıkları, kurulum ve servis / Device prerequisites, setup and service | [Cihaz rehberi](docs/GUIDE.tr.md) | [Device guide](docs/GUIDE.en.md) |
| Hosting, DB ve ilk yönetici / Hosting, database and first administrator | [Server ilk kurulum](https://github.com/tahayildirm/remvora-server/blob/main/docs/FIRST_INSTALL.tr.md) | [Server first installation](https://github.com/tahayildirm/remvora-server/blob/main/docs/FIRST_INSTALL.en.md) |

## Türkçe

### Kullanım amacı

BT ve teknik destek ekipleri, okullar, kiosk yöneticileri, çok şubeli işletmeler ve ev laboratuvarları için tarayıcıdan uzak erişim. Operatör bilgisayar/tablet/telefon kullanır; bu agent **yönetilecek cihaza** kurulur. Agent API veya web sitesi değildir; kullanıcı hesabı ve veritabanı oluşturmaz.

Remvora RDP benzeri uzak masaüstü ihtiyacını karşılar; **Microsoft RDP protokolü uygulaması değildir**, mstsc ile kullanılmaz. Önce WebRTC P2P kurar; doğrudan bağlantı mümkün değilse kendi Remvora API'niz üzerinden kimlik doğrulanan WSS relay'e geçer. API ve statik panel uygun paylaşımlı hostingde çalışabilir; cihaz agenti hosting hesabına değil yönetilen bilgisayara kurulur.

### Cihazda hangi yetenekler var?

| Yetenek | Kullanım ve izin |
|---|---|
| Uzak masaüstü | H.264 görüntü, fare/klavye, desteklenen ekran seçimi; `--allow-desktop`, grafik oturumu ve OS izinleri gerekir |
| Terminal | Gerçek shell/PTY ve dizin istemi; `--allow-terminal`; agent hesabının mevcut OS yetkileriyle çalışır |
| Sistem sesi | Opus ses, `--allow-audio`; desteklenen platform arka ucu ve masaüstü izni gerekir, mikrofon modu yoktur |
| Metin panosu | `--allow-clipboard`; açık kullanıcı eylemiyle okuma/yazma, arka planda pano izleme yoktur |
| Dosya aktarımı | `--file-root` ile izinli tek klasörde iki yönlü aktarım, SHA-256 kontrolü; dosya başına 100 MiB, oturum başına 500 MiB |
| Yeniden başlatma | `--allow-reboot` ve sunucu yetkisi/teyidiyle; OS yetkisini yükseltmez |
| Kimlik ve bağlantı | Tokenla kayıt → yönetici onayı → etkinleştirme; cihaz kimliğini güncellemelerde koruma, kopmada yeniden bağlantı |
| Panel kolaylıkları | Uyarlanabilir video, P2P/WSS göstergesi, telefon dokunma/touchpad/pinch kontrolleri, tam ekran ve mobil terminal tuşları |

Yerel izinler sunucu rollerini geçersiz kılmaz; ikisi birlikte uygulanır. Dosyalar alt dizinsiz izinli paylaşım alanındadır; mevcut dosyanın üzerine yazma yoktur. Aynı agentte bir aktif uzak oturum desteklenir.

### İlk kurulum sırası

1. Önce [Server + DB + ilk Owner](https://github.com/tahayildirm/remvora-server/blob/main/docs/FIRST_INSTALL.tr.md) kurulumunu ve Web panelini tamamlayın. Uygun paylaşımlı Windows hosting için .NET 10, WSS, DB, kalıcı özel anahtar deposu ve kurulum komutu çalıştırma olanağı gerekir; VPS zorunlu değildir.
2. Cihazın işletim sistemi/mimarisine uygun bağımlılıkları [rehberden](docs/GUIDE.tr.md) kurun ve aşağıdaki komutla derleyin. Çıktı `target/release/remvora-agent` (Windows'ta `.exe`).
3. Panelde cihaz/kayıt tokenı oluşturun. API adresini `REMVORA_SERVER`, korunan kimlik dizinini `REMVORA_STATE_DIR`, tokenı gizli `REMVORA_ENROLLMENT_TOKEN` ortam değişkeniyle verin.
4. Cihazda `enroll` → panelde onay → cihazda `activate` → açıkça seçilmiş izinlerle `run`. Tam komutlar rehberdedir; API adresini kullanın, panel adresini değil.
5. Panelde gerçek çevrimiçi durumu kontrol edip Masaüstü veya Terminal açın. Sonraki operatörleri panelin **Kullanıcılar** bölümünden ekleyin; cihaz OS hesabı ile operatör hesabı farklıdır.

```sh
cargo build --release --locked
```

**Raspberry Pi'de her güncellemede yeniden kaynak derlemek gerekmez.** Mimari/ABI ile uyumlu doğrulanmış binary güncellemesi kullanılabilir; başlangıçtaki yerel sistem paketleri yine gereklidir. Evrensel, tüm bağımlılıkları içeren imzalı kurucu sunulduğu iddia edilmez. Güncellemede cihaz kimliğini silmeyin; servisi durdurup uyumlu binary'yi değiştirin ve yeniden başlatın.

### İşletim sistemi ve bağlantı sınırları

Linux/Raspberry için erişilebilir grafik oturumu ve ekran arka ucu; ses için PulseAudio/PipeWire monitor ve `parec` gerekir. macOS ekran kaydı/erişilebilirlik izinleri ister; ses yardımcısı macOS 13+ kullanır. Windows masaüstü etkileşimli kullanıcı oturumu ister; Session 0 servisi kullanıcı masaüstüne erişim sağlamaz. Ayrıntılar ve paket listeleri [cihaz rehberinde](docs/GUIDE.tr.md).

STUN adres keşfidir, relay değildir; CGNAT/firewall nedeniyle her ağda P2P garanti edilmez. WSS fallback sunucu trafiği kullanır ve sunucunun içeriği göremediği uçtan uca şifreleme değildir. Yerleşik TURN kurulumu yoktur. Platform kaynak derlemesinin başarılı olması tüm donanım/OS özelliklerinin doğrulandığı anlamına gelmez; [test durumu ve kalanlar](docs/STATUS.md) yayımlanmıştır.

## English

### Purpose

Browser-based remote access for IT/support teams, schools, kiosk administrators, distributed businesses and home labs. Operators use a computer/tablet/phone; install this agent **on the managed device**. It is not the API or website and does not create operator accounts or databases.

Remvora serves an RDP-style use case but **does not implement Microsoft's RDP protocol** or support mstsc. It attempts WebRTC P2P first and falls back to authenticated WSS relay through your own API. The API and static panel can use compatible shared hosting; the device agent runs on the managed computer, not in the hosting account.

### Capabilities

| Capability | Usage and permissions |
|---|---|
| Remote desktop | H.264 video, keyboard/mouse, supported monitor selection; `--allow-desktop`, interactive display and OS permissions |
| Terminal | Real shell/PTY and directory prompt; `--allow-terminal`; existing privileges of the agent's OS account |
| System audio | Opus sound, `--allow-audio`; supported platform backend and desktop permission, no microphone mode |
| Text clipboard | `--allow-clipboard`; explicit user read/write actions, no background clipboard polling |
| File transfer | `--file-root` for two-way transfer in a flat allowed directory, SHA-256 checking; 100 MiB/file and 500 MiB/session |
| Reboot | `--allow-reboot` plus server authorization/confirmation; no OS privilege elevation |
| Identity/connectivity | Token enrollment → administrator approval → activation; persistent identity and connection retries |
| Panel conveniences | Adaptive video, visible P2P/WSS transport, mobile touch/touchpad/pinch controls, fullscreen and terminal helper keys |

Local opt-ins and server permissions apply together. Transfers do not overwrite existing files or access arbitrary subdirectories. One active remote session per agent is supported.

### First installation

1. Install [Server + database + first Owner](https://github.com/tahayildirm/remvora-server/blob/main/docs/FIRST_INSTALL.en.md) and Web first. Compatible Windows shared hosting needs .NET 10, WSS, a database, persistent private keys and a way to execute setup commands; a VPS is not mandatory.
2. Install OS/architecture-specific prerequisites from the [guide](docs/GUIDE.en.md), then build using the command above. Output: `target/release/remvora-agent` (`.exe` on Windows).
3. Create a device/enrollment token in the panel. Configure API origin `REMVORA_SERVER`, protected identity directory `REMVORA_STATE_DIR` and private `REMVORA_ENROLLMENT_TOKEN` environment input.
4. Run `enroll` → approve in the panel → `activate` → `run` with explicitly chosen capability flags. Follow the guide for exact commands; use the API origin, not the panel origin.
5. Confirm actual online presence and open Desktop or Terminal. Add later operators through Web → **Users**; their accounts are separate from the device's OS account.

**Raspberry Pi updates do not require a full source rebuild every time.** A verified binary matching architecture/ABI can replace the previous version; initial native runtime packages are still needed. No universal signed installer bundling all dependencies is claimed. Preserve device identity, stop the service, replace the compatible binary, restart and verify a real session.

### Platform and network limits

Linux/Raspberry need an accessible graphical session and capture backend; audio needs a PulseAudio/PipeWire monitor and `parec`. macOS requires Screen Recording/Accessibility permissions; its audio helper uses macOS 13+. Windows desktop capture needs an interactive user session; a Session 0 service does not provide access to the logged-in desktop. See prerequisites and package lists in the [device guide](docs/GUIDE.en.md).

STUN discovers addresses rather than relaying media. CGNAT/firewalls can prevent direct P2P; WSS fallback uses hosting bandwidth and is not server-blind end-to-end encryption. No built-in TURN deployment is included. Successful source builds do not certify every hardware/OS feature; read the published [validation status and remaining work](docs/STATUS.md).

## Development / Geliştirme

Rust stable and platform-native toolchains/dependencies are required. / Rust stable ve platformun yerel derleme araçları/bağımlılıkları gerekir.

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

MIT-licensed pre-release; original-source licensing does not relicense dependencies. / MIT lisanslı ön sürüm; bağımlılıkların kendi lisansları korunur.

[Release checklist](docs/PUBLIC_RELEASE.md) · [MIT](LICENSE) · [Third-party notices](THIRD_PARTY_NOTICES.md) · [Security](SECURITY.md) · [Contributing](CONTRIBUTING.md)
