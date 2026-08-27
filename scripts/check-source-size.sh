#!/usr/bin/env bash
set -euo pipefail

target=400
hard_limit=600
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

  if (( lines > hard_limit )); then
    printf 'error: %s has %d substantive production lines (hard limit %d)\n' \
      "$file" "$lines" "$hard_limit" >&2
    failed=1
  elif (( lines > target )); then
    printf 'warning: %s has %d substantive production lines (target <= %d)\n' \
      "$file" "$lines" "$target"
  fi
done < <(find src -type f -name '*.rs' -print0 | sort -z)

if (( failed )); then
  cat >&2 <<'EOF'

Production modules over 600 lines must be split by responsibility before merging.
Files over 400 lines should be treated as maintainability pressure and split when
there is a natural boundary. See ARCHITECTURE.md for the intended structure.
EOF
  exit 1
fi

printf 'Source-size guard passed: no production Rust module exceeds %d substantive lines.\n' "$hard_limit"
