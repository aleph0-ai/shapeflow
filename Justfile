set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default: help

help:
        @just --list

cargo-check:
        #!/usr/bin/env bash
        set -euo pipefail

        run_check() {
                local label="$1"
                shift
                local log_file
                log_file="$(mktemp)"

                set +e
                "$@" >"${log_file}" 2>&1
                local status=$?
                set -e

                local errors warnings
                errors="$(grep -Ec '^error(\[[A-Z0-9]+\])?:' "${log_file}" || true)"
                warnings="$(grep -Ec '^warning(\[[A-Z0-9]+\])?:' "${log_file}" || true)"

                if [ "${status}" -eq 0 ]; then
                        echo "${label}: errors=${errors} warnings=${warnings} status=ok"
                else
                        echo "${label}: errors=${errors} warnings=${warnings} status=fail"
                fi

                rm -f "${log_file}"
                return "${status}"
        }

        fail=0
        run_check "shapeflow/workspace" cargo check || fail=1

        exit "${fail}"

verus-check:
        #!/usr/bin/env bash
        set -euo pipefail

        mapfile -t files < <(find . -type f -name '*.rs' -not -path './target/*' | sort)
        found=0
        for f in "${files[@]}"; do
                if rg -q 'verus!' "${f}"; then
                        found=1
                        echo "verus ${f}"
                        /home/jack/.local/verus/verus "${f}"
                fi
        done

        if [ "${found}" -eq 0 ]; then
                echo "No Verus entry files found (expected at least one .rs file containing 'verus!')." >&2
                exit 1
        fi

lean-check:
        #!/usr/bin/env bash
        set -euo pipefail

        mapfile -t files < <(find . -type f -name '*.lean' -not -path './.lake/*' | sort)
        if [ "${#files[@]}" -eq 0 ]; then
                echo "No Lean files found (expected at least one .lean file)." >&2
                exit 1
        fi

        for f in "${files[@]}"; do
                echo "lean ${f}"
                lean "${f}"
        done

human-eval:
        mkdir -p target
        cargo run -p shapeflow-cli -- human-eval --bind 127.0.0.1:8080 --sqlite-path target/human_eval.sqlite

clean:
        rm -rf build
        cargo clean
