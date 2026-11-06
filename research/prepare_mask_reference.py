"""Extract private mask oracles with the independent DAT reader; commit hashes only."""
import argparse,hashlib,json,sys
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 for name in ['games','reference-root','output-dir','report']:p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
 manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'));rows=[]
 for i,g in enumerate(manifest['games']):
  packs=[(s['priority'],Archive(a.games/s['file'])) for s in g['sources'] if any(n in s['file'] for n in ['image.dat','patch'])]
  names=sorted({n for _,pack in packs for n in pack.entries if n.startswith('image/mask/') and n.endswith('.png')})
  for name in names:
   choices=[(priority,pack) for priority,pack in packs if name in pack.entries];top=max(priority for priority,_ in choices)
   selected=[pack for priority,pack in choices if priority==top];assert len(selected)==1
   raw=selected[0].read(name);folder=a.output_dir/str(i)/Path(name).stem.upper();folder.mkdir(parents=True,exist_ok=True)
   (folder/'mask.png').write_bytes(raw);rows.append(dict(game=i,name=name,sha256=hashlib.sha256(raw).hexdigest()))
 a.report.write_text(json.dumps(dict(scope='Independent original DAT masks; local assets excluded from git',records=rows),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
