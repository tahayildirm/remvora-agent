# Remvora Agent — cihaz kurulum rehberi

[Türkçe](GUIDE.tr.md) · [English](GUIDE.en.md)

## Cihazda ne çalışır?

Bu Rust süreç, yönetmeye yetkili olduğunuz cihazı kendi Remvora Server’ınıza bağlar. Operatör Web panelini kullanır; agent terminal, masaüstü/girdi ve yerelde izin verilen pano/ses/dosya/reboot sağlar. API değildir, kullanıcı hesabı oluşturmaz, gizli izleme ürünü değildir. BT ekipleri, okullar, kiosk yöneticileri, destek ekipleri ve ev laboratuvarları içindir. Önce ayrı Server ve Web depolarını kurun. Açık kaynak depoları: [Server](https://github.com/tahayildirm/remvora-server) · [Web](https://github.com/tahayildirm/remvora-web) · [Agent](https://github.com/tahayildirm/remvora-agent).

Server rehberi veritabanı ve ilk Owner’ı açıklar: migration, ardından boş kurulumda bootstrap. Sonraki kullanıcılar Web → Kullanıcılar’dan e-posta, başlangıç parolası (8–256) ve rolle açılır. Varsayılan hesap yoktur. Operatör hesabı, agentin işletim sistemi hesabından farklıdır.

## Platform ve çalışma gereksinimleri

Kaynak derleme için Rust stable (edition 2024) ve native araç zinciri gerekir. Güncel geliştirmede Rust 1.98 kullanıldı; lockfile ile kendi zincirinizi doğrulayın. Arşiv tüm OS kütüphanelerini içeriyor kabul edilmez.

- **Linux/Raspberry Pi:** gerçek CPU mimarisine ve ABI’ye derleyin. CI geliştirme paketleri: clang, libclang-dev, libpipewire-0.3-dev, libxcb1-dev, libxrandr-dev, libxtst-dev, libxkbcommon-dev, libgbm-dev, libdrm-dev, libegl1-mesa-dev, libasound2-dev, libwayland-dev, cmake. Paket adları dağıtıma göre değişebilir. Masaüstü erişilebilir grafik oturumu ister; Wayland compositor/izinleri önemlidir. Ses için `/usr/bin/parec` ve PulseAudio/PipeWire monitor gerekir. Yalnız PTY açık masaüstü istemez, ancak binary’nin bağlı runtime kütüphaneleri yine gerekir.
- **macOS:** Xcode command-line tools; görüntü/girdi için Screen Recording ve Accessibility izinleri. Sistem sesi Swift/ScreenCaptureKit yardımcısı kullanır (macOS 13+). İlgili oturum açmış kullanıcıda çalıştırın; başsız LaunchDaemon kullanmayın.
- **Windows:** Visual Studio C++ araçları ve uygun Rust hedefi. Masaüstü etkileşimli oturum ister; Session 0 servisi kullanıcının masaüstünü kontrol etmenin kestirme yolu değildir. Kimlik koruması hesaba bağlı DPAPI’dir. Windows runtime/installer kabulü henüz tamamlanmadı.

Seçilmiş macOS ARM64 ve Linux ARM64/Raspberry akışları test edildi; tam platform matrisi değil. İmzalı/notarize genel kurulum paketleri henüz sunulmuyor. OS izinlerini cihaz yöneticisi vermelidir; testi geçirmek için OS korumalarını kapatmayın.

```sh
cargo build --release --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Binary `target/release/remvora-agent` (Windows `.exe`) olur. Kaynaktan derleyin veya yayımlandığında doğrulanmış uyumlu sürümü kullanın. Checksum/imza, CPU ve native bağımlılıkları kontrol edin. Raspberry’de ilk kaynak derleme uzun sürebilir; sonraki uyumlu binary güncellemeleri tüm araç zincirini tekrar derlemez. Eksik runtime paketleri yine bir kez kurulur. Her bağımlılığı içeren evrensel installer iddiası yoktur.

## Kayıt → onay → etkinleştirme → çalıştırma

En az yetkili ayrı OS hesabı ve mutlak, korunan state dizini kullanın. Unix örneği; Windows’ta yolları uyarlayın. Tüm adımları aynı hesapla yapın:

```sh
mkdir -p "$HOME/.local/state/remvora"
chmod 700 "$HOME/.local/state/remvora"
export REMVORA_SERVER=https://remvora.example/
export REMVORA_STATE_DIR="$HOME/.local/state/remvora"
```

Panelde cihaz oluşturup kayıt tokeni alın. Tokeni cihazın `REMVORA_ENROLLMENT_TOKEN` süreç ortamına özel yolla verin; gerçek tokeni genel belgeye veya shell geçmişine yazmayın.

```sh
./target/release/remvora-agent enroll
unset REMVORA_ENROLLMENT_TOKEN
# Yönetici panelden bekleyen cihazı onaylar, sonra:
./target/release/remvora-agent activate
./target/release/remvora-agent --allow-terminal run
```

Token varsayılan 10 dakika geçerlidir; süresi dolduysa yeni token alıp ilgili adımı tekrarlayın. Kimliği sürekli silmeyin. Özel anahtar cihazda üretilir; güncellemede state korunur. Bir kimliği birden fazla cihaza kopyalamayın. Unix’te özel izinler zorlanır, doğrudan symlink reddedilir; Windows’ta DPAPI için kayıt/çalıştırma aynı hesap olmalıdır.

Sunucu URL’si, ayrı subdomain varsa panel değil **API origin**’idir. Sonda `/` ve güvenilir HTTPS kullanın. Redirect reddedilir. `--allow-loopback-http` yalnız yalıtılmış loopback testi içindir; genel HTTP’ye izin vermez. Özel CA için `--ca-certificate /absolute/path/ca.pem` veya `REMVORA_CA_CERTIFICATE` kullanın; hostname/sertifika kontrolü devam eder.

## Özellik izinleri

Flag’ler `run` öncesine yazılır:

| Flag | Etki ve sınır |
|---|---|
| `--allow-terminal` | Agent hesabı yetkilerinde gerçek shell/PTY; Unix `/bin/sh`, `--shell /absolute/path` ile seçilebilir |
| `--allow-desktop` | OS izni ve etkileşimli ekran koşuluyla görüntü ve klavye/fare |
| `--allow-clipboard` | Açık eylemli metin panosu; desktop gerekir; 16 KiB |
| `--file-root /absolute/shared` | Var olan korunan düz paylaşım klasörü; desktop gerekir, tüm diske erişim vermez |
| `--allow-audio` | Sistem çıkış sesi; desktop/platform bağımlılığı; mikrofon modu yok |
| `--allow-reboot` | Sunucu yetki/onayından sonra sabit native reboot; yetki yükseltme yok |
| `--monitor-id ID` | İlk ekran ID’si; bulunamazsa başka ekrana sessizce geçmez |

Paylaşım dizinini oluşturup koruduktan sonra:

```sh
./target/release/remvora-agent --allow-terminal --allow-desktop \
  --allow-clipboard --file-root /absolute/shared --allow-audio run
./target/release/remvora-agent list-displays
```

Flag değiştirmeden mevcut foreground agenti durdurun; ikinci servis başlatmayın. Yerel izin sunucu rolünü aşmaz. Aynı anda tek uzak oturum mevcut modeldir. Yetki/signaling kaybolunca oturum kapanır; yeniden bağlantı beklemeli tekrar dener.

## Kalıcı servis

Linux grafik kullanıcı servisi (önce enroll/activate):

```sh
python3 scripts/install-linux-user.py \
  --server https://remvora.example/ \
  --binary /absolute/path/remvora-agent \
  --state /absolute/private/state \
  --allow-terminal --allow-desktop --enable
systemctl --user status remvora-agent
journalctl --user -u remvora-agent -n 100
```

DISPLAY, WAYLAND_DISPLAY, XDG_RUNTIME_DIR ve DBUS doğru olsun diye ilgili grafik hesap/oturumunda çalıştırın. Kurucu mevcut unit’i ezmez; mevcut servisi ayarlarını koruyarak elle inceleyin. Başka kiosk servisini değiştirmeyin. Login öncesi/headless erişim platform tasarımı ister; yalnız linger açmak yeterli değildir. `deploy/remvora-agent.service.example` gözden geçirilerek kullanılacak sistem servisi örneğidir.

macOS: `deploy/com.remvora.agent.plist.example` içindeki binary/state/server yollarını değiştirin; aynı kullanıcıyla kayıt olup LaunchAgent yükleyin ve OS izinlerini verin. Windows: `--service`, SCM `RemvoraAgent` ile bütünleşir; kayıt/DPAPI hesabıyla eşleşen ayrı hesap ve incelenmiş servis argümanları kullanın. Servis modu Session 0 masaüstü sınırını kaldırmaz. Native installer/imza ayrı yayın koşuludur; scripts/şablonları inceleyin.

## Ağ ve kullanım

`REMVORA_STUN_URL` için işletmecinin onayladığı STUN/STUNS URI; tarayıcı için sunucuda `WebRtc:StunServers` verin. STUN keşiftir, medya relay’i değildir. Otomatik WebRTC ICE adaylarını dener, doğrudan bağlantı olmazsa WSS relay kullanır. Yerleşik TURN kurulumu yoktur. Her uçta dışarı HTTPS/WSS gerekir; P2P ayrıca UDP/NAT politikasına bağlıdır. WSS sunucu üzerinden gider ve trafik tüketir; TLS hop bazındadır, sunucuya karşı uçtan uca gizli değildir.

Panel connecting/P2P/relay ve canlı presence’ı kayıt durumundan ayrı gösterir. Masaüstünde adaptif kalite, cihaz/tarayıcıya kaydedilen tercihler, ekran seçimi, opsiyonel ses, açık pano/dosya vardır. Mobilde doğrudan dokunma, isteğe bağlı touchpad, pinch/pan, açılır araçlar ve desteklenirse tam ekran bulunur. Terminal mobil metin/özel tuşlar ve viewport’a göre satır/sütun uyarlaması sunar.

Video H.264, ses Opus’tur. Pano arka planda okunmaz. Dosya düz paylaşım klasörüdür: 100 MiB/dosya, 500 MiB/oturum, tek aktarım, SHA-256, üzerine yazma/path kaçışı/symlink yok. Tarayıcıdan dosya yapıştırmak OS dosya panosu değildir. Tarayıcı kısayolları ve güvenli OS tuş dizileri çalışmayabilir. [Özellik ayrıntıları](remote-capabilities.md).

## Güncelleme, kurtarma, kaldırma

Kimlik/anahtar, seçilmiş izinler ve paylaşılan dosyaları koruyun. Yalnız Remvora’yı durdurun, hedef/checksum/imzayı kontrol edin, binary yedeği alın, uyumlu binary’yi değiştirin, başlatıp presence ve gerçek oturum deneyin. `stage-update`/`install-update` manifest, imza, artifact ve güvenilen anahtar alır; otomatik güncelleme keşfedip indirmez. `--help` ve [yayın/update belgesini](release-and-update.md) inceleyin. İmza güvenini uydurmayın; imzasız geliştirme ZIP’ini üretim installer’ı diye sunmayın.

Servis bozulursa logları inceleyip binary/ayarı geri alın; ilk iş kimlik silmeyin. Kullanım sonlandırmada sunucu güvenini iptal edin/devre dışı bırakın, Remvora servisini durdurun/kapatın, yedekten sonra yalnız ona ait binary/unit’i kaldırın. Özel kimliğin yok edilmesi ayrıca geri alınamaz işlemdir. OS sıfırlama/state kaybı genellikle yeniden kayıt ister; anahtarı ikinci aktif cihaza geri yüklemeyin.

Sorun kontrolü: çevrimdışı → servis/WSS/TLS/state; siyah ekran → login/display/izin; ses yok → flag/monitor/backend/tarayıcı sessizliği; dosya reddi → dizin/ad/boyut; meşgul → önceki oturum; yavaş → iki ağ, CPU/encode ve relay kapasitesi. Raspberry derleme hatasında SD/depolama ve paket bütünlüğünü inceleyin; rutin çözüm olarak format atmayın.

## Lisans ve yayın sınırları

MIT özgün Remvora koduna uygulanır; bağımlılıkların topluca yeniden lisanslanması değildir. OpenH264, Opus, xcap, Enigo ve diğer bildirimleri koruyun. Tam SBOM/lisans incelemesi, native platform testleri, uzun süreli güvenilirlik/güvenlik ve imza/notarizasyon genel binary yayınının açık koşullarıdır. [Durum](STATUS.md), [yayın kontrolü](PUBLIC_RELEASE.md), SECURITY ve CONTRIBUTING’i okuyun. Hosting/destek maliyeti ve garantili SLA dahil değildir.
