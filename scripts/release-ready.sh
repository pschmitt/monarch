#!/usr/bin/env bash
set -euo pipefail

case "$EVENT_NAME" in
  push)
    ready="$(gh api "repos/$GITHUB_REPOSITORY/commits/$GITHUB_SHA/status" --jq '[.statuses[] | select(.context == "circus/artifacts" and .state == "success")] | length')"
    if [[ "$ready" != 0 ]]
    then
      tags="$(printf '%s\n' "$GITHUB_REF_NAME" | jq -Rsc 'split("\n") | map(select(length > 0))')"
    else
      tags='[]'
    fi
    ;;
  status)
    tags="$(git tag --points-at "$GITHUB_SHA" --list 'v*' | jq -Rsc 'split("\n") | map(select(length > 0))')"
    ;;
  *)
    echo "Unexpected release event: $EVENT_NAME" >&2
    exit 1
    ;;
esac
echo "tags=$tags" >> "$GITHUB_OUTPUT"
echo "ready=$(jq -r 'length > 0' <<< "$tags")" >> "$GITHUB_OUTPUT"
