#!/usr/bin/env bash

cargo +nightly llvm-cov --doctests --branch --all-features
cargo +nightly llvm-cov --doctests --branch --all-features --html
