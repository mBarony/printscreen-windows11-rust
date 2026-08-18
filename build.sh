#!/usr/bin/env bash
#
# build.sh — compila o rustshot.exe para Windows 11 x64 a partir do Linux.
#
# Mesmo alvo do CI e do release (x86_64-pc-windows-msvc), sem Windows na
# jogada: o cargo-xwin faz o papel do MSVC usando o LLVM local — clang-cl
# compila, lld-link linka, llvm-lib arquiva e llvm-rc embute os recursos Win32
# (ícone + manifesto Per-Monitor V2, que o embed-resource escolhe sozinho
# quando o alvo é MSVC). Os headers e as libs do Windows SDK/CRT são baixados
# pelo xwin no primeiro build e ficam em ~/.cache/cargo-xwin.
#
# O alvo GNU (mingw) sairia mais fácil, mas trocaria o ABI do binário
# publicado: todo o código Win32 do projeto, o manifesto amd64 e o wgpu/dx12
# são exercitados em MSVC — é esse exe que o Windows recebe.
#
# Dependências de sistema (Debian/Ubuntu): sudo apt install clang lld llvm
# O resto (target do rustup, cargo-xwin) o script instala sozinho no ~/.cargo.

set -euo pipefail

TARGET=x86_64-pc-windows-msvc
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIM_DIR="$REPO_ROOT/target/llvm-shims"
# RNF-01: o exe publicado deve ficar em 15 MB.
SIZE_LIMIT=$((15 * 1024 * 1024))

dev=0
skip_checks=0
clean=0
run_tests_opt=0
out_dir=""

if [[ -t 1 ]]; then
    C_STEP=$'\033[36m'; C_OK=$'\033[32m'; C_WARN=$'\033[33m'; C_ERR=$'\033[31m'; C_OFF=$'\033[0m'
else
    C_STEP=''; C_OK=''; C_WARN=''; C_ERR=''; C_OFF=''
fi

step() { printf '\n%s==> %s%s\n' "$C_STEP" "$1" "$C_OFF"; }
ok()   { printf '    %sOK:%s %s\n' "$C_OK" "$C_OFF" "$1"; }
warn() { printf '    %sAviso:%s %s\n' "$C_WARN" "$C_OFF" "$1"; }
die()  { printf '    %sERRO:%s %s\n' "$C_ERR" "$C_OFF" "$1" >&2; exit 1; }

usage() {
    cat <<'EOF'
Compila o rustshot.exe (Windows 11 x64, alvo MSVC) a partir do Linux.

Uso: ./build.sh [opções]

  --dev           perfil debug em vez de release
  --skip-checks   pula o clippy; só compila
  --test          roda os testes (precisa de wine ou do interop do WSL)
  --clean         cargo clean antes de compilar
  --out DIR       copia o exe para DIR ao final
  -h, --help      esta ajuda

Dependências de sistema (Debian/Ubuntu): sudo apt install clang lld llvm
EOF
    exit "${1:-0}"
}

parse_args() {
    while (($#)); do
        case "$1" in
            --dev)         dev=1 ;;
            --skip-checks) skip_checks=1 ;;
            --test)        run_tests_opt=1 ;;
            --clean)       clean=1 ;;
            --out)         out_dir="${2:?--out exige um diretório}"; shift ;;
            -h|--help)     usage 0 ;;
            *)             printf 'Opção desconhecida: %s\n\n' "$1" >&2; usage 1 ;;
        esac
        shift
    done
}

# As distros publicam o LLVM com sufixo de versão (llvm-rc-18) e/ou fora do
# PATH (/usr/lib/llvm-18/bin), mas o cargo-xwin e o embed-resource chamam os
# nomes puros. Procura nas três formas, da versão mais nova para a mais velha.
find_tool() {
    local name="$1" candidate
    if candidate="$(command -v "$name" 2>/dev/null)"; then
        printf '%s' "$candidate"
        return 0
    fi
    for candidate in $(printf '%s\n' /usr/lib/llvm-*/bin/"$name" /usr/bin/"$name"-* | sort -Vr); do
        [[ -x "$candidate" ]] && { printf '%s' "$candidate"; return 0; }
    done
    return 1
}

# Reúne o que foi achado em um diretório de symlinks com os nomes puros e põe
# esse diretório na frente do PATH — assim o cargo-xwin encontra o que espera
# sem depender de como a distro nomeou os binários.
link_tool() {
    local name="$1" path
    path="$(find_tool "$name")" ||
        die "$name não encontrado. Em Debian/Ubuntu: sudo apt install clang lld llvm"
    ln -sf "$path" "$SHIM_DIR/$name"
    ok "$name -> $path"
}

ensure_llvm() {
    step 'Verificando o LLVM (faz o papel do MSVC)'
    mkdir -p "$SHIM_DIR"
    # clang: o cargo-xwin cria o próprio symlink clang-cl a partir dele.
    # lld-link: linker. llvm-lib: archiver. llvm-rc: recursos Win32.
    local tool
    for tool in clang lld-link llvm-lib llvm-rc; do
        link_tool "$tool"
    done
    export PATH="$SHIM_DIR:$PATH"
    # O embed-resource aceita RC_<target em minúsculas> para não depender de um
    # llvm-rc sem sufixo no PATH; aponta para o symlink, que sempre existe.
    export "RC_${TARGET//-/_}=$SHIM_DIR/llvm-rc"
    # Só x86_64 interessa: o padrão do xwin também baixaria o SDK de aarch64.
    export XWIN_ARCH=x86_64
}

ensure_rust() {
    step 'Verificando o Rust'
    command -v cargo >/dev/null || die 'cargo não encontrado. Instale o Rust em https://rustup.rs'
    ok "$(rustc --version)"

    if command -v rustup >/dev/null; then
        if ! rustup target list --installed | grep -qx "$TARGET"; then
            warn "target $TARGET ausente; instalando"
            rustup target add "$TARGET"
        fi
        ok "target $TARGET"
    else
        warn "rustup não encontrado — confirme que a std de $TARGET está instalada"
    fi

    if ! command -v cargo-xwin >/dev/null; then
        warn 'cargo-xwin ausente; instalando (compila, leva alguns minutos)'
        cargo install --locked cargo-xwin
    fi
    ok "$(cargo-xwin --version)"
}

# Como rodar um exe de teste aqui: o wine executa o binário do Windows, e no
# WSL o interop também — quando está de fato funcionando. Registro no
# binfmt_misc não é garantia: com `automount`/`appendWindowsPath` desligados no
# wsl.conf a execução morre em "Invalid argument". Por isso os testes são
# opt-in (--test): infraestrutura ausente não deve derrubar o build.
test_runner() {
    if command -v wine >/dev/null; then
        command -v wine
    elif [[ -e /proc/sys/fs/binfmt_misc/WSLInterop || -e /proc/sys/fs/binfmt_misc/WSLInterop-late ]]; then
        printf '/usr/bin/env'
    fi
}

run_tests() {
    local runner key
    runner="$(test_runner)"
    [[ -n "$runner" ]] || die 'sem wine e sem interop do WSL: não há como executar os testes aqui'

    key="${TARGET//-/_}"
    step "Testes de unidade (runner: $runner)"
    env "CARGO_TARGET_${key^^}_RUNNER=$runner" cargo xwin test --target "$TARGET"
    ok 'testes aprovados'
}

run_checks() {
    if ((skip_checks)); then
        step 'Verificações puladas (--skip-checks)'
    else
        step 'Clippy (-D warnings, como no CI)'
        cargo xwin clippy --all-targets --target "$TARGET" -- -D warnings
        ok 'sem warnings'
    fi

    if ((run_tests_opt)); then
        run_tests
    elif ((!skip_checks)); then
        step 'Testes'
        warn 'os testes compilam para Windows; rode com --test se houver wine/interop'
        warn 'no CI eles rodam em windows-latest'
    fi
}

build() {
    if ((dev)); then
        step 'Compilando (perfil dev)'
        cargo xwin build --target "$TARGET"
    else
        step 'Compilando (perfil release)'
        cargo xwin build --release --target "$TARGET"
    fi
}

check_size() {
    local size="$1"
    ((dev)) && return 0
    if ((size <= SIZE_LIMIT)); then
        ok 'dentro do alvo de 15 MB (RNF-01)'
    else
        warn 'acima do alvo de 15 MB (RNF-01)'
    fi
}

# O embed_resource::compile() do build.rs é manifest_optional(): sem um
# compilador de recursos ele deixa o build passar e o exe sai sem ícone e, pior,
# sem o manifesto de DPI — um app de captura DPI-unaware entrega imagem na
# resolução errada. A string do manifesto prova que o llvm-rc entrou.
check_resources() {
    local exe="$1"
    if grep -aq PerMonitorV2 "$exe"; then
        ok 'recursos Win32 embutidos (ícone + manifesto Per-Monitor V2)'
    elif ((dev)); then
        warn 'exe sem o manifesto de DPI — o llvm-rc não rodou'
    else
        die 'exe sem o manifesto de DPI (llvm-rc não rodou): não distribua este binário'
    fi
}

report() {
    local exe="$1" size mb
    step 'Artefato'
    size="$(stat -c%s "$exe")"
    mb="$(awk -v b="$size" 'BEGIN { printf "%.2f", b / 1048576 }')"
    printf '    Caminho: %s\n' "$exe"
    printf '    Tamanho: %s MB\n' "$mb"
    check_size "$size"
    check_resources "$exe"
}

install_out() {
    local exe="$1"
    [[ -n "$out_dir" ]] || return 0
    step "Copiando para $out_dir"
    mkdir -p "$out_dir"
    cp -f "$exe" "$out_dir/"
    ok "$out_dir/rustshot.exe"
    printf '    (config.json e rustshot.log nascem ao lado do exe, nessa pasta)\n'
}

main() {
    local profile_dir exe
    parse_args "$@"
    cd "$REPO_ROOT"

    ensure_rust
    # Antes do ensure_llvm: o clean apaga o target/, e é lá que ficam os shims.
    if ((clean)); then
        step 'Limpando artefatos'
        cargo clean
    fi
    ensure_llvm

    run_checks
    build

    profile_dir=release
    ((dev)) && profile_dir=debug
    exe="$REPO_ROOT/target/$TARGET/$profile_dir/rustshot.exe"
    [[ -f "$exe" ]] || die "build terminou sem erro, mas $exe não existe"

    report "$exe"
    install_out "$exe"

    # "Build concluído" e não só "Concluído": o build-signed.sh delega o build
    # para cá e emenda a assinatura depois — dois "Concluído" confundiriam.
    printf '\n%sBuild concluído.%s\n' "$C_OK" "$C_OFF"
}

main "$@"
