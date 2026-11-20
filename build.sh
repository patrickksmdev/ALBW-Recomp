#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_ROM="$ROOT_DIR/Legend of Zelda, The - A Link Between Worlds (USA) (En,Fr,Es).cci"
RECOMP_BIN="$ROOT_DIR/3dsrecomp/target/release/3dsrecomp"
PORT_DIR="$ROOT_DIR/albw-port"
PORT_BIN="$PORT_DIR/target/release/albw-native"

usage() {
    cat <<EOF
Uso: ./build.sh <comando> [caminho_da_rom]

Comandos:
  runtime       Recompila apenas o runtime Rust (zakuro + albw-port). Rápido.
  full [rom]    Regera librecomp.a a partir dos overrides/ e recompila tudo.
  run [rom]     Executa o binário albw-native com a ROM informada (ou padrão).
EOF
}

cmd="${1:-}"
rom="${2:-$DEFAULT_ROM}"

case "$cmd" in
    runtime)
        cargo build --release --offline --manifest-path "$PORT_DIR/Cargo.toml"
        ;;
    full)
        if [[ ! -f "$rom" ]]; then
            echo "Erro: ROM não encontrada em: $rom" >&2
            exit 1
        fi
        if [[ ! -x "$RECOMP_BIN" ]]; then
            cargo build --release --offline --manifest-path "$ROOT_DIR/3dsrecomp/Cargo.toml"
        fi
        "$RECOMP_BIN" port "$rom" "$PORT_DIR" \
            --zakuro "$ROOT_DIR/zakuro" \
            --overrides "$ROOT_DIR/overrides" \
            --name albw-native
        cargo build --release --offline --manifest-path "$PORT_DIR/Cargo.toml"
        ;;
    run)
        if [[ ! -f "$rom" ]]; then
            echo "Erro: ROM não encontrada em: $rom" >&2
            exit 1
        fi
        LOG_FILE="$ROOT_DIR/albw-run.log"
        "$PORT_BIN" "$rom" 2>&1 | tee "$LOG_FILE"
        ;;
    *)
        usage
        exit 1
        ;;
esac
