#!/usr/bin/env bash
#
# curl fallback for `qube pod deploy` (q64-lang/q64): pack the QubePod
# bundle (qubepod.jsonc + component/blackbird.wasm + web/ assets) and POST
# it to qubepods. Prefer the qube CLI where available:
#
#     QUBEPODS_TOKEN=… qube pod deploy --url https://api.qubepods.com
#
# Token: $QUBEPODS_TOKEN, or $qube (the name used in the Claude Code
# environment). Environment: $1, default "production".
#
set -euo pipefail
cd "$(dirname "$0")/.."

TOKEN="${QUBEPODS_TOKEN:-${qube:-}}"
[ -n "$TOKEN" ] || { echo "set QUBEPODS_TOKEN (or qube)" >&2; exit 1; }
ENVIRONMENT="${1:-production}"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp qubepod.jsonc "$STAGE/"
mkdir -p "$STAGE/component"
cp component/blackbird.wasm "$STAGE/component/blackbird.wasm"
cp -r web "$STAGE/web"

(cd "$STAGE" && zip -qr bundle.zip qubepod.jsonc component web)

curl -sS --fail-with-body -X POST https://api.qubepods.com/api/deploy \
  -H "Authorization: Bearer $TOKEN" \
  -F "environment=$ENVIRONMENT" \
  -F "bundle=@$STAGE/bundle.zip"
echo
