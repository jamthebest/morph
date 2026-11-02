#!/usr/bin/env bash
# Prueba local (sin sudo, sin red) de scripts/apt-repo.sh con llaves desechables
# en un GNUPGHOME temporal. Nunca toca el llavero del usuario ni /etc.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d)
export GNUPGHOME=$T/gnupg
mkdir -m 700 "$GNUPGHOME"
cleanup() { gpgconf --kill gpg-agent >/dev/null 2>&1 || true; rm -rf "$T"; }
trap cleanup EXIT

PASS=0
ok() { PASS=$((PASS + 1)); echo "ok: $*"; }
fail() { echo "FALLO: $*" >&2; exit 1; }

# Llaves desechables: A sin passphrase, B con passphrase (prueba --passphrase-fd).
gpg --batch --pinentry-mode loopback --passphrase '' --quick-gen-key "Morph Test A <a@example.invalid>" ed25519 sign 1d 2>/dev/null
gpg --batch --pinentry-mode loopback --passphrase 'secreto' --quick-gen-key "Morph Test B <b@example.invalid>" ed25519 sign 1d 2>/dev/null
KEY_A=$(gpg --list-keys --with-colons a@example.invalid | awk -F: '/^fpr/{print $10; exit}')
KEY_B=$(gpg --list-keys --with-colons b@example.invalid | awk -F: '/^fpr/{print $10; exit}')
gpg --armor --export "$KEY_A" > "$T/a.asc"
gpg --armor --export "$KEY_B" > "$T/b.asc"
gpg --dearmor < "$T/a.asc" > "$T/a.gpg"

# .deb mínimos con dpkg-deb (el orden alfabético y el de versión difieren a propósito).
mkdeb() { # versión directorio-destino
    local d=$T/build-$1
    mkdir -p "$d/DEBIAN" "$d/usr/share/doc/morph" "$2"
    printf 'Package: morph\nVersion: %s\nArchitecture: amd64\nMaintainer: Test <t@example.invalid>\nDescription: test\n' "$1" > "$d/DEBIAN/control"
    echo "$1" > "$d/usr/share/doc/morph/version"
    dpkg-deb --root-owner-group --build "$d" "$2/morph_$1_amd64.deb" >/dev/null
}
for v in 0.1.0 0.2.0 0.9.0 0.10.0 0.10.1; do mkdeb "$v" "$T/debs-$v"; done

SITE=$T/site
repo() { APT_GPG_KEY_ID=$KEY_A APT_PUBKEY_FILE=$T/a.asc "$ROOT/scripts/apt-repo.sh" "$@" >/dev/null; }

# Aislamiento de apt.
apt_site() { # sitio keyring -> prepara $T/apt-<n>
    local n=$T/apt-$RANDOM
    mkdir -p "$n/etc/sources.list.d" "$n/etc/preferences.d" "$n/etc/apt.conf.d" "$n/state/lists/partial" "$n/cache/archives/partial"
    : > "$n/status"
    echo "deb [signed-by=$2] file:$1 stable main" > "$n/etc/sources.list.d/morph.list"
    echo "$n"
}
apt_opts() {
    echo -o "Dir::Etc=$1/etc" -o "Dir::State=$1/state" -o "Dir::Cache=$1/cache" \
        -o "Dir::State::status=$1/status" -o Debug::NoLocking=1 -o APT::Sandbox::User= \
        -o APT::Architecture=amd64 -o "Dir::Etc::sourcelist=/dev/null"
}

# 1. Poda con orden por versión Debian: añadir 0.1.0, 0.2.0, 0.9.0, 0.10.0, 0.10.1 de uno en uno.
for v in 0.1.0 0.2.0 0.9.0 0.10.0 0.10.1; do repo "$T/debs-$v" "$SITE"; done
POOL=$SITE/pool/main/m/morph
[ "$(ls "$POOL" | wc -l)" -eq 3 ] || fail "el pool debería tener 3 .deb: $(ls "$POOL")"
[ -f "$POOL/morph_0.10.1_amd64.deb" ] && [ -f "$POOL/morph_0.10.0_amd64.deb" ] && [ -f "$POOL/morph_0.9.0_amd64.deb" ] \
    || fail "poda incorrecta (debe quedar 0.9.0, 0.10.0, 0.10.1): $(ls "$POOL")"
ok "la poda deja las 3 versiones más recientes (orden Debian, no alfabético)"

# 2. Idempotencia: repetir no cambia el contenido de Packages.
sum1=$(sha256sum "$SITE/dists/stable/main/binary-amd64/Packages" | cut -d' ' -f1)
repo "$T/debs-0.10.1" "$SITE"
sum2=$(sha256sum "$SITE/dists/stable/main/binary-amd64/Packages" | cut -d' ' -f1)
[ "$sum1" = "$sum2" ] || fail "no es idempotente"
ok "idempotente"

# 3. Estructura y llaves publicadas.
for f in dists/stable/InRelease dists/stable/Release dists/stable/Release.gpg \
         dists/stable/main/binary-amd64/Packages dists/stable/main/binary-amd64/Packages.gz \
         morph-archive-keyring.gpg morph-archive-keyring.asc index.html; do
    [ -f "$SITE/$f" ] || fail "falta $f"
done
grep -q '^Origin: Morph$' "$SITE/dists/stable/Release" && grep -q '^Suite: stable$' "$SITE/dists/stable/Release" \
    || fail "Release sin Origin/Suite"
ok "estructura del sitio completa"

# 4. apt-get update acepta la firma y apt-cache policy muestra la versión.
A=$(apt_site "$SITE" "$T/a.gpg")
# shellcheck disable=SC2046
apt-get $(apt_opts "$A") update >"$T/update.log" 2>&1 || { cat "$T/update.log" >&2; fail "apt-get update rechazó el repositorio"; }
# shellcheck disable=SC2046
apt-cache $(apt_opts "$A") policy morph | grep -q 'Candidate: 0.10.1' || fail "apt-cache policy no muestra 0.10.1"
ok "apt-get update acepta la firma y apt-cache policy muestra morph 0.10.1"

# 5. Una firma con otra llave es rechazada (sitio firmado con B, cliente confía en A).
SITE_B=$T/site-b
APT_GPG_KEY_ID=$KEY_B APT_GPG_PASSPHRASE=secreto APT_PUBKEY_FILE=$T/b.asc \
    "$ROOT/scripts/apt-repo.sh" "$T/debs-0.10.1" "$SITE_B" >/dev/null
B=$(apt_site "$SITE_B" "$T/a.gpg")
# shellcheck disable=SC2046
if apt-get $(apt_opts "$B") update >"$T/update-b.log" 2>&1; then
    fail "apt aceptó un repositorio firmado con otra llave"
fi
grep -Eqi 'not signed|NO_PUBKEY|signature' "$T/update-b.log" || fail "el rechazo no fue por la firma: $(cat "$T/update-b.log")"
ok "una firma con otra llave es rechazada por apt (y la firma con passphrase funciona)"

# 6. apt-repo.sh no publica sin firmar ni con llave equivocada.
if APT_GPG_KEY_ID=$KEY_B APT_GPG_PASSPHRASE=secreto APT_PUBKEY_FILE=$T/a.asc \
    "$ROOT/scripts/apt-repo.sh" "$T/debs-0.10.1" "$T/site-c" >/dev/null 2>&1; then
    fail "el script aceptó una llave pública que no corresponde a la de firma"
fi
[ ! -e "$T/site-c/dists/stable/InRelease" ] || fail "quedó un InRelease tras el fallo"
ok "llave pública distinta a la de firma: el script falla y no deja InRelease"
if env -u APT_GPG_KEY_ID APT_PUBKEY_FILE=$T/a.asc "$ROOT/scripts/apt-repo.sh" "$T/debs-0.10.1" "$T/site-d" >/dev/null 2>&1; then
    fail "el script publicó sin APT_GPG_KEY_ID"
fi
if APT_GPG_KEY_ID=$KEY_A APT_PUBKEY_FILE=$T/no-existe.asc "$ROOT/scripts/apt-repo.sh" "$T/debs-0.10.1" "$T/site-e" >/dev/null 2>&1; then
    fail "el script publicó sin llave pública"
fi
ok "sin APT_GPG_KEY_ID o sin llave pública el script falla"

echo "Todas las pruebas pasaron ($PASS)"
