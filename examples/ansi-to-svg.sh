#!/usr/bin/env bash

set -Eeuo pipefail

output="$(
LC_ALL=C.UTF-8 gawk '
    BEGIN {
        escape = sprintf("%c", 27)

        remap["30"] = "#1c1c1c"
        remap["31"] = "#bc5653"
        remap["32"] = "#909d63"
        remap["33"] = "#ddb46d"
        remap["34"] = "#7eaac7"
        remap["35"] = "#aa6292"
        remap["36"] = "#86d3ce"
        remap["37"] = "#cacaca"
        remap["90"] = "#636363"
        remap["91"] = "#f79a94"
        remap["92"] = "#cedda0"
        remap["93"] = "#fedda4"
        remap["94"] = "#b3e0ff"
        remap["95"] = "#ec9fd1"
        remap["96"] = "#b1fff9"
        remap["97"] = "#f7f7f7"

        x_max   = 0
        y_max   = 0
    }

    {
        x       = 0
        y       = NR - 1
        input   = $0
        output  = ""
        while (match(input, escape "\\[[0-9;]*m")) {
            i1 = RSTART
            l1 = RLENGTH

            segment = substr(input, 1, i1 - 1)
            output = output write_segment(segment, 0, "")

            seq = substr(input, i1 + 2, l1 - 3)
            if (substr(seq, 1, 2) == "1;") {
                bold = 1
                color = substr(seq, 3)
            } else {
                bold = 0
                color = seq
            }

            input = substr(input, i1 + l1)
            match(input, escape "\\[0m")
            i2 = RSTART
            l2 = RLENGTH
 
            segment = substr(input, 1, i2 - 1)
            output = output write_segment(segment, bold, color)

            input   = substr(input, i2 + l2)
        }

        segment = input
        output = output write_segment(segment, 0, "")

        printf "%s", output
    }

    END {
        printf "        <!-- SIZE %d %d -->\n", y_max, x_max
    }

    function write_segment(segment, bold, color) {
        svg_x = 8 * x + 8
        svg_y = 17 * y + 8
        svg_len = 8 * length(segment)

        x = x + length(segment)
        x_max = max(x_max, 8 * x + 8 + 16)
        y_max = max(y_max, 17 * y + 8 + 25)

        if (match(segment, /^ *$/)) { return "" }

        # Implicitly updates `segment`.
        gsub(/&/, "\\&amp;", segment)
        gsub(/</, "\\&lt;", segment)
        gsub(/>/, "\\&gt;", segment)
        gsub(/"/, "\\&quot;", segment)
        gsub(/'"'"'/, "\\&#39;", segment)

        item = "        <text x=\"" svg_x "\" y=\"" svg_y "\" textLength=\"" svg_len "\""
        if (bold == 1) {
            item = item " class=\"bold\""
        }
        if (color != "") {
            color = remap[color]
            item = item " style=\"fill: " color "\""
        }
        item = item ">"

        item = item segment
        item = item "</text>\n"

        return item
    }

    function max(lhs, rhs) {
        if (lhs > rhs) { return lhs } else { return rhs }
    }
')"

size="$( echo "$output" | rg -o "^ *<!-- SIZE [0-9]* [0-9]* -->$" )"
# output="$( echo "$output" | rg -v "^ *<!-- SIZE [0-9]* [0-9]* -->$" )"
height="$( echo "$size" | awk '{ print $3 }' )"
width="$( echo "$size" | awk '{ print $4 }' )"

printf "%s" '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 '"$width"' '"$height"'" width="'"$width"'" height="'"$height"'">
    <defs>
        <style type="text/css">
            .screen {
                font-family: "DejaVu Sans Mono", ui-monospace, monospace;
                font-style: normal;
                font-size: 14px;
            }
            text {
                dominant-baseline: text-before-edge;
                white-space: pre;
                fill: #cccccc;
            }
            .background { fill: #2a2a2a; }
            .bold { font-weight: bold; }
        </style>
    </defs>
    <g class="screen">
        <rect class="background" x="0" y="0" width="100%" height="100%"/>
'"$output"'
    </g>
</svg>'
