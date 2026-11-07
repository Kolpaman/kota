# Yayın rehberi

Hazır çıktılar `make pack`, `make tarball` ve `make rpm` ile `dist/` altına üretilir.

## 1. GitHub

```sh
cd kota
git init -b main && git add . && git commit -m "Kota 0.5.1"
git remote add origin git@github.com:ensignalp/kota.git
git push -u origin main
git tag v0.5.1 && git push origin v0.5.1
```

Sürüm sayfasına `dist/kota@ensignalp.shell-extension.zip` ve RPM'leri ekle.
`metadata.json` ve `kota.spec` içindeki `url` GitHub adresiyle aynı olmalı.

## 2. extensions.gnome.org

1. https://extensions.gnome.org/upload/ (GNOME hesabıyla giriş).
2. `dist/kota@ensignalp.shell-extension.zip` dosyasını yükle.
3. İnceleme notu: *"The extension contains no binaries. It only reads data from the
   separately packaged `kota` D-Bus service (io.github.ensignalp.Kota); the
   description says so."*

İnceleme genelde birkaç gün sürer; reddedilirse gerekçe e-postayla gelir.

## 3. Fedora COPR

1. https://copr.fedorainfracloud.org → New Project (`kota`), Fedora 43/44 x86_64/aarch64.
2. Ayarlarda **Enable internet access during builds** açık olsun (cargo bağımlılıkları).
3. Build → SCM: GitHub adresi, spec: `packaging/rpm/kota.spec`, yöntem `rpkg` ya da
   "Upload": `dist/kota-0.5.1-1.*.src.rpm`.
4. Kullanıcılar: `sudo dnf copr enable ensignalp/kota && sudo dnf install kota gnome-shell-extension-kota`
   ardından `systemctl --user enable --now kota`.
