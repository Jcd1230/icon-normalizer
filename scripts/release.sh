#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <version>" >&2
  exit 1
fi

version="$1"
tag="v${version}"

if ! GIT_TERMINAL_PROMPT=0 SSH_ASKPASS=/bin/false timeout 15s jj git fetch --all-remotes; then
  echo "manual authentication is needed before releasing; run 'jj git fetch' and authenticate your SSH key first" >&2
  exit 1
fi

if [[ -n "${JIRI_RELEASE_TRUNK:-}" ]]; then
  trunk="${JIRI_RELEASE_TRUNK}"
else
  # Check if there is a tracked bookmark matching main or master, or just get the first bookmark
  trunk="$(jj bookmark list --tracked -r 'trunk()' -T 'name ++ "\n"' | head -n1)"
fi

if [[ -z "${trunk}" ]]; then
  # Fallback: if 'trunk()' query returns nothing, use 'main' as default since we just created it
  trunk="main"
fi

jj bookmark set "${trunk}" -r @
jj git push --bookmark "${trunk}"

gh release create "${tag}" --target "${trunk}" --generate-notes
