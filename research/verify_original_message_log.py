"""Pin BLESS message-log/STAR evidence; static scripts, PE and raw DAT only."""
import argparse,hashlib,json,struct,subprocess
from pathlib import Path
import pefile,capstone

def main():
    p=argparse.ArgumentParser()
    for key in ('games','scripts','cli','output'):p.add_argument('--'+key,type=Path,required=True)
    a=p.parse_args();exe=next(a.games.rglob('Tiny_BoD.exe'));raw=exe.read_bytes()
    digest=lambda b:hashlib.sha256(b).hexdigest()
    assert digest(raw)=='fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521'
    pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
    md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
    expected=[(0x4468cd,'push','0x550af0'),(0x4468d3,'mov','ebx, 0x5845e0'),(0x4468f7,'push','0x550ae7'),(0x4468fd,'call','0x4492a4'),(0x446918,'push','0x550af6'),(0x446924,'lea','esi, [ebx + 0x60]'),(0x446928,'call','0x4c0478'),(0x446932,'push','0x64'),(0x446935,'call','0x4c04e8'),(0x44693d,'push','0'),(0x446942,'call','0x4c0558'),(0x44694a,'push','0'),(0x44694e,'call','0x4c0024')]
    facts=[]
    for va,mn,op in expected:
        i=next(md.disasm(pe.get_data(va-base,16),va));assert (i.mnemonic,i.op_str)==(mn,op)
        facts.append(dict(va=hex(va),instruction=(mn+' '+op).strip()))
    for va,s in [(0x550af0,b'se_%s'),(0x550ae7,b'sound/se'),(0x550af6,b'VolSYS')]:assert pe.get_data(va-base,len(s)+1)==s+b'\0'
    checks={'message.nut':['function ViewLog()','for(local i=0; i<4; ++i)','local pagemax = MessageLog.len() - 4','if(m.voice)PlayVoice(m.voice)','if(!rest)MessageLog.append(m);','case ID_CANCEL:','g.Size = 22','g.LineWidth = text.width - 10','sound("PAGE")','sound("CLICK")','if(p != page)'],
            'saveload.nut':['::Save(f,MessageLog)','MessageLog = ::Load(f)','function QuickSave()','function QuickLoad(id=1)','function NormalSave(id)','function NormalLoad(id)'],
            'system.nut':['MessageLog <- []','case ID_LOG:  ViewLog(); return true']}
    def lookup_name(folder,name,archive):
        lines=subprocess.check_output([str(a.cli),'lookup-name',folder.encode().hex(),name.encode().hex(),str(archive)]).decode('utf8').splitlines()
        fields=lines[0].split('\t');assert fields[:2]==['selected','0'],lines
        return int(fields[2]),lines
    def entry(archive,index):return subprocess.check_output([str(a.cli),'read-entry',str(archive),str(index)])
    scripts={}
    for name,needles in checks.items():
        data=(a.scripts/name).read_bytes();text=data.decode('utf8').replace('\r\r\n','\n').replace('\r\n','\n');lines=text.splitlines()
        locations={n:[i for i,s in enumerate(lines,1) if n in s] for n in needles}
        assert all(locations.values()),(name,locations)
        if name=='message.nut':
            assert [n for n in locations['sound("PAGE")'] if 65<=n<=146]==[66,135]
            assert locations['sound("CLICK")']==[123,346]
            assert locations['if(p != page)']==[122]
        if name=='saveload.nut':
            assert text.count('sound("STAR")')==4
            locations['sound("STAR")']=[i for i,s in enumerate(lines,1) if 'sound("STAR")' in s]
        ordinal,_=lookup_name('system',name,exe.parent/'system.dat');source=entry(exe.parent/'system.dat',ordinal)
        assert source.decode('cp932').replace('\r','')==data.decode('utf8').replace('\r',''),name
        scripts[name]=dict(sha256=digest(data),raw_dat_sha256=digest(source),ordinal=ordinal,normalized_line_locations=locations)
    sound=exe.parent/'sound.dat'
    lookup=subprocess.check_output([str(a.cli),'lookup-name',b'sound/se'.hex(),b'se_STAR.ogg'.hex(),str(sound)]).decode('utf8').splitlines()
    assert lookup[0].split('\t')[:3]==['selected','0','79'],lookup
    audio=subprocess.check_output([str(a.cli),'read-entry',str(sound),'79'])
    assert audio.startswith(b'OggS');head=audio.index(b'\x01vorbis');rate=struct.unpack_from('<I',audio,head+12)[0];channels=audio[head+11]
    at=0;granule=0
    while at<len(audio):
        assert audio[at:at+4]==b'OggS';granule=struct.unpack_from('<Q',audio,at+6)[0];n=audio[at+26];at+=27+n+sum(audio[at+27:at+27+n])
    assert at==len(audio)
    log_sounds={}
    for name in ['se_PAGE','se_CLICK']:
        ordinal,_=lookup_name('sound/se',name+'.ogg',sound);data=entry(sound,ordinal)
        assert data.startswith(b'OggS')
        log_sounds[name]=dict(ordinal=ordinal,sha256=digest(data),bytes=len(data),category='VolSYS')
    log_ordinal,_=lookup_name('system','log.mng',exe.parent/'system.dat');sprites=[]
    for line in subprocess.check_output([str(a.cli),'inspect-sprites',str(exe.parent/'system.dat'),str(log_ordinal)]).decode().splitlines():
        n,x,y,name=line.split('\t');sprites.append(dict(ordinal=int(n),x=int(x),y=int(y),name=bytes.fromhex(name).decode('ascii')))
    assert len(sprites)==27 and not any('FACE' in s['name'] for s in sprites)
    result=dict(scope='BLESS static evidence; no Windows live timing or acoustic parity claim',exe_sha256=digest(raw),facts=facts,scripts=scripts,lookup=lookup,log_sounds=log_sounds,log=dict(ordinal=log_ordinal,sha256=digest(entry(exe.parent/'system.dat',log_ordinal)),sprites=sprites),
        star=dict(resource='sound/se/se_STAR.ogg',archive='sound.dat',ordinal=79,sha256=digest(audio),bytes=len(audio),sample_rate=rate,channels=channels,frames=granule,duration_seconds=granule/rate,category='VolSYS'),
        semantics=dict(log_rows=4,initial_page='max(0, count - 4)',voice='PlayVoice replaces the current voice channel; closing log does not explicitly stop voice',bgm='ViewLog contains no BGM stop/pause',save='MessageLog is saved/restored; InRestore skips append',star_trigger='QuickSave/NormalSave after InRoute check; QuickLoad/NormalLoad before file load'),
        limits=['Static target build evidence only','Runtime log font uses installed host font, not MS Gothic pixel parity','ID_BACK has an extra CLICK before ViewLog; that separate back-history input is not mapped by this batch'])
    a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print('PASS original message-log and STAR static evidence')
if __name__=='__main__':main()
