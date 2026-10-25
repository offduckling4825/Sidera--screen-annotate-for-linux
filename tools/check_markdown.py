#!/usr/bin/env python3
"""Check repository Markdown relative links, images and local heading anchors."""
from pathlib import Path
import html
import re
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]


def documents():
    result = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "-co", "--exclude-standard"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False)
    if result.returncode == 0:
        return sorted({ROOT / name for name in result.stdout.decode("utf-8").split("\0")
                       if name.lower().endswith(".md") and (ROOT / name).is_file()})
    excluded = {".git", "target", "node_modules", "vendor", "amd64-sysroot", "aarch64-sysroot"}
    return sorted(p for p in ROOT.rglob("*.md") if not excluded.intersection(p.relative_to(ROOT).parts))


def prose(source):
    lines, fence = [], None
    for line in source.splitlines():
        match = re.match(r"^\s*(`{3,}|~{3,})", line)
        if match:
            marker = match[1]
            if fence is None:
                fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence):
                fence = None
            continue
        if fence is None:
            lines.append(line)
    return "\n".join(lines)


def anchors(source):
    result, counts = set(), {}
    for title in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", prose(source), re.MULTILINE):
        title = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", title)
        slug = re.sub(r"[^\w\- ]", "", html.unescape(title).lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        result.add(slug if count == 0 else f"{slug}-{count}")
    result.update(re.findall(r'''(?:id|name)=["']([^"']+)["']''', source))
    return result


def main():
    files = documents()
    errors, checked = [], 0
    cache = {path.resolve(): path.read_text(encoding="utf-8-sig") for path in files}
    for path, source in cache.items():
        text = prose(source)
        targets = re.findall(r"\[[^\]\n]*\]\(([^)\n]+)\)", text)
        targets += re.findall(r'''(?:src|href)=["']([^"']+)["']''', text)
        for target in targets:
            match = re.match(r"^(?:<([^>]+)>|(\S+))", target.strip())
            if not match:
                continue
            target = html.unescape(match[1] or match[2])
            url = urlsplit(target)
            if url.scheme or url.netloc:
                continue
            destination = (path.parent / unquote(url.path)).resolve() if url.path else path
            checked += 1
            if not destination.exists():
                errors.append(f"{path.relative_to(ROOT)}: missing {target}")
            elif url.fragment and destination.suffix.lower() == ".md":
                content = cache.get(destination)
                if content is None:
                    content = destination.read_text(encoding="utf-8-sig")
                if unquote(url.fragment) not in anchors(content):
                    errors.append(f"{path.relative_to(ROOT)}: missing anchor {target}")
    print(f"Checked {len(files)} Markdown documents and {checked} local links/images")
    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
