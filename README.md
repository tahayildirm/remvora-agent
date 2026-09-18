# Remvora Agent

Repositories / Depolar: [Server](https://github.com/tahayildirm/remvora-server) · [Web](https://github.com/tahayildirm/remvora-web) · [Agent](https://github.com/tahayildirm/remvora-agent)

[English device installation](docs/GUIDE.en.md) · [Türkçe cihaz kurulumu](docs/GUIDE.tr.md)

**EN:** Rust agent for authorized, self-hosted remote terminal and desktop management. Connects to Remvora Server; controlled through Remvora Web. WebRTC P2P first, authenticated WSS relay fallback. Capabilities are explicit local opt-ins. MIT-licensed pre-release; platform/runtime limits apply.

**TR:** Yetkili, kendi sunucunuzda uzak terminal/masaüstü yönetimi için Rust agent. Remvora Server’a bağlanır, Remvora Web’den yönetilir. Önce WebRTC P2P, gerektiğinde kimlik doğrulanan WSS relay. Özellikler yerelde açık izin ister. MIT lisanslı ön sürüm; platform/runtime sınırları geçerlidir.

## Build / Derle

```sh
cargo build --release --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Install the platform dependencies in the guide first. / Önce rehberdeki platform bağımlılıklarını kurun.

## Enroll / Kayıt

Create a device/token in the panel. Set private `REMVORA_ENROLLMENT_TOKEN`, HTTPS `REMVORA_SERVER` and protected `REMVORA_STATE_DIR`, then `enroll` → approve in Web → `activate` → `--allow-terminal run`. / Panelde cihaz/token oluşturun; özel token, HTTPS sunucu ve korunan state ortamını verin. `enroll` → panelde onay → `activate` → `--allow-terminal run`.

Use the guide for exact commands, desktop/clipboard/audio/files/reboot flags, Raspberry dependencies, services, updates, removal and troubleshooting. Keep identity across upgrades. No default account, automatic public registration or universal signed installer is included.

Tam komutlar, masaüstü/pano/ses/dosya/reboot izinleri, Raspberry paketleri, servisler, güncelleme/kaldırma ve sorun giderme rehberdedir. Güncellemede kimliği koruyun. Varsayılan hesap, genel kayıt veya evrensel imzalı kurucu yoktur.

[Status / Durum](docs/STATUS.md) · [Public release / Yayın](docs/PUBLIC_RELEASE.md) · [MIT](LICENSE) · [Third-party notices](THIRD_PARTY_NOTICES.md) · [Security](SECURITY.md) · [Contributing](CONTRIBUTING.md)
