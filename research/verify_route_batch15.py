"""BLESS-only raw DAT route evidence and bounded movie-to-chapter regression."""
import argparse,hashlib,json,subprocess,sys
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['cli','semantics','games','reference-root','fixtures','pages','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
entry=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
files=[a.games/s['file'] for s in entry['sources']]
packs=[(s['priority'],Archive(a.games/s['file'])) for s in entry['sources'] if any(n in s['file'] for n in ['script.dat','system.dat','patch'])]
def read(name):
 matches=[(priority,pack) for priority,pack in packs if name in pack.entries]
 highest=max(priority for priority,_ in matches);chosen=[pack for priority,pack in matches if priority==highest]
 assert len(chosen)==1
 return chosen[0].read(name)
sources=[]
for name in ['system/system.nut','system/saveload.nut','script/story1.txt','script/012401.txt']:
 raw=read(name);fixture=a.fixtures/'0'/name
 assert raw.decode('cp932')==fixture.read_bytes().decode('utf8').replace('\r\r\n','\r\n')
 sources.append(dict(name=name,raw_sha256=hashlib.sha256(raw).hexdigest(),utf8_fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest()))
r=subprocess.run([str(a.semantics.resolve()),str(a.fixtures/'0/system/system.nut'),str(a.fixtures/'0/system/saveload.nut')],capture_output=True,text=True,encoding='utf8')
assert r.returncode==0 and r.stdout.strip()=='PASS original route, title, reset, save gating, metadata order, load restore and unwind',(r.stdout,r.stderr)
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
run=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'30',*map(str,files)],capture_output=True)
assert run.returncode==1
boundary=json.loads((root/'boundary.json').read_text(encoding='utf8'))
assert (root/'boundary.txt').read_text(encoding='utf8')=='story 012401:6 depth=2 steps=205: unsupported story preview command filter'
assert [boundary[k] for k in ['source','line','depth','steps']]==['012401',6,2,205]
assert boundary['labels']==dict(title='２４日目：水の曜日',route='第１章',caption='Tiny Dungeon ～BLESS of DRAGON～ 第１章 ２４日目：水の曜日',in_route=True)
assert 'movie op.mpg' in boundary['effects']
assert len(list(root.glob('[0-9]*.json')))==12
pages=[]
for n in range(12):
 page=json.loads((root/f'{n}.json').read_text(encoding='utf8'))
 old=json.loads((Path('.local/batch14-story-verified/0')/f'{n}.json').read_text(encoding='utf8'))
 labels=page.pop('labels');assert page==old
 assert labels['route']=='　' and labels['title']=='プロローグ～選定者の扉' and labels['in_route'] is True
 assert (root/f'{n}.png').read_bytes()==(Path('.local/batch14-story-verified/0')/f'{n}.png').read_bytes()
 pages.append(dict(sequence=n+1,source=page['source'],line=page['line'],labels=labels))
a.output.write_text(json.dumps(dict(scope='BLESS only; original system scripts executed with recording file/renderer/native-save test doubles; metadata semantics, not original save bytes or complete runtime save/load',sources=sources,semantics_result=r.stdout.strip(),cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),semantics_executable_sha256=hashlib.sha256(a.semantics.read_bytes()).hexdigest(),pages=pages,boundary=boundary),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS route semantics and movie-to-012401 call')
