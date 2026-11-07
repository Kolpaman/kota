# Kota — kullanıcı düzeyinde kurulum (sudo gerekmez).
#   make install      servis + D-Bus etkinleştirme + GNOME eklentisi
#   make uninstall    hepsini kaldırır
#   make pack         extensions.gnome.org için eklenti zip'i (binary içermez)

PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
SYSTEMD_USER_DIR ?= $(HOME)/.config/systemd/user
DBUS_SERVICES_DIR ?= $(DATADIR)/dbus-1/services
UUID := kota@ensignalp
EXT_SRC := gnome-extension/$(UUID)
EXT_DIR ?= $(DATADIR)/gnome-shell/extensions/$(UUID)
CARGO ?= cargo

.PHONY: all build test install install-service install-extension uninstall pack tarball rpm clean

all: build

build:
	$(CARGO) build --release --locked

test:
	$(CARGO) test --workspace --locked

install: install-service install-extension
	@echo
	@echo "Kota kuruldu. Durum: systemctl --user status kota   ·   CLI: $(BINDIR)/kota"

install-service: build
	install -Dm755 target/release/kota $(BINDIR)/kota
	install -d $(SYSTEMD_USER_DIR) $(DBUS_SERVICES_DIR)
	sed 's|@BINDIR@|$(BINDIR)|g' packaging/systemd/kota.service > $(SYSTEMD_USER_DIR)/kota.service
	sed 's|@BINDIR@|$(BINDIR)|g' packaging/dbus/io.github.ensignalp.Kota.service > $(DBUS_SERVICES_DIR)/io.github.ensignalp.Kota.service
	rm -f $(DBUS_SERVICES_DIR)/io.github.mustazafi.Kota.service  # 0.5.0 öncesi ad
	systemctl --user daemon-reload
	systemctl --user enable kota.service
	systemctl --user restart kota.service

install-extension:
	-gnome-extensions disable kota@mustazafi 2>/dev/null  # 0.5.0 öncesi kimlik
	rm -rf $(DATADIR)/gnome-shell/extensions/kota@mustazafi
	rm -rf $(EXT_DIR)
	install -d $(EXT_DIR)
	cp -r $(EXT_SRC)/. $(EXT_DIR)/
	glib-compile-schemas $(EXT_DIR)/schemas
	sh scripts/enable-extension.sh $(UUID)

uninstall:
	-gnome-extensions disable $(UUID)
	-systemctl --user disable --now kota.service
	rm -f $(SYSTEMD_USER_DIR)/kota.service $(DBUS_SERVICES_DIR)/io.github.ensignalp.Kota.service $(BINDIR)/kota
	rm -rf $(EXT_DIR)
	-systemctl --user daemon-reload

pack:
	install -d dist
	gnome-extensions pack --force --extra-source=lib --extra-source=icons --out-dir=dist $(EXT_SRC)

VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

# Kaynak arşivi (RPM/COPR için; GitHub etiket arşiviyle aynı yapı).
tarball:
	install -d dist
	tar --exclude=./target --exclude=./dist --exclude=./.git --exclude='*.zip' \
	  --transform 's,^\.,kota-$(VERSION),' -czf dist/kota-$(VERSION).tar.gz .

# Yerel RPM (rpmbuild gerekir).
rpm: tarball
	rpmbuild -ba packaging/rpm/kota.spec --define "_sourcedir $(CURDIR)/dist" --define "_rpmdir $(CURDIR)/dist" --define "_srcrpmdir $(CURDIR)/dist"

clean:
	$(CARGO) clean
	rm -rf dist
