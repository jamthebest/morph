default: run

# Prefijos de instalación de usuario (se pueden cambiar con HOME)
bin := env_var('HOME') / ".local/bin"
share := env_var('HOME') / ".local/share"
app_id := "io.github.jamthebest.Morph"

run:
    cargo run

build:
    cargo build --release

# Instala en ~/.local (sin autoinicio: se activa desde Ajustes)
install: build
    install -Dm755 target/release/morph {{bin}}/morph
    install -Dm644 resources/{{app_id}}.desktop {{share}}/applications/{{app_id}}.desktop
    install -Dm644 resources/icons/hicolor/scalable/apps/{{app_id}}.svg {{share}}/icons/hicolor/scalable/apps/{{app_id}}.svg
    install -Dm644 resources/{{app_id}}.metainfo.xml {{share}}/metainfo/{{app_id}}.metainfo.xml

# Quita lo instalado (y el autoinicio si apunta a este binario); no toca config ni datos
uninstall:
    #!/usr/bin/env bash
    set -eu
    rm -f "{{bin}}/morph"
    rm -f "{{share}}/applications/{{app_id}}.desktop"
    rm -f "{{share}}/icons/hicolor/scalable/apps/{{app_id}}.svg"
    rm -f "{{share}}/metainfo/{{app_id}}.metainfo.xml"
    auto="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/{{app_id}}.desktop"
    if [ -f "$auto" ] && grep -qF "{{bin}}/morph" "$auto"; then
        rm -f "$auto"
    fi

# Genera el paquete .deb en dist/
deb:
    scripts/build-deb.sh

# Regenera THIRD-PARTY-LICENSES.md
licenses:
    scripts/third-party-licenses.py

test:
    cargo test

check:
    cargo clippy --all-targets
