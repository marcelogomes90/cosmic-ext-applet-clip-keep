name := 'cosmic-clip-keep'
appid := 'io.github.marcelogomes90.cosmic-ext-applet-clip-keep'
icon-name := appid + '-symbolic'

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')

bin-src := cargo-target-dir / 'release' / name
bin-dst := base-dir / 'bin' / name
dump-src := cargo-target-dir / 'release' / name + '-dump'
dump-dst := base-dir / 'bin' / name + '-dump'
desktop-dst := base-dir / 'share' / 'applications' / appid + '.desktop'
metainfo-dst := base-dir / 'share' / 'metainfo' / appid + '.metainfo.xml'
icon-src := 'resources' / (icon-name + '.svg')
icon-dst := base-dir / 'share' / 'icons' / 'hicolor' / 'scalable' / 'apps' / (icon-name + '.svg')
legacy-icon-dst := base-dir / 'share' / 'icons' / 'hicolor' / 'scalable' / 'apps' / (appid + '.svg')
license-dst := base-dir / 'share' / 'licenses' / appid / 'LICENSE'

user-base := env('XDG_DATA_HOME', env('HOME') / '.local' / 'share')
user-bin-dst := env('HOME') / '.local' / 'bin' / name
user-dump-dst := env('HOME') / '.local' / 'bin' / name + '-dump'
user-desktop-dst := user-base / 'applications' / appid + '.desktop'
user-metainfo-dst := user-base / 'metainfo' / appid + '.metainfo.xml'
user-icon-dst := user-base / 'icons' / 'hicolor' / 'scalable' / 'apps' / (icon-name + '.svg')
user-legacy-icon-dst := user-base / 'icons' / 'hicolor' / 'scalable' / 'apps' / (appid + '.svg')
user-license-dst := user-base / 'licenses' / appid / 'LICENSE'

flatpak-dir := 'flatpak' / appid
manifest := flatpak-dir / appid + '.json'
flatpak-build-dir := 'build' / 'flatpak'

default: build-release

clean:
    cargo clean

clean-flatpak:
    rm -rf build .flatpak-builder

build-debug *args:
    cargo build {{args}}

build-release *args: (build-debug '--release' args)

check *args:
    cargo clippy --all-targets --all-features {{args}} -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

test *args:
    cargo test {{args}}

layering:
    #!/usr/bin/env bash
    set -euo pipefail
    if grep -rn 'cosmic::\|iced::\|use cosmic\b' src/clip/ 2>/dev/null; then
        echo "src/clip must not depend on libcosmic or iced" >&2
        exit 1
    fi
    echo "layering ok"

validate:
    #!/usr/bin/env bash
    set -euo pipefail
    output="$(desktop-file-validate resources/{{appid}}.desktop || true)"
    output="$(printf '%s\n' "${output}" | grep -v 'unregistered value "COSMIC"' \
        | grep -v 'does not contain a registered main category' | grep -v '^$' || true)"
    if [ -n "${output}" ]; then printf '%s\n' "${output}"; exit 1; fi
    echo "desktop entry ok"
    appstreamcli validate --no-net resources/{{appid}}.metainfo.xml

verify: fmt-check check layering test validate

run *args:
    cargo run --bin {{name}} {{args}}

run-dump *args:
    cargo run --bin {{name}}-dump -- {{args}}

install:
    install -Dm0755 {{bin-src}} {{bin-dst}}
    install -Dm0755 {{dump-src}} {{dump-dst}}
    install -Dm0644 resources/{{appid}}.desktop {{desktop-dst}}
    install -Dm0644 resources/{{appid}}.metainfo.xml {{metainfo-dst}}
    install -Dm0644 {{icon-src}} {{icon-dst}}
    install -Dm0644 LICENSE {{license-dst}}

uninstall:
    rm -f {{bin-dst}} {{dump-dst}} {{desktop-dst}} {{metainfo-dst}} {{icon-dst}} {{legacy-icon-dst}} {{license-dst}}

install-user:
    install -Dm0755 {{bin-src}} {{user-bin-dst}}
    install -Dm0755 {{dump-src}} {{user-dump-dst}}
    install -Dm0644 resources/{{appid}}.metainfo.xml {{user-metainfo-dst}}
    install -Dm0644 {{icon-src}} {{user-icon-dst}}
    install -Dm0644 LICENSE {{user-license-dst}}
    mkdir -p "$(dirname {{user-desktop-dst}})"
    sed 's|^Exec=.*|Exec={{user-bin-dst}}|' resources/{{appid}}.desktop > {{user-desktop-dst}}
    chmod 0644 {{user-desktop-dst}}
    @echo "Installed. Add 'Clip Keep' in Settings -> Desktop -> Panel -> Applets."

uninstall-user:
    rm -f {{user-bin-dst}} {{user-dump-dst}} {{user-desktop-dst}} {{user-metainfo-dst}} {{user-icon-dst}} {{user-legacy-icon-dst}} {{user-license-dst}}

flatpak-sources:
    flatpak run --filesystem="$(pwd)" --share=network \
        --command=flatpak-cargo-generator org.flatpak.Builder \
        "$(pwd)/Cargo.lock" -o "$(pwd)/{{flatpak-dir}}/cargo-sources.json"

flatpak-build:
    flatpak-builder --user --install --force-clean --ccache \
        {{flatpak-build-dir}} {{manifest}}

flatpak-build-local:
    #!/usr/bin/env bash
    set -euo pipefail
    local_manifest="{{flatpak-dir}}/local-test.json"
    trap 'rm -f "${local_manifest}"' EXIT
    python3 -c 'import json,sys; m=json.load(open(sys.argv[1])); m["modules"][0]["sources"][0]={"type":"dir","path":"../..","skip":["target","build",".flatpak-builder",".git","flatpak"]}; json.dump(m,open(sys.argv[2],"w"),indent=2)' \
        "{{manifest}}" "${local_manifest}"
    flatpak-builder --user --install --force-clean --ccache \
        {{flatpak-build-dir}}-local "${local_manifest}"

flatpak-run *args:
    flatpak run {{appid}} {{args}}

flatpak-dump:
    flatpak run --command={{name}}-dump {{appid}}

flatpak-uninstall:
    flatpak uninstall --user -y {{appid}}
