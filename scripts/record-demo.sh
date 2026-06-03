#!/usr/bin/env bash
set -euo pipefail

# Records an asciinema demo of claudex by building a cast file directly.
# Each command is "typed" character-by-character with realistic delays,
# then the real output is spliced in.
#
# Usage:
#   ./scripts/record-demo.sh [output-dir]
#   UPLOAD=1 ./scripts/record-demo.sh          # record + upload
#   CLAUDEX_BIN=/path/to/claudex ./scripts/...  # override binary

CLAUDEX="${CLAUDEX_BIN:-/Users/jamesbrink/.local/bin/claudex}"
OUT_DIR="${1:-/tmp/asciinema}"
CAST="$OUT_DIR/claudex-demo.cast"
COLS=180
ROWS=35

# Average human types ~60 WPM = ~300 CPM = ~50ms per char.
# Good typist ~80-100 WPM = ~25-35ms per char.
TYPING_MIN_MS=17
TYPING_MAX_MS=35

# Output renders instantly (like a real terminal)
OUTPUT_LINE_MS=2

# Pauses between steps
PAUSE_BEFORE_TYPING_MS=200
PAUSE_AFTER_ENTER_MS=50
PAUSE_BETWEEN_CMDS_MS=4000
PAUSE_INITIAL_MS=500
PAUSE_FINAL_MS=800

mkdir -p "$OUT_DIR"

NEXT_DELAY=0

set_delay_ms() {
  NEXT_DELAY=$1
}

random_ms() {
  echo $(( $1 + RANDOM % ($2 - $1 + 1) ))
}

emit_event() {
  local ts
  ts=$(echo "$NEXT_DELAY / 1000" | bc -l)
  ts=$(printf '%.3f' "$ts")
  local data
  data=$(printf '%s' "$1" | python3 -c 'import sys,json; print(json.dumps(sys.stdin.read()), end="")')
  echo "[$ts, \"o\", $data]"
  NEXT_DELAY=0
}

PROMPT=$'\033[1;32m❯\033[0m '

type_string() {
  local text="$1"
  for (( i=0; i<${#text}; i++ )); do
    set_delay_ms "$(random_ms $TYPING_MIN_MS $TYPING_MAX_MS)"
    emit_event "${text:$i:1}"
  done
}

run_cmd() {
  local display_cmd="$1"
  local real_cmd="$2"

  # NEXT_DELAY carries the between-commands pause from previous call
  # (or PAUSE_INITIAL_MS on first run). Add the pre-typing gap.
  NEXT_DELAY=$(( NEXT_DELAY + PAUSE_BEFORE_TYPING_MS ))
  emit_event "$PROMPT"

  type_string "$display_cmd"

  # hit enter
  set_delay_ms "$PAUSE_AFTER_ENTER_MS"
  emit_event $'\r\n'

  # run command, emit output nearly instantly (like a real terminal flush)
  local output
  output=$($real_cmd 2>&1) || true

  local first=1
  while IFS= read -r line; do
    set_delay_ms "$OUTPUT_LINE_MS"
    emit_event "${line}"$'\r\n'
  done <<< "$output"

  # user reads the output — set delay for NEXT event
  NEXT_DELAY="$PAUSE_BETWEEN_CMDS_MS"
}

COMMANDS=(
  "claudex --version"
  "claudex summary"
  "claudex cost --limit 5"
  "claudex models"
  "claudex tools --limit 10"
  "claudex sessions --limit 5"
)

echo "Recording to $CAST ..."

{
  cat <<HEADER
{"version":3,"term":{"cols":$COLS,"rows":$ROWS},"timestamp":$(date +%s),"idle_time_limit":5.0,"title":"claudex — CLI session analytics","env":{"SHELL":"/bin/zsh"}}
HEADER

  NEXT_DELAY="$PAUSE_INITIAL_MS"

  for cmd in "${COMMANDS[@]}"; do
    real_cmd="$CLAUDEX ${cmd#claudex } --color always"
    run_cmd "$cmd" "$real_cmd"
  done

  set_delay_ms "$PAUSE_FINAL_MS"
  emit_event "$PROMPT"

} > "$CAST"

EVENTS=$(wc -l < "$CAST")
DURATION=$(python3 -c "
import json, sys
total=0
with open('$CAST') as f:
    next(f)
    for l in f: total+=json.loads(l)[0]
print(f'{total:.1f}')
")
echo "Done: $CAST (${DURATION}s, $EVENTS events)"
echo ""

if [[ "${UPLOAD:-}" == "1" ]]; then
  echo "Uploading..."
  asciinema upload --title "claudex — CLI session analytics" --visibility unlisted "$CAST"
fi
