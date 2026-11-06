"""Cross-check staged story regression against final movie/FFI/control evidence."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
def read(name):return json.loads((Path('research')/name).read_text(encoding='utf8'))
def digest(path):return hashlib.sha256(path.read_text(encoding='utf-8-sig').encode()).hexdigest()
reference=read('movie-batch14-independent-counts.json')['games'];rows=[];final_builds=set();inputs=[]
for label in ['arm64','translated']:
 names={
  'core':f'story-batch14-{label}-verification.json',
  'smoke':f'story-batch14-{label}-final-smoke-verification.json',
  'chain':f'story-batch14-{label}-movie-chain-verification.json',
  'skip':f'story-batch14-{label}-movie-skip-verification.json',
  'movies':f'movie-batch14-{label}-verification.json',
  'controls':f'movie-batch14-{label}-controls.json',
  'cancel':f'movie-batch14-{label}-story-cancel.json',
  'ffi':f'story-batch14-{label}-ffi-verification.json',
  'snapshots':f'story-batch14-{label}-snapshot-verification.json'}
 data={k:read(v) for k,v in names.items()}
 for k,name in names.items():inputs.append(dict(file=name,sha256_lf=digest(Path('research')/name)))
 for k in ['chain','skip','movies','controls','cancel','ffi','smoke']:final_builds.add(data[k]['build']['sha256']['nekopack-arm64-probe.apk'])
 assert [g['index'] for g in data['smoke']['games']]==[1,2]
 assert all(len(g['pages'])==2 and g['closed_vm_live_bytes']==0 for g in data['smoke']['games'])
 core=data['core']['games'];assert [len(g['pages']) for g in core]==[12,120,120,49]
 assert len(data['snapshots']['snapshots'])==85
 assert [g['index'] for g in data['chain']['games']]==[0,3]
 for game in data['chain']['games']:
  old=core[game['index']];assert old['pages']==game['pages'] and old['boundary']==game['boundary']
  assert len(game['movies'])==1 and game['closed_vm_live_bytes']==0
  movie=game['movies'][0];boundary=game['boundary']
  assert movie['metadata']['host_effect']=='movie' and movie['metadata']['name']=='op.mpg'
  assert boundary['checkpoint']['source']=='story1' and boundary['checkpoint']['line']==12 and boundary['vm_live_bytes']==0
  assert movie['events'][-1]['reason']==0 and movie['events'][-1]['error'] is None
  # Final decoder-only changes must not alter earlier independent static scene captures.
  index=game['index'];oldroot=Path(f'.local/batch14-{label}-release-effects')/str(index);newroot=Path(f'.local/batch14-{label}-checked-effects')/str(index)
  expected={str(e['token']) for e in game['host_events'] if e['event']=='begin' and e['kind'] in ['fade','wipe','shake','flash']}
  assert {f.stem for f in newroot.glob('*.png')}==expected
  for token in expected:assert (oldroot/f'{token}.png').read_bytes()==(newroot/f'{token}.png').read_bytes()
 skip=data['skip']['games'];assert len(skip)==1 and skip[0]['index']==0
 assert skip[0]['boundary']==data['chain']['games'][0]['boundary']
 end=skip[0]['movies'][0]['events'][-1];assert end['reason']==2 and end['error'] is None and 0<end['frames']<reference[0]['frames']
 assert {g['game'] for g in data['movies']['games']}=={0,1,2,3}
 for game,ref in zip(data['movies']['games'],reference):
  assert game['game']==ref['game'];end=game['closed']
  assert end['reason']==0 and end['error'] is None and end['frames']==ref['frames'] and end['samples']==ref['samples_per_channel']
  assert game['info']['source_sha256']==ref['source_sha256'] and game['display_captured']
  assert game['desktop_pcm_error']['max_error']<=4 and game['desktop_pcm_error']['rms_error']<=2
 controls=data['controls']['cases'];assert len(controls)==3
 assert [g['events'][-1]['reason'] for g in controls]==[2,1,3]
 assert all(g['events'][-1]['error'] is None and not g['late_events'] for g in controls)
 cancel=data['cancel'];assert cancel['vm_live_bytes']==0 and cancel['movie_events'][-1]['reason']==3 and cancel['movie_events'][-1]['error'] is None
 assert cancel['pending']==data['chain']['games'][0]['movies'][0]['metadata']
 assert len(data['ffi']['games'])==4 and all(g['passed'] for g in data['ffi']['games'])
 rows.append(dict(device=label,execution=data['movies']['execution'],core_regression_pages=301,core_regression_apk=data['core']['build']['sha256']['nekopack-arm64-probe.apk'],final_movie_chain_pages=61,final_birth_brave_smoke_pages=4,final_original_movies=4,final_skip_chain_pages=12,final_background_did_not_resume=True,independent_static_snapshots=85,final_scene_captures_equal_independently_checked_core=True))
assert len(final_builds)==1
build=json.loads(Path('.local/android-probe/build.json').read_text());assert next(iter(final_builds))==build['sha256']['nekopack-arm64-probe.apk']
a.output.write_text(json.dumps(dict(scope='301-page staged core regression followed by final decoder-specific movie, story return, input and destruction checks; not a claim that all 301 pages used the final APK',final_apk_sha256=next(iter(final_builds)),devices=rows,inputs=inputs),indent=2)+'\n')
print('PASS final cross-report evidence')
