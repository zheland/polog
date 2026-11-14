#!/bin/bash

# Compile-time and execution time benchmark for fixture.

set -euo pipefail

MODE="${1:-build}"
REPEATS="${2:-1}"

CRATE="$(dirname -- "${BASH_SOURCE[0]}")"
FIXTURE="$CRATE/Cargo.toml"
TARGET="$CRATE/target/bench"
TIMETEMP="$CRATE/target/time.txt"

[[ -f "$FIXTURE" ]] || { echo "missing $FIXTURE" >&2; exit 1; }


PARAMS=(
  "default:complex,more-args"
  "log:complex,more-args,log"
  "slog:complex,more-args,slog"
  "polog:complex,more-args,polog"
  "tracing:complex,more-args,tracing"
  "polog-span:complex,more-args,polog,span"
  "tracing-span:complex,more-args,tracing,span"
)

declare -A data

for profile in "release" "dev"; do
    for mode in "send" "write"; do
        for params in "${PARAMS[@]}"; do
            IFS=: read -r name features <<< "$params"

            target="$TARGET/$profile-$name"

            printf "\n================ %s %s %s ================\n\n" \
                "$profile" "$mode" "$name"

            rm -rf "$target"
            mkdir -p "$target"
            CARGO_TARGET_DIR="$target" cargo fetch --manifest-path "$FIXTURE"

            set -x
            /usr/bin/time -f "%e" -o "$TIMETEMP" -- \
                env CARGO_TARGET_DIR="$target" \
                cargo build -q --profile $profile --manifest-path "$FIXTURE" \
                    --features "$features,$mode"

            buildtime="$( cat "$TIMETEMP" )"

            /usr/bin/time -f "%e" -o "$TIMETEMP" -- \
                env CARGO_TARGET_DIR="$target" \
                cargo run -q --profile $profile --manifest-path "$FIXTURE" \
                    --features "$features,$mode" 2> /dev/null

            runtime="$( cat "$TIMETEMP" )"
            set +x

            # printf -v tmp "%-8s %-6s %-12s %6s %6s\n" "$profile" "$mode" "$name" "$buildtime" "$runtime"
            # raw_data+="$tmp"

            data["build:$profile:$mode:$name"]="$buildtime"
            data["run:$profile:$mode:$name"]="$runtime"


            bdfd="${data[build:dev:write:default]:-0.01}"
            ndsd="${data[build:dev:send:default]:-0.01}"
            brfd="${data[build:release:write:default]:-0.01}"
            brsd="${data[build:release:send:default]:-0.01}"

            printf "\n"
            printf "| Facade            | Dev Format | Dev Send   | Rel Format | Rel Send   |\n"
            printf "|:-----------------:|:----------:|:----------:|:----------:|:----------:|\n"
            printf "| Baseline          | %7.1fs   | %7.1fs   | %7.1fs   | %7.1fs   |\n" \
                "$bdfd" "$ndsd" "$brfd" "$brsd"

            for params2 in "${PARAMS[@]}"; do
                IFS=: read -r name2 features2 <<< "$params2"
                if [[ $name2 != "default" ]]; then
                    bdfc="${data[build:dev:write:$name2]:-0.01}"
                    ndsc="${data[build:dev:send:$name2]:-0.01}"
                    brfc="${data[build:release:write:$name2]:-0.01}"
                    brsc="${data[build:release:send:$name2]:-0.01}"

                    printf "| %-17s | %+7.0f%%   | %+7.0f%%   | %+7.0f%%   | %+7.0f%%   |\n" \
                        "$name2" \
                        $( echo "scale=2; 100 * ( $bdfc / $bdfd - 1 )" | bc) \
                        $( echo "scale=2; 100 * ( $ndsc / $ndsd - 1 )" | bc) \
                        $( echo "scale=2; 100 * ( $brfc / $brfd - 1 )" | bc) \
                        $( echo "scale=2; 100 * ( $brsc / $brsd - 1 )" | bc)
                fi
            done

            bdfd="${data[run:dev:write:default]:-0.01}"
            ndsd="${data[run:dev:send:default]:-0.01}"
            brfd="${data[run:release:write:default]:-0.01}"
            brsd="${data[run:release:send:default]:-0.01}"

            printf "\n"
            printf "| Facade            | Dev Format | Dev Send   | Rel Format | Rel Send   |\n"
            printf "|:-----------------:|:----------:|:----------:|:----------:|:----------:|\n"
            printf "| Baseline          | %7.1fs   | %7.1fs   | %7.1fs   | %7.1fs   |\n" \
                "$bdfd" "$ndsd" "$brfd" "$brsd"

            for params2 in "${PARAMS[@]}"; do
                IFS=: read -r name2 features2 <<< "$params2"
                if [[ $name2 != "default" ]]; then
                    bdfc="${data[run:dev:write:$name2]:-0}"
                    ndsc="${data[run:dev:send:$name2]:-0}"
                    brfc="${data[run:release:write:$name2]:-0}"
                    brsc="${data[run:release:send:$name2]:-0}"

                    printf "| %-17s | %7.1f×   | %7.1f×   | %7.1f×   | %7.1f×   |\n" \
                        "$name2" \
                        $( echo "scale=2; $bdfc / $bdfd" | bc) \
                        $( echo "scale=2; $ndsc / $ndsd" | bc) \
                        $( echo "scale=2; $brfc / $brfd" | bc) \
                        $( echo "scale=2; $brsc / $brsd" | bc)
                fi
            done
        done
    done
done
