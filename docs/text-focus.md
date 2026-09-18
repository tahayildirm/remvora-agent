# Text focus detection / Yazı alanı algılama

Experimental; requires a matching agent and web update. Raspberry GTK3/X11 metadata detection was verified with a real editable field and a non-text button. End-to-end native mobile keyboard and Windows acceptance remain pending.

For mobile layouts the browser also polls `text.regions` once per second over the existing authorized desktop channel. Linux returns up to 32 visible editable/terminal rectangles in the active window. The browser expires them after 1.5 seconds and can focus the native keyboard synchronously during a tap, without a visible input panel. Windows currently returns the focused editable rectangle only; complete Windows region detection is not claimed.

During an authorized desktop session, the browser sends `text.focus` with a numeric nonce after a direct tap. The agent queries accessibility metadata after the click and responds with the same nonce and `bounds: [left, top, right, bottom]` normalized to the selected screen, or null. It does not read text, passwords, names, or field values. Both P2P and WSS reuse the same handler. Queries run outside the input/video loop, one at a time, with a two-second helper deadline and a 300ms minimum interval.

Linux requires `/usr/bin/python3`, PyGObject and AT-SPI (`python3-gi`, `gir1.2-atspi-2.0`, `at-spi2-core` on Debian-family distributions), the user's accessibility D-Bus session, and an application exposing focused editable controls. Accessible terminal roles are also accepted. The helper does not enable accessibility or alter application settings. Unsupported applications return no result.

Windows uses system PowerShell and UI Automation in the interactive user's session. Edit controls and writable ValuePattern providers are recognized. Service Session 0 cannot inspect the user's desktop; the service/user helper architecture remains a separate prerequisite. Custom terminals without an editable accessibility provider may remain unsupported. macOS currently returns no result.

The web validates nonce, age and bounds, and only tries to focus its native keyboard sink if the tap falls inside the detected field. New touches, cancelled gestures and disconnects invalidate pending responses. No tools panel opens. iOS/browser user-activation restrictions can prevent a keyboard from opening in response to the asynchronous reply; a fresh preloaded region allows focus directly in the touch gesture; otherwise the keyboard button remains the reliable fallback. This does not promise universal automatic keyboard opening.

TR: Bu özellik deneysel; gerçek cihaz kabul testleri tamamlanmadı. Yalnızca alanın konumu aktarılır, yazı/parola içeriği okunmaz. Erişilebilirlik sağlamayan uygulamalarda ve macOS'ta klavye düğmesi kullanılır. iPhone'un asenkron odaklama kısıtlaması nedeniyle otomatik açılma garantisi yoktur. Windows servisinden kullanıcı masaüstüne erişim bu özellik tarafından çözülmez.

Providers returning invalid/unsupported screen coordinates are rejected, including GTK providers reporting (0,0) for an offset control. GTK3/X11 coordinates were verified on the Raspberry; this does not establish support for every native Wayland application.

## Raspberry Pi OS: empty accessibility tree

If the helper returns `[]`, check `gsettings get org.gnome.desktop.interface toolkit-accessibility`, `org.a11y.Status.IsEnabled` on the session bus, and the target application's `NO_AT_BRIDGE` environment variable. Raspberry Pi OS may set `NO_AT_BRIDGE=1` in `/etc/profile.d/at-dbus-fix.sh` when the accessibility bus package was absent at login. Installing the package does not change the environment of already-running applications. Enable toolkit accessibility and reopen the application without that flag, or sign out and back in after installation. Do not terminate an existing terminal with unsaved work.

TR: Raspberry Pi OS, erişilebilirlik paketi kurulmadan önce açılmış uygulamalara `NO_AT_BRIDGE=1` aktarabiliyor. Paket kurulsa bile açık uygulama bu değeri koruyor. Erişilebilirliği etkinleştirip uygulamayı bu değişken olmadan yeniden açın; mevcut işi olan terminali kapatmayın. Kurulum sonrası yeni masaüstü oturumu da eski ortam değişkenini temizler.

On the Raspberry, enabling accessibility and opening LXTerminal with `NO_AT_BRIDGE=0 GDK_BACKEND=x11` produced valid terminal bounds in approximately 160–170 ms. A tap on that terminal in the live mobile layout focused the invisible keyboard sink; typing appeared in the remote terminal without a separate text box. This desktop-browser test does not verify the physical iOS/Android keyboard.
