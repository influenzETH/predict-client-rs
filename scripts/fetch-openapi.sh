#!/usr/bin/env bash
# Refresh openapi/openapi.json from the live predict.fun docs page.
#
# The API does not expose a standalone OpenAPI URL — the spec is embedded
# as a `let spec = {...};` JS literal inside the openapi-explorer HTML at
# https://api.predict.fun/docs. We curl that page and slice the literal
# out with a tiny Python helper.
#
# Usage:  scripts/fetch-openapi.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/openapi/openapi.json"
DOCS_URL="https://api.predict.fun/docs"

tmp="$(mktemp -t predictfun-docs.XXXXXX.html)"
trap 'rm -f "$tmp"' EXIT

echo "fetching $DOCS_URL"
curl -fsSL "$DOCS_URL" -o "$tmp"

python3 - "$tmp" "$OUT" <<'PY'
import json, sys
html_path, out_path = sys.argv[1], sys.argv[2]
html = open(html_path).read()
start_marker = "let spec = "
end_marker = ";\n        document.getElementsByTagName"
i = html.find(start_marker)
if i < 0:
    sys.exit("could not locate `let spec = ` literal in docs HTML")
j = html.find(end_marker, i)
if j < 0:
    sys.exit("could not locate end of spec literal in docs HTML")
spec_text = html[i + len(start_marker):j]
spec = json.loads(spec_text)
with open(out_path, "w") as f:
    json.dump(spec, f, indent=2)
    f.write("\n")
schemas = len(spec.get("components", {}).get("schemas", {}))
paths = len(spec.get("paths", {}))
print(f"wrote {out_path}: openapi={spec.get('openapi')} version={spec.get('info', {}).get('version')} paths={paths} schemas={schemas}")
PY

echo "done. Re-run \`cargo check\` to regenerate progenitor codegen."
