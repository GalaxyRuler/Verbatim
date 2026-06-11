import re, json

def keep_head(text):
    return re.sub(
        r'<<<<<<< HEAD\r?\n(.*?)=======\r?\n.*?>>>>>>> feature/adaptive-profiles-v1\r?\n',
        r'\1', text, flags=re.S)

def keep_theirs(text, n=1):
    return re.sub(
        r'<<<<<<< HEAD\r?\n.*?=======\r?\n(.*?)>>>>>>> feature/adaptive-profiles-v1\r?\n',
        r'\1', text, flags=re.S, count=n)

for p in ['src-tauri/nsis/installer.nsi', 'src-tauri/src/llm_client.rs',
          'src-tauri/src/portable.rs']:
    s = open(p, encoding='utf-8').read()
    s = keep_head(s)
    assert '<<<<<<<' not in s
    open(p, 'w', encoding='utf-8', newline='\n').write(s)
    print('resolved (head)', p)

# tauri.conf.json: identifier=HEAD, signCommand=theirs (removed), endpoints=HEAD
p = 'src-tauri/tauri.conf.json'
s = open(p, encoding='utf-8').read()
sign_block = re.search(
    r'<<<<<<< HEAD\r?\n.*?signCommand.*?=======\r?\n(.*?)>>>>>>> feature/adaptive-profiles-v1\r?\n',
    s, flags=re.S)
assert sign_block
s = s[:sign_block.start()] + sign_block.group(1) + s[sign_block.end():]
s = keep_head(s)
assert '<<<<<<<' not in s
json.loads(s)
open(p, 'w', encoding='utf-8', newline='\n').write(s)
print('resolved (mixed)', p)
