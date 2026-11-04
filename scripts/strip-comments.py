import sys

def strip_rust(src):
    i = 0
    n = len(src)
    out = []
    removed_any = False
    while i < n:
        c = src[i]
        nxt = src[i+1] if i+1 < n else ''
        if c == '/' and nxt == '/':
            removed_any = True
            while i < n and src[i] != '\n':
                i += 1
            continue
        if c == '/' and nxt == '*':
            removed_any = True
            depth = 1
            i += 2
            while i < n and depth > 0:
                if src[i] == '/' and i+1 < n and src[i+1] == '*':
                    depth += 1
                    i += 2
                elif src[i] == '*' and i+1 < n and src[i+1] == '/':
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        if c == 'r' and (nxt == '"' or nxt == '#'):
            j = i+1
            hashes = 0
            while j < n and src[j] == '#':
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                out.append(src[i:j+1])
                i = j+1
                close = '"' + ('#'*hashes)
                while i < n:
                    if src[i] == '"' and src[i:i+1+hashes] == close:
                        out.append(src[i:i+1+hashes])
                        i += 1+hashes
                        break
                    else:
                        out.append(src[i])
                        i += 1
                continue
        if c == 'b' and nxt == 'r' and i+2 < n and (src[i+2] == '"' or src[i+2] == '#'):
            out.append('b')
            i += 1
            continue
        if c == '"':
            out.append(c)
            i += 1
            while i < n:
                if src[i] == '\\':
                    out.append(src[i:i+2])
                    i += 2
                    continue
                out.append(src[i])
                if src[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        if c == "'":
            if nxt == '\\':
                j = i+2
                while j < n and src[j] != "'":
                    j += 1
                out.append(src[i:j+1])
                i = j+1
                continue
            if i+2 < n and src[i+2] == "'":
                out.append(src[i:i+3])
                i += 3
                continue
            out.append(c)
            i += 1
            continue
        out.append(c)
        i += 1
    text = ''.join(out)
    if not removed_any:
        return None
    orig_lines = src.split('\n')
    new_lines = text.split('\n')
    result = []
    for idx, nl in enumerate(new_lines):
        stripped = nl.rstrip()
        ol = orig_lines[idx] if idx < len(orig_lines) else ''
        if stripped == '' and ol.strip() != '':
            continue
        result.append(stripped)
    final = '\n'.join(result)
    while '\n\n\n' in final:
        final = final.replace('\n\n\n', '\n\n')
    if src.endswith('\n') and not final.endswith('\n'):
        final += '\n'
    return final

def main():
    args = sys.argv[1:]
    check = False
    if args and args[0] == '--check':
        check = True
        args = args[1:]
    changed = 0
    offenders = []
    for path in args:
        try:
            with open(path, 'r', encoding='utf-8') as f:
                src = f.read()
        except Exception:
            continue
        res = strip_rust(src)
        if res is None or res == src:
            continue
        if check:
            offenders.append(path)
            continue
        with open(path, 'w', encoding='utf-8') as f:
            f.write(res)
        changed += 1
    if check:
        if offenders:
            sys.stderr.write('comments found in non-vendored code:\n')
            for p in offenders:
                sys.stderr.write('  ' + p + '\n')
            sys.exit(1)
        sys.exit(0)
    print('changed', changed)

main()
