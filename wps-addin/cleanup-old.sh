#!/bin/sh
# 清理 Sidera 旧版 WPS 加载项登记与授权缓存（纯 shell/awk，不依赖 Python）。
# 可安全重复执行；只删除旧名称/同一本地桥的旧条目，保留当前 sidera-bridge。
# 用法：
#   cleanup-old.sh                 # 清理当前 $HOME
#   cleanup-old.sh --home /home/u  # 清理指定用户（包安装脚本以 root 调用）
set -eu

HOME_DIR="${HOME:-}"
REMOVE_CURRENT=0
while [ $# -gt 0 ]; do
  case "$1" in
    --home)
      [ -n "${2:-}" ] || { echo "用法: $0 [--home <目录>] [--remove-current]" >&2; exit 2; }
      HOME_DIR="$2"; shift 2 ;;
    --remove-current) REMOVE_CURRENT=1; shift ;;
    *) echo "用法: $0 [--home <目录>] [--remove-current]" >&2; exit 2 ;;
  esac
done
[ -n "$HOME_DIR" ] || { echo "错误：HOME 为空" >&2; exit 2; }
[ -d "$HOME_DIR" ] || exit 0

JS_DIR="$HOME_DIR/.local/share/Kingsoft/wps/jsaddons"
[ -d "$JS_DIR" ] || exit 0
command -v awk >/dev/null 2>&1 || { echo "警告：缺少 awk，跳过旧加载项清理" >&2; exit 0; }

OLD_NAME="screen-annotate-bridge"
CURRENT_NAME="sidera-bridge"
OUR_URL="http://127.0.0.1:16666"

backup_file() {
  file="$1"
  backup="$file.sidera-legacy-backup-$(date +%s)-$$"
  cp -p "$file" "$backup" || return 1
  chmod 600 "$backup" 2>/dev/null || true
  printf '%s' "$backup"
}

preserve_meta() {
  source="$1"
  target="$2"
  chmod --reference="$source" "$target" 2>/dev/null || chmod 644 "$target" 2>/dev/null || true
  if ! chown --reference="$source" "$target" 2>/dev/null; then
    owner="$(stat -c '%u:%g' "$source" 2>/dev/null || true)"
    [ -z "$owner" ] || chown "$owner" "$target" 2>/dev/null || true
  fi
}

clean_publish() {
  file="$1"
  [ -f "$file" ] || return 0
  [ -L "$file" ] && return 0
  tmp="$(mktemp "$(dirname "$file")/.sidera-cleanup.XXXXXX")" || return 0
  if ! awk -v legacy="$OLD_NAME" -v current="$CURRENT_NAME" -v our="$OUR_URL" -v remove_current="$REMOVE_CURRENT" '
    function xml_value(text, key, parts, count, i) {
      count = split(text, parts, "\"")
      for (i = 1; i < count; i++) {
        if (parts[i] ~ (key "[[:space:]]*=[[:space:]]*$")) return parts[i + 1]
      }
      return ""
    }
    function tail_after_element(record, at) {
      at = index(record, "/>")
      if (at > 0) return substr(record, at + 2)
      at = index(record, "</jspluginonline>")
      if (at > 0) return substr(record, at + length("</jspluginonline>"))
      return ""
    }
    BEGIN { RS = "<jspluginonline"; first = 1 }
    {
      if (first) { printf "%s", $0; first = 0; next }
      segment = $0
      sub(/>.*/, "", segment)
      name = xml_value(segment, "name")
      url = xml_value(segment, "url")
      sub(/\/+$/, "", url)
      if (name == legacy) { printf "%s", tail_after_element($0); next }
      if (tolower(url) == our && name != current) { printf "%s", tail_after_element($0); next }
      if (remove_current == 1 && name == current && tolower(url) == our) {
        printf "%s", tail_after_element($0); next
      }
      printf "<jspluginonline%s", $0
    }
  ' "$file" > "$tmp"; then
    rm -f "$tmp"
    return 0
  fi
  if cmp -s "$file" "$tmp"; then
    rm -f "$tmp"
    return 0
  fi
  created="$(backup_file "$file")" || { rm -f "$tmp"; return 0; }
  preserve_meta "$file" "$tmp"
  mv -f "$tmp" "$file"
  echo "已清理旧 WPS 登记：$file（备份：$created）"
}

clean_auth() {
  file="$1"
  [ -f "$file" ] || return 0
  [ -L "$file" ] && return 0
  tmp="$(mktemp "$(dirname "$file")/.sidera-cleanup.XXXXXX")" || return 0
  if ! awk -v legacy="$OLD_NAME" -v current="$CURRENT_NAME" -v our="$OUR_URL" -v remove_current="$REMOVE_CURRENT" '
    function json_value(text, key, parts, count, i) {
      count = split(text, parts, "\"")
      for (i = 1; i < count; i++) {
        if (parts[i] == key) return parts[i + 2]
      }
      return ""
    }
    function brace_delta(line, copy, opens, closes) {
      copy = line; opens = gsub(/\{/, "{", copy)
      copy = line; closes = gsub(/\}/, "}", copy)
      return opens - closes
    }
    function flush_section(i, block, need_comma) {
      for (i = 1; i <= kept_count; i++) {
        block = kept[i]
        sub(/\n+$/, "", block)
        sub(/,[[:space:]]*$/, "", block)
        need_comma = (i < kept_count || have_namelist)
        if (need_comma) block = block ","
        print block
      }
      if (have_namelist) {
        printf "%s\"namelist\": \"%s\"\n", namelist_indent, kept_ids
      }
      kept_count = 0
      kept_ids = ""
      have_namelist = 0
    }
    BEGIN { depth = 0; in_entry = 0 }
    {
      line = $0
      if (in_entry) {
        buffer = buffer line "\n"
        if (line ~ /"name"[[:space:]]*:/) entry_name = json_value(line, "name")
        if (line ~ /"path"[[:space:]]*:/) entry_path = json_value(line, "path")
        depth += brace_delta(line)
        if (depth == entry_depth - 1) {
          normalized_path = tolower(entry_path)
          sub(/\/+$/, "", normalized_path)
          if (!(entry_name == legacy ||
                (normalized_path == our && entry_name != current) ||
                (remove_current == 1 && normalized_path == our && entry_name == current))) {
            if (entry_name == current && normalized_path == our) {
              # WPS 可能把当前加载项保留成 enable:false，导致每次重启都再次询问。
              gsub(/"enable"[[:space:]]*:[[:space:]]*false/, "\"enable\": true", buffer)
              gsub(/"isload"[[:space:]]*:[[:space:]]*false/, "\"isload\": true", buffer)
            }
            kept[++kept_count] = buffer
            kept_ids = kept_ids (kept_ids == "" ? "" : ";") entry_key
          }
          in_entry = 0
          buffer = ""
          entry_name = ""
          entry_path = ""
        }
        next
      }
      if (depth == 2 && line ~ /^[[:space:]]*"[^"]+"[[:space:]]*:[[:space:]]*\{[[:space:]]*$/) {
        entry_key = line
        sub(/^[[:space:]]*"/, "", entry_key)
        sub(/".*/, "", entry_key)
        entry_depth = depth + brace_delta(line)
        depth = entry_depth
        in_entry = 1
        buffer = line "\n"
        entry_name = ""
        entry_path = ""
        next
      }
      if (depth == 2 && line ~ /"namelist"[[:space:]]*:/) {
        have_namelist = 1
        namelist_indent = substr(line, 1, index(line, "\"namelist\"") - 1)
        next
      }
      if (depth == 2 && line ~ /^[[:space:]]*\}[,]?[[:space:]]*$/) {
        flush_section()
        print line
        depth += brace_delta(line)
        next
      }
      print line
      depth += brace_delta(line)
    }
  ' "$file" > "$tmp"; then
    rm -f "$tmp"
    return 0
  fi
  if cmp -s "$file" "$tmp"; then
    rm -f "$tmp"
    return 0
  fi
  created="$(backup_file "$file")" || { rm -f "$tmp"; return 0; }
  preserve_meta "$file" "$tmp"
  mv -f "$tmp" "$file"
  echo "已清理旧 WPS 授权：$file（备份：$created）"
}

clean_publish "$JS_DIR/publish.xml" || true
clean_auth "$JS_DIR/authaddin.json" || true
exit 0
