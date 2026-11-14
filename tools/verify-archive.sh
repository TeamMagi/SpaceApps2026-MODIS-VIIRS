#!/bin/sh
# Generate or verify SHA-256 checksums for the local FIRMS country archive cache.
# Usage: tools/verify-archive.sh [verify|generate]

set -eu

ARCHIVE_DIR="${ARCHIVE_CACHE_DIR:-.cache/firms-archive}"
MANIFEST="data/archive-manifest.sha256"

usage() {
    cat <<EOF
Usage: $0 [verify|generate]

Modes:
  verify    Verify files and SHA-256 checksums against $MANIFEST (default)
  generate  Regenerate $MANIFEST from CSVs in $ARCHIVE_DIR

Environment:
  ARCHIVE_CACHE_DIR  Local archive directory (default: .cache/firms-archive)
EOF
}

detect_sha_cmd() {
    if command -v sha256sum >/dev/null 2>&1; then
        SHA_CMD="sha256sum"
        SHA_CHECK="sha256sum -c"
    elif command -v shasum >/dev/null 2>&1; then
        SHA_CMD="shasum -a 256"
        SHA_CHECK="shasum -a 256 -c"
    else
        echo "Error: neither 'sha256sum' nor 'shasum' was found in PATH." >&2
        echo "Install GNU coreutils (sha256sum) or Perl/macOS shasum -a 256." >&2
        exit 1
    fi
}

abs_manifest_path() {
    manifest_dir=$(dirname "$MANIFEST")
    manifest_base=$(basename "$MANIFEST")
    if [ ! -d "$manifest_dir" ]; then
        return 1
    fi
    printf '%s/%s\n' "$(cd "$manifest_dir" && pwd)" "$manifest_base"
}

cmd_generate() {
    if [ ! -d "$ARCHIVE_DIR" ]; then
        echo "Error: archive directory '$ARCHIVE_DIR' does not exist." >&2
        exit 1
    fi

    detect_sha_cmd

    manifest_dir=$(dirname "$MANIFEST")
    mkdir -p "$manifest_dir"
    manifest_abs=$(abs_manifest_path)
    tmp_manifest="${manifest_abs}.tmp.$$"

    cd "$ARCHIVE_DIR"
    file_count=$(find . -type f -name '*.csv' | wc -l | tr -d ' ')
    if [ "$file_count" -eq 0 ]; then
        echo "Error: no .csv files found in '$ARCHIVE_DIR'." >&2
        exit 1
    fi

    if ! find . -type f -name '*.csv' | LC_ALL=C sort | xargs $SHA_CMD > "$tmp_manifest"; then
        rm -f "$tmp_manifest"
        echo "Error: failed to compute SHA-256 checksums." >&2
        exit 1
    fi

    mv "$tmp_manifest" "$manifest_abs"
    line_count=$(wc -l < "$manifest_abs" | tr -d ' ')
    echo "Generated $MANIFEST ($line_count files)"
}

cmd_verify() {
    if [ ! -f "$MANIFEST" ]; then
        echo "Error: manifest file '$MANIFEST' not found." >&2
        echo "Run 'tools/verify-archive.sh generate' (or 'make manifest') first." >&2
        exit 1
    fi

    if [ ! -d "$ARCHIVE_DIR" ]; then
        echo "Error: archive directory '$ARCHIVE_DIR' does not exist." >&2
        exit 1
    fi

    detect_sha_cmd
    manifest_abs=$(abs_manifest_path)

    cd "$ARCHIVE_DIR"

    expected_count=$(grep -c '[^[:space:]]' "$manifest_abs" || true)
    if [ "$expected_count" -eq 0 ]; then
        echo "Error: manifest file '$MANIFEST' is empty." >&2
        exit 1
    fi

    actual_count=$(find . -type f -name '*.csv' | wc -l | tr -d ' ')
    missing_count=0
    cr=$(printf '\r')

    # List missing files explicitly before running checksum verification so gaps are obvious.
    while IFS= read -r line || [ -n "$line" ]; do
        line=${line%"$cr"}
        case "$line" in
            ""|\#*) continue ;;
        esac
        rel_path=${line#* }
        rel_path=${rel_path# }
        rel_path=${rel_path#\*}
        if [ ! -f "$rel_path" ]; then
            echo "MISSING: $rel_path" >&2
            missing_count=$((missing_count + 1))
        fi
    done < "$manifest_abs"

    echo "Archive files: actual=$actual_count expected=$expected_count missing=$missing_count"

    checksum_ok=1
    if ! $SHA_CHECK "$manifest_abs"; then
        checksum_ok=0
    fi

    if [ "$missing_count" -ne 0 ] || [ "$actual_count" -ne "$expected_count" ] || [ "$checksum_ok" -ne 1 ]; then
        echo "Archive verification FAILED." >&2
        exit 1
    fi

    echo "Archive verification OK ($actual_count/$expected_count files)."
}

mode="${1:-verify}"
if [ "$#" -gt 1 ]; then
    usage >&2
    exit 1
fi

case "$mode" in
    verify)
        cmd_verify
        ;;
    generate)
        cmd_generate
        ;;
    -h|--help)
        usage
        ;;
    *)
        echo "Error: unknown mode '$mode'." >&2
        usage >&2
        exit 1
        ;;
esac
