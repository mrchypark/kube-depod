#!/bin/bash

# 기본 출력 파일 이름
OUTPUT="all_files.md"

# 자기 자신 파일 이름
SELF=$(basename "$0")

# 제외할 경로와 확장자 배열
EXCLUDE_PATHS=()
EXCLUDE_EXTS=()

# 옵션 파싱
while getopts "p:e:o:" opt; do
  case $opt in
    p) EXCLUDE_PATHS+=("$OPTARG") ;;   # 제외할 경로
    e) EXCLUDE_EXTS+=("$OPTARG") ;;    # 제외할 확장자
    o) OUTPUT="$OPTARG" ;;             # 출력 파일명 변경 옵션
    *) echo "Usage: $0 [-p path_to_exclude] [-e ext_to_exclude] [-o output_file]" >&2; exit 1 ;;
  esac
done

# 출력 파일 초기화
> "$OUTPUT"

# find 명령어 준비
FIND_CMD=(find . -type f)

# 경로 제외 조건 추가
for path in "${EXCLUDE_PATHS[@]}"; do
  FIND_CMD+=(! -path "./$path/*")
done

# 확장자 제외 조건 추가
for ext in "${EXCLUDE_EXTS[@]}"; do
  FIND_CMD+=(! -name "*.$ext")
done

# 파일 순회
"${FIND_CMD[@]}" | while read -r file; do
    # 자기 자신과 출력 파일은 제외
    if [[ $(basename "$file") == "$SELF" ]] || [[ $(basename "$file") == "$OUTPUT" ]]; then
        continue
    fi

    echo "## File: $file" >> "$OUTPUT"
    echo '```' >> "$OUTPUT"
    cat "$file" >> "$OUTPUT"
    echo '```' >> "$OUTPUT"
    echo "" >> "$OUTPUT"
done

echo "✅ 모든 파일이 $OUTPUT 에 정리되었습니다."