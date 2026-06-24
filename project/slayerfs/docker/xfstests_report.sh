#!/usr/bin/env bash

set -euo pipefail

INPUT_DIR=""
NO_TAR=false

usage() {
    cat <<EOF
用法: $(basename "$0") <artifact_root 或 artifact_run_dir> [选项]

说明:
  - 如果传入目录包含 results/，则认为它就是一次运行的 artifact_run_dir
  - 否则会在 artifact_root 下选择最新的子目录作为 artifact_run_dir

选项:
  --no-tar    不生成压缩包
  -h, --help  显示帮助

示例:
  $(basename "$0") /tmp/slayerfs-kvm-xfstests/local/redis
  $(basename "$0") /tmp/slayerfs-kvm-xfstests/local/redis/1712345678-123-pid9999
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "${1:-}" in
        --no-tar)
            NO_TAR=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            if [[ -z "$INPUT_DIR" ]]; then
                INPUT_DIR="$1"
                shift
            else
                echo "未知参数: $1" >&2
                usage
            fi
            ;;
    esac
done

if [[ -z "$INPUT_DIR" ]]; then
    usage
fi

if [[ ! -d "$INPUT_DIR" ]]; then
    echo "目录不存在: $INPUT_DIR" >&2
    exit 1
fi

run_dir=""
if [[ -d "$INPUT_DIR/results" ]]; then
    run_dir="$INPUT_DIR"
else
    latest="$(ls -1dt "$INPUT_DIR"/*/ 2>/dev/null | head -n 1 || true)"
    latest="${latest%/}"
    if [[ -z "$latest" || ! -d "$latest/results" ]]; then
        echo "在 $INPUT_DIR 下找不到包含 results/ 的 artifact 运行目录" >&2
        exit 1
    fi
    run_dir="$latest"
fi

results_dir="$run_dir/results"
check_out="$results_dir/check.out"
check_log="$results_dir/check.log"

report_md="$run_dir/report.md"
failures_txt="$run_dir/failures.txt"
summary_txt="$run_dir/summary.txt"

tail_file() {
    local path="$1"
    local lines="$2"
    if [[ -f "$path" ]]; then
        tail -n "$lines" "$path"
    else
        echo "missing: $path"
    fi
}

relpath() {
    local path="$1"
    local base="$2"
    if [[ "$path" == "$base" ]]; then
        echo "."
    elif [[ "$path" == "$base/"* ]]; then
        echo "${path#"$base/"}"
    else
        echo "$path"
    fi
}

bad_files=()
if [[ -d "$results_dir" ]]; then
    while IFS= read -r -d '' f; do
        bad_files+=("$f")
    done < <(find "$results_dir" -type f -name '*.out.bad' -print0 2>/dev/null | sort -z)
fi

{
    echo "artifact_dir: $run_dir"
    echo "generated_at: $(date -Is)"
    echo
    echo "key_files:"
    for f in "slayerfs.log" "xfstests-script.log" "local.config" "results/check.out" "results/check.log"; do
        if [[ -e "$run_dir/$f" ]]; then
            echo "  - $f"
        else
            echo "  - $f (missing)"
        fi
    done
    echo
    echo "failures_count: ${#bad_files[@]}"
    if [[ "${#bad_files[@]}" -gt 0 ]]; then
        echo "failures:"
        for f in "${bad_files[@]}"; do
            echo "  - $(relpath "$f" "$run_dir")"
        done
    fi
    echo
    echo "check_out_tail:"
    tail_file "$check_out" 200
} >"$summary_txt"

{
    echo "# SlayerFS xfstests 测试报告"
    echo
    echo "- artifact_dir: $run_dir"
    echo "- generated_at: $(date -Is)"
    echo
    echo "## 关键文件"
    echo
    echo "- slayerfs.log"
    echo "- xfstests-script.log"
    echo "- local.config"
    echo "- results/check.out"
    echo "- results/check.log"
    echo
    echo "## 汇总"
    echo
    echo '```text'
    cat "$summary_txt"
    echo '```'
    echo
    echo "## 失败用例 (.out.bad)"
    echo
    if [[ "${#bad_files[@]}" -eq 0 ]]; then
        echo "- none"
    else
        for f in "${bad_files[@]}"; do
            rel="$(relpath "$f" "$run_dir")"
            echo "- $rel"
        done
    fi
    echo
    echo "## check.log (tail)"
    echo
    echo '```text'
    tail_file "$check_log" 200
    echo '```'
    echo
    echo "## check.out (tail)"
    echo
    echo '```text'
    tail_file "$check_out" 200
    echo '```'
} >"$report_md"

{
    if [[ "${#bad_files[@]}" -gt 0 ]]; then
        for f in "${bad_files[@]}"; do
            rel="$(relpath "$f" "$run_dir")"
            echo "$rel"
        done
    fi
} >"$failures_txt"

if [[ "$NO_TAR" == false ]]; then
    if command -v tar >/dev/null 2>&1; then
        parent="$(dirname "$run_dir")"
        base="$(basename "$run_dir")"
        tarball="${run_dir}.tar.gz"
        tar -C "$parent" -czf "$tarball" "$base"
    fi
fi

echo "report: $report_md"
echo "summary: $summary_txt"
echo "failures: $failures_txt"
if [[ "$NO_TAR" == false && -f "${run_dir}.tar.gz" ]]; then
    echo "tarball: ${run_dir}.tar.gz"
fi
