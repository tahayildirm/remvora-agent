# Public release / Genel yayın

## English

The three components are published as separate public MIT source repositories: [Server](https://github.com/tahayildirm/remvora-server) · [Web](https://github.com/tahayildirm/remvora-web) · [Agent](https://github.com/tahayildirm/remvora-agent). Private vulnerability reporting is enabled. This is a source preview, not a signed binary release. Do not publish the private deployment directories or copy the local workspaces wholesale.

Checklist for subsequent releases:

1. Run `git status --short --ignored` and `git ls-files --cached --others --exclude-standard`. Review every candidate, including scripts and test fixtures. Ignoring a tracked file does not remove it from history.
2. Scan the staged tree and all Git history with a dedicated secret scanner. Inspect results privately; do not post secrets in reports. Rotate any previously exposed credentials. Never commit production settings, keys, identity state, bootstrap input, logs, database backups, deployment ZIPs or browser traces.
3. Keep LICENSE and THIRD_PARTY_NOTICES. Review the full locked dependency/license inventory and shipped native libraries. Source MIT licensing does not certify binary redistribution compliance.
4. Run this component’s tests and CI on supported hosts. Record actual results, known failures and platform-specific runtime requirements. Do not describe an unsigned development archive as a signed installer or fully production-certified release.
5. Enable private vulnerability reporting; configure the actual private contact before announcing the repository. Review branch protection and least-privilege CI permissions. Do not use public Actions secrets for untrusted pull requests.
6. Publish sanitized example configuration only. Add verified repository links, version/tag, checksums, platform artifacts and installation instructions. A clean-machine installation must verify bootstrap, second user, device enrollment, terminal/desktop and backup recovery.
7. Add a feature/license inventory and screen images without customer data. Check logo redistribution rights, code-signing/notarization, and third-party notices before distributing installable releases.

Open items: versioned binary release, release SBOM, signed platform installers, clean-machine and full platform/browser acceptance. No universal P2P or production-readiness claim.

## Türkçe

Üç bileşen ayrı, herkese açık MIT kaynak depolarında yayımlanır: [Server](https://github.com/tahayildirm/remvora-server) · [Web](https://github.com/tahayildirm/remvora-web) · [Agent](https://github.com/tahayildirm/remvora-agent). Özel güvenlik bildirimi etkindir. Bu bir kaynak ön sürümüdür; imzalı binary sürümü değildir. Özel kurulum klasörlerini veya tüm çalışma alanını topluca yayımlamayın.

Sonraki sürümler için kontrol listesi:

1. `git status --short --ignored` ve `git ls-files --cached --others --exclude-standard` ile aday dosyaları inceleyin. Takip edilen dosyayı ignore etmek Git geçmişinden kaldırmaz.
2. Stage ve tüm Git geçmişini özel bir secret scanner ile tarayın. Bulguları gizli tutun; rapora parola yazmayın. Daha önce açığa çıkan sırları değiştirin. Üretim ayarları, kimlik anahtarları, bootstrap girdileri, loglar, yedekler, ZIP’ler ve tarayıcı izleri Git’e girmez.
3. LICENSE ve üçüncü taraf bildirimlerini koruyun. Kilitli bağımlılıkların ve native kütüphanelerin lisanslarını inceleyin. Kaynak kodun MIT olması, binary dağıtımının tüm lisans şartlarının tamamlandığını göstermez.
4. Her bileşenin testlerini ve platform CI işlerini çalıştırın; gerçek sonuçları/sınırları yazın. İmzasız geliştirme paketini imzalı veya her platformda production onaylı diye sunmayın.
5. Özel güvenlik bildirimi kanalını açın; duyurudan önce gerçek iletişim kanalını belirleyin. Dal korumasını ve CI izinlerini kontrol edin; güvenilmeyen PR’lara sır vermeyin.
6. Yalnız temiz örnek ayarları yayımlayın. Gerçek depo bağlantıları, sürüm, checksum ve kurulum belgeleri ekleyin. Temiz makinede ilk yönetici, ikinci kullanıcı, cihaz kaydı, terminal/masaüstü ve yedekten dönüşü deneyin.
7. Müşteri verisi içermeyen görseller ekleyin. Kurulabilir sürüm öncesi logo dağıtım hakkını, kod imzasını/notarizasyonu ve bağımlılık bildirimlerini doğrulayın.

Kalanlar: sürümlenmiş binary yayını, SBOM, imzalı kurulum paketleri, temiz makine ve tam platform/tarayıcı kabul testleri. Her ağda P2P veya koşulsuz production hazırlığı iddiası yoktur.
