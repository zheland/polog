#!/usr/bin/env bash

set -Eeuo pipefail

printf "<pre style=\"color: #cccccc; background-color: #2a2a2a;\"><code>"

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
    }

    {
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

        printf "%s\n", output
    }

    function write_segment(segment, bold, color) {
        # Implicitly updates `segment`.
        gsub(/&/, "\\&amp;", segment)
        gsub(/</, "\\&lt;", segment)
        gsub(/>/, "\\&gt;", segment)
        gsub(/"/, "\\&quot;", segment)
        gsub(/'"'"'/, "\\&#39;", segment)

        item = ""

        if (color != "") {
            color = remap[color]
            item = item "<span style=\"color: " color ";\">"
        }
        if (bold == 1) { item = item "<b>" }
        item = item segment
        if (bold == 1) { item = item "</b>" }
        if (color != "") { item = item "</span>" }

        return item
    }
'

printf "</code></pre>"
