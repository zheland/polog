#!/bin/bash

# Quick check of write correctness.

cargo run --features log,slog,tracing,polog,span,context,more-args,write-stderr,send
