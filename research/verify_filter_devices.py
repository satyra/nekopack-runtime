"""Require real Android movie return, chapter dialogue, pixels and cleanup evidence."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
def load(name):return json.loads((Path('research')/name).read_text(encoding='utf8'))
desktop=load('filter-batch16-desktop-verification.json');layers=load('filter-batch16-layer-verification.json')
assert len(desktop['pages'])==len(layers['pages'])==55
assert layers['actor_filter_profile'] is True and sum('night' in p['actor_filters'] for p in layers['pages'])==23
rows=[];builds=set();actuals=[]
for mode,count,reason in [('natural',55,0),('skip',13,2)]:
 name=f'filter-batch16-arm64-{mode}-verification.json';r=load(name)
 assert r['execution']=='native ARM64' and len(r['games'])==1
 g=r['games'][0];actuals.append(g);assert g['index']==0 and len(g['pages'])==count
 assert g['closed_vm_live_bytes']==0 and len(g['movies'])==1
 for actual,expected in zip(g['pages'],desktop['pages']):
  assert all(actual[k]==expected[k] for k in ['sequence','source','line','png_sha256'])
 post=g['pages'][12];assert post['source']=='012401' and post['line']==11
 assert post['screen']['dialogue_visible'] and post['screen']['landscape']
 end=g['movies'][0]['events'][-1];assert end['error'] is None and end['reason']==reason
 if mode=='natural':
  assert g['pages'][27]['screen']['dialogue_visible'] and g['pages'][27]['screen']['landscape']
  assert g['boundary']['checkpoint']==desktop['boundary'] and g['boundary']['vm_live_bytes']==0
  assert end['frames']==2497 and end['samples']==4590720 and end['head']>=end['samples']
 else:assert g['boundary'] is None and 0<end['frames']<2497
 builds.add(r['build']['sha256']['nekopack-arm64-probe.apk'])
 rows.append(dict(report=name,sha256_lf=hashlib.sha256((Path('research')/name).read_text(encoding='utf8').encode()).hexdigest(),completion=mode,pages=count,post_movie_pages=count-12,first_chapter_dialogue=post,movie_frames=end['frames'],vm_live_bytes=0))
for key in ['source','line','text_sha256','png_sha256','metadata_sha256']:assert actuals[0]['pages'][12][key]==actuals[1]['pages'][12][key]
assert len(builds)==1 and next(iter(builds))==json.loads(Path('.local/android-probe/build.json').read_text())['sha256']['nekopack-arm64-probe.apk']
a.output.write_text(json.dumps(dict(scope='BLESS only, native ARM64; natural movie plus 55 pages, skip plus first chapter dialogue; no original screen parity or other-game claim',apk_sha256=next(iter(builds)),independent_pixel_pages=55,runs=rows),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS filter and post-movie dialogue on native ARM64')
