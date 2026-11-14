#!/usr/bin/env bash

set -Eeuo pipefail


main() {
    trap cleanup EXIT

    echo_and_run cargo +nightly fmt --all -- --check
    echo_and_run cargo outdated --exit-code 1
    echo_and_run cargo deny --workspace --all-features check

    # Check README and docs are synchronized.
    crates=(
        'polog,polog'
        'polog-util,polog_util'
        'pologger,pologger'
    );
    for crate in "${crates[@]}"; do
        IFS=, read -r name name_lc <<< "$crate"
        crate_docs_link="https://docs\.rs/${name}/latest/${name_lc}"
        readme="$( cat "crates/${name}/README.md" | normalize_docs "README.md" )"
        libdoc="$( cat "crates/${name}/src/lib.rs" | normalize_docs "lib.rs" )"
        echo_and_run git diff <(echo "$readme") <(echo "$libdoc")
    done

    check_docs_valid_terminal_captures_version

    echo_and_run env RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" \
        cargo +nightly doc --all-features
    echo_and_run cargo +nightly llvm-cov --doctests --all-features --html

    echo_and_run bash examples/update-terminal-captures.sh

    params_sets=(
        'stable,'
        'beta,'
        'nightly,'
        '1.100.0,'
        'stable,--release'
        'nightly,--release'
    );
    for params in "${params_sets[@]}"; do
        IFS=, read -r toolchain flags <<< "$params"
        echo_and_run cargo +$toolchain clippy --all-targets $flags -- -D warnings
        echo_and_run cargo +$toolchain clippy --all-targets --all-features $flags -- -D warnings
        echo_and_run cargo +$toolchain test --all-targets $flags
        echo_and_run cargo +$toolchain test --all-targets --all-features $flags
        echo_and_run cargo +$toolchain test --doc $flags
        echo_and_run cargo +$toolchain test --doc --all-features $flags
    done

    echo_and_run cargo +nightly miri test --all-targets --all-features

    # Check no-std support.
    for target in "x86_64-unknown-none" "thumbv7em-none-eabihf"; do
        echo_and_run cargo hack check \
            -p polog -p polog-util \
            --target $target \
            --feature-powerset \
            --exclude-features "\
default,std,test-util,fanout,\
offloader,offloader-support-tokio,\
shared,shared-filter,\
tracing-0-1-semver-unstable"
    done

    # Check different combinations of features.
    for flags in "" "--release"; do
        for crate in "polog" "polog-util" "pologger"; do
            echo_and_run cargo hack clippy -p $crate --all-targets \
                --feature-powerset $flags --depth 2 -- -D warnings
            echo_and_run cargo hack test -p $crate --all-targets \
                --feature-powerset $flags --depth 2
            echo_and_run cargo hack test -p $crate --doc \
                --feature-powerset $flags --depth 1
        done
    done

    ok "All checks succeeded." 1>&2
}

exec() { "$@"; }
passthru() { tee; }
comment() { tee; }
ok() { echo $'\033[1;32m'"OK"$'\033[0m'": $@" 1>&2; }
fail() { echo $'\033[1;31m'"ERROR"$'\033[0m'": $@" 1>&2; exit 1; }
echo_and_run() { echo "$ ${*@Q}"; "$@"; }

cleanup() {
    local return_code=$?
    exit $return_code
}

# Apply some transformations to lib.rs and README.md, to get the documentation
# in a single intermediate format, which is expected to be the same for both:
# lib.rs and README.md.
normalize_docs() {
    source=$1
    local libdoc=false
    local readme=false
    if [[ $source == "lib.rs" ]]; then
        libdoc=exec
        readme=passthru
    elif [[ $source == "README.md" ]]; then
        libdoc=passthru
        readme=exec
    else
        fail "Internal script error: invalid docs source."
    fi

    set +eo pipefail

    cat \
        | comment "Remove title and badges from README.md" \
        | $readme awk '
            BEGIN { is_content=0 }
            is_content==1 { print; next }
            /^(# .*||!\[.*|\[!\[.*)$/ { next } # Ignore title, empty line and images.
            is_content==0 { is_content=1; print; next }
        ' \
        | comment "Remove svg assets from README.md" \
        | $readme rg -v $'assets/terminal-captures/' \
        | comment "Remove svg assets from lib.rs docs" \
        | $libdoc rg -v $'assets/terminal-captures/' \
        | comment "Remove non-'//!' lines from lib.rs" \
        | $libdoc rg --passthru '#!\[doc = ""\]' -r '//!' \
        | $libdoc rg '^//!' \
        | $libdoc rg '^//! ?' -r '' \
        | comment "Add "C " prefix for markdown code lines." \
        | comment "Add "T " prefix for other markdown lines." \
        | awk '
            BEGIN { is_code=0 }
            /^```.*$/ { print "C " $0; is_code=!is_code; next }
            is_code==0 { print "T " $0; next }
            is_code==1 { print "C " $0; next }
        ' \
        | comment "Remove hidden portions from lib.rs doc examples." \
        | $libdoc rg -v $'^C #( |$)' \
        | comment "Decrease all headers level in README.md." \
        | $readme rg --passthru $'^T #(#+) (.*)' -r $'T $1 $2' \
        | comment "Convert rust stdlib links to rust paths in README.md." \
        | comment "Use "L " prefix for markdown lines with link references." \
        | $readme rg --passthru \
            $'^T \[(.*)\]: https://doc\.rust-lang\.org/(std|core)/(?:(?:(?:([^/]*)/)?([^/]*)/)?([^/]*)/)?([^/]*\.html)(#| |$)' \
            -r $'L [$1]: $2::$3::$4::$5::$6$7' \
        | comment "Convert rust stdlib module links to rust paths in README.md." \
        | $readme rg --passthru \
            $'^T \[(.*)\]: https://doc\.rust-lang\.org/(std|core)/(?:(?:(?:([^/]*)/)?([^/]*)/)?([^/]*)/)?index\.html(#| |$)' \
            -r $'L [$1]: $2::$3::$4::$5$6' \
        | comment "Convert crate links to rust paths in README.md." \
        | $readme rg --passthru \
            $'^T \[(.*)\]: '"$crate_docs_link"$'/(?:(?:(?:([^/]*)/)?([^/]*)/)?([^/]*)/)?([^/]*\.html)(#| |$)' \
            -r $'L [$1]: $2::$3::$4::$5$6' \
        | comment "Convert crate module links to rust paths in README.md." \
        | $readme rg --passthru \
            $'^T \[(.*)\]: '"$crate_docs_link"$'/(?:(?:(?:([^/]*)/)?([^/]*)/)?([^/]*)/)?index\.html(#| |$)' \
            -r $'L [$1]: $2::$3::$4$5' \
        | comment "Remove artifact "::::" and starting "::" in paths after the previous link conversions." \
        | $readme rg --passthru \
            $'^L \[(.*)\]: ([^:]*)::(?:::)+' -r $'L [$1]: $2::' \
        | $readme rg --passthru \
            $'^L \[(.*)\]: ::' -r $'L [$1]: ' \
        | comment "Remove link titles that now duplicates rust path in README.md." \
        | $readme rg --pcre2 --passthru \
            $'^L \[(.*)\]: ([^ ]*) "\\2"' -r $'L [$1]: $2' \
        | comment "Remove link references that now duplicates rust path in README.md" \
        | $readme rg --pcre2 -v $'^L \[`(.*)`\]: \\1$' \
        | $readme rg -U --pcre2 --passthru $'^(L .*)\nT \n(?:T \n)+(L .*)$' -r $'$1\n\n$2' \
        | comment "Remove temporary prefixes." \
        | rg --passthru $'T ' -r '' \
        | rg --passthru $'C ' -r '' \
        | rg --passthru $'L ' -r '' \
        | tee

    set -eo pipefail
}

check_docs_valid_terminal_captures_version() {
    local version="$( rg ^version Cargo.toml | rg "version = |\"" -r "" )"
    local count="$( rg "refs/tags/v$version/" crates/pologger/src/lib.rs | wc -l )"

    if [[ "$count" -ne 3 ]]; then
        fail "Wrong version of terminal-captures in crates/pologger/src/lib.rs"
    fi
}

main
