"""Pin BLESS native line-break branches and compare real phone layouts to a separate oracle."""
import argparse, hashlib, json, math, sys
from pathlib import Path
import capstone, pefile
from fontTools.ttLib import TTFont

HANG='、。，．'
OPEN='（〔［｛〈《「『【‘“'
CLOSE='）〕］｝〉》」』】’”ゝゞヽヾ々？！：；♪'
PAIR='…‥―'

def reference_layout(text, advances, width=1005):
    """Build line groups, then apply native positive-gap distribution to closed rows."""
    rows=[];current=[];used=0;breaks={}
    for index,ch in enumerate(text):
        if ch=='\n':
            breaks[index]=[used,len(rows)*30]
            rows.append((current,False));current=[];used=0;continue
        if ch not in HANG and used+advances[index]>width:
            carry=used<=width and current and (text[current[-1]] in OPEN or ch in CLOSE or (ch in PAIR and ch==text[current[-1]]))
            kept=current[-1:] if carry else []
            rows.append((current[:-1] if carry else current,True))
            current=kept;used=sum(advances[i] for i in current)
        current.append(index);used+=advances[index]
    rows.append((current,False));origins=[[0,0] for _ in text]
    for index,origin in breaks.items():origins[index]=origin
    for row,(indices,justify) in enumerate(rows):
        gap=max(0,width-sum(advances[i] for i in indices));x=0
        for column,index in enumerate(indices):
            extra=gap*column//(len(indices)-1) if justify and len(indices)>1 else 0
            origins[index]=[x+extra,row*30];x+=advances[index]
    return origins

def main():
    p=argparse.ArgumentParser()
    for key in ('games','portkit','font','reference','device-report','output'):p.add_argument('--'+key,required=True)
    a=p.parse_args();sha=lambda b:hashlib.sha256(b).hexdigest()
    exe=next(Path(a.games).rglob('Tiny_BoD.exe')).read_bytes()
    pinned=json.loads(Path('research/original-filter-batch16-verification.json').read_text())
    assert sha(exe)==pinned['exe_sha256']
    pe=pefile.PE(data=exe);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
    classes=[(0x55ca1d,HANG),(0x55ca26,OPEN),(0x55ca3d,CLOSE),(0x55ca68,PAIR)]
    for va,value in classes:assert pe.get_data(va-base,len(value.encode('cp932'))+1)==value.encode('cp932')+b'\0'
    expected=[
        (0x4a2430,'push','0x55ca1d'),(0x4a24c8,'push','0x55ca26'),
        (0x4a2560,'push','0x55ca3d'),(0x4a25f8,'push','0x55ca68'),
        (0x4a33c5,'push','0'),(0x4a33c7,'push','0'),(0x4a33ca,'call','0x4a3f08'),
        (0x4a3424,'call','0x4a2410'),(0x4a342c,'jne','0x4a34bf'),
        (0x4a343b,'cmp','edx, ecx'),(0x4a343d,'jge','0x4a344e'),
        (0x4a343f,'push','0'),(0x4a3441,'push','1'),(0x4a3444,'call','0x4a3f08'),
        (0x4a3454,'add','eax, edx'),(0x4a345e,'jle','0x4a34bf'),
        (0x4a3461,'call','0x4a24a8'),(0x4a346e,'call','0x4a2540'),(0x4a347b,'call','0x4a25d8'),
        (0x4a3485,'cmp','esi, edi'),(0x4a34b3,'push','eax'),(0x4a34b4,'push','1'),(0x4a34b7,'call','0x4a3f08'),
        (0x4a3693,'add','dword ptr [ebx + 0x70], ecx'),(0x4a3696,'mov','esi, edi'),
        (0x4a3f6d,'mov','al, byte ptr [ebp + 0x10]'),(0x4a3f74,'dec','esi'),
        (0x4a40e4,'sub','ecx, edx'),(0x4a40ee,'jle','0x4a419b'),
        (0x4a4184,'imul','edi'),(0x4a4188,'div','dword ptr [ebp - 0x10]'),(0x4a418b,'add','ecx, eax'),
        (0x4a32ec,'mov','dword ptr [eax + 0x70], edx'),(0x4a32f2,'add','dword ptr [eax + 0x74], ecx'),
        (0x4a4616,'mov','dword ptr [eax + 0x20], edx'),(0x4a4622,'add','dword ptr [ebx + 0x70], eax')]
    facts=[]
    for va,mnemonic,operands in expected:
        ins=next(md.disasm(pe.get_data(va-base,16),va));assert (ins.mnemonic,ins.op_str)==(mnemonic,operands),(hex(va),ins.mnemonic,ins.op_str)
        facts.append(dict(va=hex(va),instruction=mnemonic+' '+operands))
    sys.path.insert(0,a.portkit)
    from portkit.neko import Archive
    manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
    packs=[(s['priority'],Archive(Path(a.games)/s['file'])) for s in manifest['sources'] if any(name in s['file'] for name in ('system.dat','script.dat','patch'))]
    def script(name):
        key='script/'+name.lower()+'.txt';choices=[(priority,archive) for priority,archive in packs if key in archive.entries]
        priority=max(p for p,_ in choices);selected=[arc for p,arc in choices if p==priority];assert len(selected)==1
        return selected[0].read(key)
    report=json.loads(Path(a.device_report).read_text(encoding='utf8'));font_bytes=Path(a.font).read_bytes()
    assert report['execution']=='native ARM64' and report['font_sha256']==sha(font_bytes)
    font=TTFont(a.font,fontNumber=0);cmap=font.getBestCmap();metrics=font['hmtx'].metrics;units=font['head'].unitsPerEm
    checked=[]
    for page in report['pages']:
        n=page['sequence'];source=json.loads((Path(a.reference)/f'{n-1}.json').read_text(encoding='utf8'));text=source['text']
        raw=script(source['source']);assert raw.split(b'\n')[source['line']-1].strip(b' \t\r').decode('cp932')==text
        advances=[int(math.floor(metrics[cmap[ord(c)]][0]*22/units+.5)) for c in text]
        expected_origins=reference_layout(text,advances)
        actual=page['layout']['origins'];assert actual==expected_origins,(n,actual,expected_origins)
        simple=[];x=y=0
        for advance in advances:
            if x+advance>1005:x=0;y+=30
            simple.append([x,y]);x+=advance
        changed=[i for i,(old,new) in enumerate(zip(simple,actual)) if old!=new]
        checked.append(dict(sequence=n,source=source['source'],line=source['line'],script_sha256=sha(raw),scalar_count=len(text),changed_origins=changed,lines=page['layout']['lines'],clipped=page['layout']['clipped']))
        if n in (23,63):
            stop=next(i for i,c in enumerate(text) if c=='。' and simple[i][1]!=actual[i][1])
            assert actual[stop][0]>=990
        if n in (156,217):assert any(simple[i][1]!=actual[i][1] for i in range(len(text)))
        if n==81:assert [v[1] for v in simple]==[v[1] for v in actual], 'small kana are not in the native carry table'
    assert {23,63,81,156,217}.issubset({p['sequence'] for p in checked})
    Path(a.output).write_text(json.dumps(dict(scope='BLESS native static line-break evidence plus independent phone-font layout oracle',execution=report['execution'],apk_sha256=report['apk_sha256'],exe_sha256=sha(exe),font_sha256=sha(font_bytes),classes=[dict(va=hex(va),characters=value) for va,value in classes],facts=facts,pages=checked,limits=['Not a live original-GDI pixel comparison','Font advances use the explicitly selected Noto substitute','One-character carry, not a recursive general Japanese kinsoku algorithm']),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('PASS native punctuation classes, wrap/carry/justify branches, DAT text and phone layout origins')

if __name__=='__main__':main()
