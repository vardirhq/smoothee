#!/usr/bin/env bash
set -euo pipefail

limit=350
failed=0

while IFS= read -r -d '' file; do
  # Count non-blank production lines only. Inline test modules are intentionally
  # excluded: extensive tests are healthy and should not force runtime splits.
  lines=$(awk '
    /^#\[cfg\(test\)\]/{exit}
    /^[[:space:]]*$/{next}
    {count++}
    END{print count+0}
  ' "$file")
  if (( lines > limit )); then
    printf 'error: %s has %d substantive production lines (limit %d)\n' "$file" "$lines" "$limit" >&2
    failed=1
  fi
done < <(find src -type f -name '*.rs' -print0 | sort -z)

if (( failed )); then
  cat >&2 <<'EOF'

Split oversized modules by responsibility instead of raising the limit.
See ARCHITECTURE.md for the intended dependency and module boundaries.
EOF
  exit 1
fi

printf 'Source-size guard passed: all production Rust modules are <= %d substantive lines.\n' "$limit"
