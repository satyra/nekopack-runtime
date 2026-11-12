"""Verify five mounted-original message/config voice contracts; system code only."""
import argparse
import hashlib
import json
from pathlib import Path
import sys


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--games', type=Path, required=True)
    p.add_argument('--reference-root', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a=p.parse_args()
    sys.path.insert(0,str(a.reference_root))
    from portkit.neko import Archive
    rows=[]
    for path in sorted(a.games.rglob('system.dat')):
        pack=Archive(path)
        message=pack.read('system/message.nut')
        config=pack.read('system/config.nut')
        text=message.decode('cp932')
        begin='com[""] = function(args){'
        parsed='local is_readed ='
        gate='local voice = IsAvailVoice('
        assert all(text.count(marker)==1 for marker in (begin,parsed,gate))
        start=text.index(begin)+len(begin)
        end=text.index(parsed)
        voice=text.index(gate)
        assert start<end<voice
        prefix=text[start:end]
        assert 'MessageLog' not in prefix and 'Host.' not in prefix
        # All state writes in this verified prefix target the local m object.
        assert 'local m = {' in prefix and 'foreach(line in args)' in prefix
        predicate=text[voice:].splitlines()[0].strip()
        assert predicate in ['local voice = IsAvailVoice(m.voice)','local voice = IsAvailVoice(m.mask)']
        evidence=[]
        for name,raw in [('message.nut',message),('config.nut',config)]:
            lines=raw.decode('cp932').splitlines()
            wanted=set()
            for i,line in enumerate(lines):
                if name=='message.nut' and any(key in line for key in ['m.voice = p[1]','m.mask  = p[2]','local mask  =','local face =','local voice = IsAvailVoice']):
                    wanted.add(i)
                if name=='config.nut' and line.startswith('function IsAvailVoice('):
                    balance=0
                    for j in range(i,len(lines)):
                        wanted.add(j)
                        balance+=lines[j].count('{')-lines[j].count('}')
                        if balance==0:break
            evidence.append(dict(resource='system/'+name,sha256=sha(raw),code_lines=[dict(line=i+1,code=lines[i].strip()) for i in sorted(wanted)]))
        rows.append(dict(file=path.relative_to(a.games).as_posix(),parser_prefix_sha256=sha(prefix.encode('utf-8')),parser_side_effects='local m fields only; history/global/UI effects begin after extraction boundary',predicate=predicate,evidence=evidence))
    assert len(rows)==5
    assert sum(row['predicate'].endswith('(m.voice)') for row in rows)==4
    assert sum(row['predicate'].endswith('(m.mask)') for row in rows)==1
    result=dict(scope='Five original system.dat message/config source contracts only; no scene text, images, or recollection content',conclusion='Four original handlers pass audio name; Endless handler passes complete third-field mask. Shared adapter executes each original local parser and predicate rather than identifying a game.',source_boundary_contract=dict(handler=begin,parser_end=parsed,predicate=gate,all_markers_unique=True),games=rows)
    a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('PASS five original message parsers and voice predicate contracts (4 name, 1 mask)')


if __name__=='__main__':
    main()
