from pathlib import Path
import sys,json,re,struct,argparse
parser=argparse.ArgumentParser(description="Prepare private, decoded title fixtures from owned games; never commit output.")
parser.add_argument("--games",type=Path,required=True)
parser.add_argument("--reference-root",type=Path,required=True)
parser.add_argument("--metadata",type=Path,default=Path("research/metadata-verification.json"))
parser.add_argument("--output",type=Path,required=True)
args=parser.parse_args()
sys.path.insert(0,str(args.reference_root));from portkit.neko import Archive
m=json.load(args.metadata.open(encoding='utf8'))
for game_id,g in enumerate(m['games']):
 root=args.output/str(game_id);(root/'script').mkdir(parents=True,exist_ok=True);(root/'system').mkdir(exist_ok=True)
 packs=[(s,Archive(args.games/s['file'])) for s in g['sources'] if any(x in s['file'] for x in ['system.dat','script.dat','patch'])]
 entries={};priorities={}
 for s,a in sorted(packs,key=lambda p:p[0]['priority']):
  for name in a.entries:
   if name.startswith(('script/','system/')):
    if priorities.get(name)==s['priority']:raise ValueError('ambiguous equal-priority title resource: '+name)
    entries[name]=(a,name);priorities[name]=s['priority']
 assets={};data={};ini={};ids=set()
 def literal(v):
  if isinstance(v,dict):return '{'+','.join('['+literal(k)+']='+literal(x) for k,x in v.items())+'}'
  if isinstance(v,list):return '['+','.join(map(literal,v))+']'
  return json.dumps(v,ensure_ascii=False)
 for name,(a,_) in entries.items():
  if name.endswith(('.nut','.txt')):
   content=a.read(name).decode('cp932'); (root/name).write_text(content,encoding='utf8');ids.update(re.findall(r'\bID_[A-Z0-9_]+',content))
   if name.startswith('system/') and name.endswith('.txt'):
    lines=[l.strip() for l in content.splitlines() if l.strip() and not l.lstrip().startswith(';')]
    data[Path(name).name]=[re.split('[ \t]+',l) for l in lines]
    ini[Path(name).name]={l.split('=',1)[0].strip():l.split('=',1)[1].strip() for l in lines if '=' in l}
  if name.startswith('system/') and name.endswith('.mng'):
   b=a.read(name);at=8;pos=[0,0];label='';bank={};ordinal=0
   while at<len(b):
    n=struct.unpack_from('>I',b,at)[0];kind=b[at+4:at+8];body=b[at+8:at+8+n]
    if kind==b'FRAM':label=body[1:].split(b'\0',1)[0].decode('cp932')
    if kind==b'DEFI':pos=list(struct.unpack_from('>2i',body,4))
    if kind==b'IHDR':
     w,h=struct.unpack_from('>2I',body);bank[label]=[name+':'+str(ordinal),*pos,w,h];ordinal+=1
    at+=n+12
   assets[Path(name).stem]=bank
 text='Assets <- '+literal(assets)+'\nData <- '+literal(data)+'\nIni <- '+literal(ini)+'\n'
 for i,key in enumerate(sorted(ids)):text+=key+' <- '+str({'ID_OK':1,'ID_CANCEL':2,'ID_USER':1000,'ID_FUNC':2000}.get(key,100+i))+'\n'
 (root/'assets.nut').write_text(text,encoding='utf8')
 print(game_id,len(assets),len(entries))
