set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    just --list

test:
    cargo test --all-targets --all-features

test-coverage:
    cargo llvm-cov clean --workspace
    cargo llvm-cov --all-targets --all-features --no-report
    rm -rf target/llvm-cov
    cargo llvm-cov report --html --output-dir target/llvm-cov
    cargo llvm-cov report --lcov --output-path target/llvm-cov/lcov.info
    cargo llvm-cov report --json --summary-only --output-path target/llvm-cov/summary.json
    @echo "HTML coverage: target/llvm-cov/html/index.html"
    @echo "LCOV coverage: target/llvm-cov/lcov.info"
