#!/usr/bin/env bash
#
# Deploy API-Classic (a standard JS Cloudflare Worker) to qubepods.
#
# `qube pod deploy` is component-only in the pre-alpha CLI, so a plain JS-module
# qube ships via this direct POST /api/deploy (the API accepts a JS-module
# bundle: qubepod.jsonc + the module source). The platform injects env.KV / env.DB
# / env.BUCKET from the manifest's `imports` — no wrangler.jsonc.
#
# Auth — mint a DEPLOY-scoped project token in the qubepods console
# (app.qubepods.com), then either `qube pod login --url <API> --token <t>`
# (saved in ~/.qube/pods.toml) or pass it via $QUBEPODS_TOKEN. NEVER commit the
# token — this repo is public.
#
# Usage (from anywhere):  api-classic/deploy.sh
# Overrides: QUBEPODS_TOKEN, QUBEPODS_API, QUBEPODS_ENV.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API="${QUBEPODS_API:-https://api.qubepods.com}"
ENVIRONMENT="${QUBEPODS_ENV:-production}"
TOKEN="${QUBEPODS_TOKEN:-$(sed -n 's/.*token *= *"\(qube_[A-Za-z0-9]*\)".*/\1/p' "${HOME}/.qube/pods.toml" 2>/dev/null | head -1)}"
[ -n "${TOKEN:-}" ] || { echo "no deploy token — run: qube pod login --url $API --token <t>" >&2; exit 1; }

ZIP="$(mktemp -u).zip"
trap 'rm -f "$ZIP"' EXIT
( cd "$DIR" && zip -qr "$ZIP" qubepod.jsonc src )

echo "deploying $DIR → $API ($ENVIRONMENT)…"
curl -fsS -X POST "$API/api/deploy" \
  -H "Authorization: Bearer $TOKEN" \
  -F "environment=$ENVIRONMENT" \
  -F "bundle=@$ZIP" \
  -w '\nHTTP %{http_code}\n'
