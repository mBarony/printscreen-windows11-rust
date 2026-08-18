#!/usr/bin/env bash
#
# build-signed.sh — compila e assina o rustshot.exe (Authenticode) no Linux.
#
# Complementa o build.sh, que só compila: aqui o binário sai assinado com o
# certificado de código do projeto, como o workflow de release faz. O signtool
# do Windows SDK não existe aqui, e o osslsigncode faz o mesmo serviço —
# PKCS#12, digest SHA-256 e carimbo de tempo RFC 3161.
#
# A chave privada não entra no repositório: aponte o .pfx com --pfx (ou
# CODE_SIGN_PFX / CODE_SIGN_PFX_BASE64, o mesmo nome do segredo do CI) e passe a
# senha em CODE_SIGN_PASSWORD — se faltar, o script pergunta sem ecoar. O .pfx
# decodificado e o arquivo de senha ficam em um diretório temporário fora da
# árvore do projeto, apagado na saída, e a senha nunca vai para a linha de
# comando (onde qualquer `ps` a leria).
#
# O exe assinado sai em dist/, nunca em target/: é o mesmo cuidado do CI —
# binário assinado dentro do target/ contaminaria o cache de build.
#
# Uso:
#   CODE_SIGN_PASSWORD=... ./build-signed.sh --pfx ~/keys/rustshot.pfx
#   CODE_SIGN_PFX_BASE64="$(base64 -w0 rustshot.pfx)" ./build-signed.sh
#
# Dependência de sistema: sudo apt install osslsigncode
# (o build em si depende do que o build.sh pede: clang lld llvm)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET=x86_64-pc-windows-msvc
UNSIGNED="$REPO_ROOT/target/$TARGET/release/rustshot.exe"
# Mesmos parâmetros da action .github/actions/sign-windows.
TIMESTAMP_URL=http://timestamp.digicert.com
DESCRIPTION='RustShot'
DESCRIPTION_URL='https://github.com/mBarony/printscreen-windows11-rust'

pfx_path="${CODE_SIGN_PFX:-}"
expected_subject='CN=Marcio Baroni'
out_dir="$REPO_ROOT/dist"
skip_build=0
skip_checks=0
tmp_dir=''

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
Compila e assina o rustshot.exe (Windows 11 x64, Authenticode) a partir do Linux.

Uso: ./build-signed.sh [opções]

  --pfx ARQUIVO   PKCS#12 com chave + certificado (ou env CODE_SIGN_PFX,
                  ou CODE_SIGN_PFX_BASE64 com o conteúdo em base64)
  --subject TEXTO trecho exigido no Subject do certificado
                  (padrão: CN=Marcio Baroni)
  --out DIR       destino do exe assinado (padrão: dist/)
  --skip-build    assina o exe já compilado em target/, sem recompilar
  --skip-checks   repassado ao build.sh (pula o clippy)
  -h, --help      esta ajuda

Senha do PKCS#12: env CODE_SIGN_PASSWORD (se ausente, o script pergunta).
Dependência de sistema: sudo apt install osslsigncode
EOF
    exit "${1:-0}"
}

parse_args() {
    while (($#)); do
        case "$1" in
            --pfx)         pfx_path="${2:?--pfx exige um arquivo}"; shift ;;
            --subject)     expected_subject="${2:?--subject exige um texto}"; shift ;;
            --out)         out_dir="${2:?--out exige um diretório}"; shift ;;
            --skip-build)  skip_build=1 ;;
            --skip-checks) skip_checks=1 ;;
            -h|--help)     usage 0 ;;
            *)             printf 'Opção desconhecida: %s\n\n' "$1" >&2; usage 1 ;;
        esac
        shift
    done
}

# Tudo o que toca a chave privada vive aqui, com permissão só para o dono, e é
# apagado mesmo se o script morrer no meio.
make_tmp_dir() {
    umask 077
    tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/rustshot-codesign.XXXXXX")"
    # shellcheck disable=SC2064  # o caminho tem de ser expandido agora
    trap "rm -rf '$tmp_dir'" EXIT
}

resolve_pfx() {
    if [[ -n "${CODE_SIGN_PFX_BASE64:-}" ]]; then
        pfx_path="$tmp_dir/codesign.pfx"
        printf '%s' "${CODE_SIGN_PFX_BASE64}" | tr -d '[:space:]' | base64 -d > "$pfx_path" ||
            die 'CODE_SIGN_PFX_BASE64 não é base64 válido'
        ok 'PKCS#12 decodificado de CODE_SIGN_PFX_BASE64'
        return
    fi

    [[ -n "$pfx_path" ]] ||
        die 'informe o certificado com --pfx, CODE_SIGN_PFX ou CODE_SIGN_PFX_BASE64'
    [[ -f "$pfx_path" ]] || die "PKCS#12 não encontrado: $pfx_path"
    ok "PKCS#12: $pfx_path"
}

# A senha vai para um arquivo lido pelo osslsigncode (-readpass): em -pass ela
# apareceria na linha de comando, visível a qualquer `ps` da máquina.
resolve_password() {
    local password="${CODE_SIGN_PASSWORD:-}"
    if [[ -z "$password" ]]; then
        [[ -t 0 ]] || die 'defina CODE_SIGN_PASSWORD (sem terminal para perguntar)'
        read -rsp '    Senha do PKCS#12: ' password
        printf '\n'
    fi
    printf '%s' "$password" > "$tmp_dir/password"
    openssl pkcs12 -in "$pfx_path" -nokeys -passin file:"$tmp_dir/password" -out /dev/null 2>/dev/null ||
        die 'senha incorreta ou PKCS#12 ilegível'
    ok 'senha confere com o PKCS#12'
}

build() {
    if ((skip_build)); then
        step 'Build pulado (--skip-build)'
        [[ -f "$UNSIGNED" ]] || die "não há exe compilado em $UNSIGNED"
        return
    fi

    step 'Compilando (delegado ao build.sh)'
    if ((skip_checks)); then
        "$REPO_ROOT/build.sh" --skip-checks
    else
        "$REPO_ROOT/build.sh"
    fi
}

sign() {
    local signed="$1"
    step 'Assinando (Authenticode, SHA-256 + carimbo de tempo RFC 3161)'
    mkdir -p "$out_dir"
    # O carimbo de tempo não é opcional: sem ele a assinatura morre junto com a
    # validade do certificado.
    osslsigncode sign \
        -pkcs12 "$pfx_path" -readpass "$tmp_dir/password" \
        -h sha256 -ts "$TIMESTAMP_URL" \
        -n "$DESCRIPTION" -i "$DESCRIPTION_URL" \
        -in "$UNSIGNED" -out "$signed" ||
        die 'osslsigncode falhou ao assinar'
}

extract_digest() {
    sed -n "s/^$1 message digest[[:space:]]*:[[:space:]]*\([0-9A-Fa-f]\{16,\}\).*/\1/p" | head -1
}

# O exit code do `verify` não serve de veredito: a cadeia de um certificado
# autoassinado é reprovada por definição (é por isso que o CI também não usa
# `signtool verify /pa`), e com -CAfile ele passa a devolver 0 até para uma
# assinatura sem carimbo de tempo. Então se conferem os três fatos que importam,
# lidos da saída: a assinatura casa com os bytes do exe, o signatário é o
# esperado e existe contra-assinatura de carimbo de tempo.
verify() {
    local signed="$1" output current calculated
    step 'Verificando a assinatura'
    output="$(osslsigncode verify -in "$signed" 2>&1 || true)"

    current="$(extract_digest Current <<<"$output")"
    calculated="$(extract_digest Calculated <<<"$output")"
    [[ -n "$current" && "$current" == "$calculated" ]] ||
        die 'o digest da assinatura não corresponde aos bytes do exe'
    ok 'digest da assinatura casa com o exe'

    grep -qF "$expected_subject" <<<"$output" ||
        die "assinatura sem '$expected_subject' no Subject do certificado"
    ok "assinado por: $(grep -m1 -F "$expected_subject" <<<"$output" | sed 's/^[[:space:]]*//')"

    # "Timestamp time:" só aparece na contra-assinatura; sem carimbo o
    # osslsigncode escreve "Timestamp is not available", que um grep por
    # "timestamp" aceitaria por engano.
    grep -qE '^[[:space:]]*Timestamp time:' <<<"$output" ||
        die 'assinatura sem carimbo de tempo — expiraria junto com o certificado'
    ok "carimbo de tempo: $(grep -m1 -E '^[[:space:]]*Timestamp time:' <<<"$output" | sed 's/^[[:space:]]*//')"
}

report() {
    local signed="$1" mb
    mb="$(awk -v b="$(stat -c%s "$signed")" 'BEGIN { printf "%.2f", b / 1048576 }')"
    step 'Artefato assinado'
    printf '    Caminho: %s\n' "$signed"
    printf '    Tamanho: %s MB\n' "$mb"
    warn 'certificado autoassinado: o SmartScreen ainda aparece na primeira execução'
    warn 'a assinatura prova a origem do arquivo, não substitui um certificado de CA'
}

main() {
    local signed
    parse_args "$@"
    cd "$REPO_ROOT"

    command -v osslsigncode >/dev/null ||
        die 'osslsigncode não encontrado. Em Debian/Ubuntu: sudo apt install osslsigncode'

    step 'Certificado'
    make_tmp_dir
    resolve_pfx
    resolve_password

    build

    signed="$out_dir/rustshot.exe"
    sign "$signed"
    verify "$signed"
    report "$signed"

    printf '\n%sConcluído.%s\n' "$C_OK" "$C_OFF"
}

main "$@"
