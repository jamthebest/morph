#!/usr/bin/env bash
# Añade .deb a un repositorio APT firmado (suite stable, componente main, amd64).
# Uso: scripts/apt-repo.sh <dir-con-debs> <dir-del-sitio>
# Variables: APT_GPG_KEY_ID (obligatoria), APT_GPG_PASSPHRASE (opcional),
#            APT_PUBKEY_FILE (opcional, por defecto resources/apt/morph-archive-keyring.asc),
#            APT_KEEP (versiones a conservar, por defecto 3).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEBS=${1:?uso: apt-repo.sh <dir-con-debs> <dir-del-sitio>}
SITE=${2:?uso: apt-repo.sh <dir-con-debs> <dir-del-sitio>}
KEEP=${APT_KEEP:-3}
PUBKEY=${APT_PUBKEY_FILE:-$ROOT/resources/apt/morph-archive-keyring.asc}
BASE_URL=https://jamthebest.github.io/morph

die() { echo "error: $*" >&2; exit 1; }

for tool in apt-ftparchive gpg gpgv gzip dpkg-deb dpkg; do
    command -v "$tool" >/dev/null || die "falta '$tool' (apt-ftparchive está en el paquete apt-utils)"
done
[ -d "$DEBS" ] || die "no existe el directorio de .deb: $DEBS"
[ -n "${APT_GPG_KEY_ID:-}" ] || die "define APT_GPG_KEY_ID: no se publica un repositorio sin firmar"
[ -f "$PUBKEY" ] || die "falta la llave pública $PUBKEY (créala siguiendo docs/apt-setup.md)"
[ "$KEEP" -ge 1 ] 2>/dev/null || die "APT_KEEP debe ser un entero >= 1"

SITE=$(mkdir -p "$SITE" && cd "$SITE" && pwd)
POOL=$SITE/pool/main/m/morph
DIST=$SITE/dists/stable
BIN=$DIST/main/binary-amd64
mkdir -p "$POOL" "$BIN"

# 1. Copiar los .deb nuevos (solo el paquete morph, amd64).
shopt -s nullglob
NEW=()
for deb in "$DEBS"/*.deb; do
    [ "$(dpkg-deb -f "$deb" Package)" = morph ] || die "$deb no es el paquete morph"
    [ "$(dpkg-deb -f "$deb" Architecture)" = amd64 ] || { echo "aviso: se omite $deb (no es amd64)" >&2; continue; }
    cp -f "$deb" "$POOL/$(basename "$deb")"
    NEW+=("$(basename "$deb")")
done
POOLED=("$POOL"/*.deb)
shopt -u nullglob
[ "${#POOLED[@]}" -gt 0 ] || die "no hay ningún .deb en $DEBS ni en el pool"

# 2. Podar: ordenar por versión Debian (inserción con dpkg --compare-versions), más nueva primero.
SORTED_V=(); SORTED_F=()
for f in "${POOLED[@]}"; do
    v=$(dpkg-deb -f "$f" Version)
    i=${#SORTED_V[@]}
    while [ "$i" -gt 0 ] && dpkg --compare-versions "$v" gt "${SORTED_V[$((i - 1))]}"; do i=$((i - 1)); done
    SORTED_V=("${SORTED_V[@]:0:$i}" "$v" "${SORTED_V[@]:$i}")
    SORTED_F=("${SORTED_F[@]:0:$i}" "$f" "${SORTED_F[@]:$i}")
done
for idx in "${!SORTED_F[@]}"; do
    [ "$idx" -lt "$KEEP" ] && continue
    f=${SORTED_F[$idx]}
    for n in "${NEW[@]}"; do
        [ "$(basename "$f")" = "$n" ] && { echo "aviso: se conserva $n (recién añadido) aunque sea antiguo" >&2; continue 2; }
    done
    echo "poda: ${SORTED_V[$idx]}"
    rm -f "$f"
done

# 3. Packages y Release.
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$SITE"
rm -f "$DIST/InRelease" "$DIST/Release" "$DIST/Release.gpg"
apt-ftparchive --arch amd64 packages pool > "$BIN/Packages"
gzip -9nc "$BIN/Packages" > "$BIN/Packages.gz"
cat > "$WORK/release.conf" <<CONF
APT::FTPArchive::Release::Origin "Morph";
APT::FTPArchive::Release::Label "Morph";
APT::FTPArchive::Release::Suite "stable";
APT::FTPArchive::Release::Codename "stable";
APT::FTPArchive::Release::Architectures "amd64";
APT::FTPArchive::Release::Components "main";
APT::FTPArchive::Release::Description "Morph dynamic island for COSMIC";
CONF
apt-ftparchive -c "$WORK/release.conf" release dists/stable > "$WORK/Release"
mv "$WORK/Release" "$DIST/Release"

# 4. Firmar (InRelease clearsign + Release.gpg detached armored).
sign() {
    local args=(--batch --yes --local-user "$APT_GPG_KEY_ID")
    if [ -n "${APT_GPG_PASSPHRASE:-}" ]; then
        printf '%s' "$APT_GPG_PASSPHRASE" | gpg "${args[@]}" --pinentry-mode loopback --passphrase-fd 0 "$@"
    else
        gpg "${args[@]}" "$@"
    fi
}
fail_unsigned() { rm -f "$DIST/InRelease" "$DIST/Release.gpg" "$DIST/Release"; die "$1"; }
sign --clearsign -o "$DIST/InRelease" "$DIST/Release" || fail_unsigned "no se pudo firmar InRelease"
sign --armor --detach-sign -o "$DIST/Release.gpg" "$DIST/Release" || fail_unsigned "no se pudo firmar Release.gpg"

# 5. Llave pública y comprobación de que la firma corresponde a esa llave.
gpg --dearmor < "$PUBKEY" > "$WORK/keyring.gpg"
gpgv --keyring "$WORK/keyring.gpg" "$DIST/InRelease" >/dev/null 2>&1 \
    || fail_unsigned "la firma no se verifica con $PUBKEY: la llave pública no corresponde a APT_GPG_KEY_ID"
gpgv --keyring "$WORK/keyring.gpg" "$DIST/Release.gpg" "$DIST/Release" >/dev/null 2>&1 \
    || fail_unsigned "Release.gpg no se verifica con $PUBKEY"
cp -f "$PUBKEY" "$SITE/morph-archive-keyring.asc"
cp -f "$WORK/keyring.gpg" "$SITE/morph-archive-keyring.gpg"

# 6. index.html mínimo.
cat > "$SITE/index.html" <<HTML
<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Morph APT repository</title></head>
<body>
<h1>Morph APT repository</h1>
<p>Signed APT repository for <a href="https://github.com/jamthebest/morph">Morph</a>, the dynamic island for COSMIC (amd64).</p>
<pre>sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSL $BASE_URL/morph-archive-keyring.gpg -o /etc/apt/keyrings/morph.gpg
sudo tee /etc/apt/sources.list.d/morph.sources &gt;/dev/null &lt;&lt;'EOF'
Types: deb
URIs: $BASE_URL
Suites: stable
Components: main
Architectures: amd64
Signed-By: /etc/apt/keyrings/morph.gpg
EOF
sudo apt update &amp;&amp; sudo apt install morph</pre>
</body></html>
HTML
touch "$SITE/.nojekyll"
echo "repositorio actualizado en $SITE"
