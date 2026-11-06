"""Build independent hashes of actual audio requested by reference story pages."""
import argparse,json,hashlib,sys
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 for name in ['games','reference-root','pages','output']:p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
 manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'));games=[]
 for i,g in enumerate(manifest['games']):
  packs=[(s['priority'],Archive(a.games/s['file'])) for s in g['sources'] if any(x in s['file'] for x in ['sound.dat','voice','patch'])]
  wanted=set()
  for file in (a.pages/str(i)).glob('*.json'):
   page=json.loads(file.read_text(encoding='utf8'))
   if page['voice']:wanted.add(('voice',page['voice']))
   for effect in page['effects']:
    kind,_,name=effect.partition(' ')
    if kind in ['bgm','se'] and not name.startswith('fade '):wanted.add((kind,name))
  rows={}
  for kind,name in sorted(wanted):
   logical=({'bgm':'sound/bgm','se':'sound/se','voice':'voice'}[kind]+'/'+name+'.ogg').lower()
   selected=[(priority,pack) for priority,pack in packs if logical in pack.entries]
   highest=max(priority for priority,_ in selected);best=[pack for priority,pack in selected if priority==highest]
   if len(best)!=1:raise RuntimeError('ambiguous reference audio')
   raw=best[0].read(logical)
   if not raw.startswith(b'OggS'):raise RuntimeError('not Ogg audio')
   rows[kind+'/'+name.lower()]=hashlib.sha256(raw).hexdigest()
  games.append(rows)
 a.output.write_text(json.dumps(dict(scope='independent reference DAT decoder hashes for audio in validated story pages and boundary checkpoint',games=games),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
