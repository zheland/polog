#!/usr/bin/env bash

set -Eeuo pipefail

mkdir -p crates/pologger/assets/terminal-captures

for preset in text-compact text-span-path ndjson-compact; do
    output="$( FORCE_COLOR=true cargo -q run -p example -- "$preset" 2>&1 )"
    output="$ cargo -q run -p example -- \"$preset\""$'\n'"$output"
    echo "$output" | bash examples/ansi-to-svg.sh > "crates/pologger/assets/terminal-captures/$preset.svg"
    if [[ "preset" == "text-compact" ]]; then
        echo "$output" | bash examples/ansi-to-html.sh > "crates/pologger/assets/terminal-captures/$preset.html"
    fi
done

