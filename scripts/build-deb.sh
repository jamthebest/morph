#!/usr/bin/env bash
# Construye dist/morph_<versión>_<arquitectura>.deb (no lo instala).
# Variable opcional: MORPH_MAINTAINER="Nombre <correo>".
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
APP_ID=io.github.jamthebest.Morph

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
ARCH=$(dpkg --print-architecture)
MAINTAINER=${MORPH_MAINTAINER:-Javier Midence <javier.midence22@gmail.com>}
HOMEPAGE=$(sed -n 's/^homepage = "\(.*\)"/\1/p' Cargo.toml | head -n1)

cargo build --release --locked

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
PKG=$STAGE/pkg
DOC=$PKG/usr/share/doc/morph

install -Dm755 target/release/morph "$PKG/usr/bin/morph"
strip --strip-unneeded "$PKG/usr/bin/morph"
install -Dm644 "resources/$APP_ID.desktop" "$PKG/usr/share/applications/$APP_ID.desktop"
install -Dm644 "resources/icons/hicolor/scalable/apps/$APP_ID.svg" \
    "$PKG/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
install -Dm644 "resources/$APP_ID.metainfo.xml" "$PKG/usr/share/metainfo/$APP_ID.metainfo.xml"
install -Dm644 README.md "$DOC/README.md"
gzip -9n -c THIRD-PARTY-LICENSES.md > "$DOC/THIRD-PARTY-LICENSES.md.gz"
chmod 644 "$DOC/THIRD-PARTY-LICENSES.md.gz"

cat > "$DOC/copyright" <<COPYRIGHT
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: morph
Source: $HOMEPAGE

Files: *
Copyright: 2026 Javier Midence
License: GPL-3.0-only

License: GPL-3.0-only
 This program is free software: you can redistribute it and/or modify it
 under the terms of the GNU General Public License version 3 as published
 by the Free Software Foundation.
 .
 This program is distributed in the hope that it will be useful, but
 WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
 or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License
 for more details.
 .
 On Debian systems, the complete text of the GNU General Public License
 version 3 can be found in /usr/share/common-licenses/GPL-3.
 .
 Third-party components are listed in THIRD-PARTY-LICENSES.md.gz.
COPYRIGHT
chmod 644 "$DOC/copyright"

# Suscripción al repositorio APT (como Chrome o VS Code): llave + fuente deb822 (conffile).
APT_KEY_ASC=$ROOT/resources/apt/morph-archive-keyring.asc
if [ -f "$APT_KEY_ASC" ]; then
    install -d -m755 "$PKG/usr/share/keyrings"
    gpg --dearmor < "$APT_KEY_ASC" > "$PKG/usr/share/keyrings/morph-archive-keyring.gpg"
    chmod 644 "$PKG/usr/share/keyrings/morph-archive-keyring.gpg"
    install -d -m755 "$PKG/etc/apt/sources.list.d"
    cat > "$PKG/etc/apt/sources.list.d/morph.sources" <<SOURCES
Types: deb
URIs: https://jamthebest.github.io/morph
Suites: stable
Components: main
Architectures: amd64
Signed-By: /usr/share/keyrings/morph-archive-keyring.gpg
SOURCES
    chmod 644 "$PKG/etc/apt/sources.list.d/morph.sources"
    mkdir -p "$PKG/DEBIAN"
    echo /etc/apt/sources.list.d/morph.sources > "$PKG/DEBIAN/conffiles"
else
    echo "aviso: falta resources/apt/morph-archive-keyring.asc; el .deb se construye sin la fuente APT" >&2
fi

# Depends calculado con dpkg-shlibdeps (necesita un debian/control mínimo)
mkdir -p "$STAGE/debian"
printf 'Source: morph\n\nPackage: morph\nArchitecture: any\n' > "$STAGE/debian/control"
DEPENDS=$(cd "$STAGE" && dpkg-shlibdeps -O -e"$PKG/usr/bin/morph" 2>/dev/null \
    | sed -n 's/^shlibs:Depends=//p')
# libwayland-client se carga en tiempo de ejecución (dlopen): dpkg-shlibdeps no la ve.
DEPENDS="$DEPENDS, libwayland-client0"

SIZE=$(du -sk "$PKG" | cut -f1)
mkdir -p "$PKG/DEBIAN"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: morph
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: $MAINTAINER
Installed-Size: $SIZE
Depends: $DEPENDS
Recommends: pulseaudio-utils | pipewire-pulse, wireplumber, gnome-keyring
Suggests: gh
Homepage: $HOMEPAGE
Description: Dynamic island for the COSMIC desktop
 Morph is a dynamic island that lives at the top of the COSMIC desktop.
 It expands to show media playback, notifications, calendar, a file shelf,
 workspaces and developer tools, and collapses when you are done.
CONTROL

mkdir -p "$ROOT/dist"
OUT=$ROOT/dist/morph_${VERSION}_${ARCH}.deb
dpkg-deb --root-owner-group --build "$PKG" "$OUT" >/dev/null
echo "$OUT"
