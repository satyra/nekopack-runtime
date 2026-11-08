"""Replay current audio evidence rules against completed device reports."""
import argparse,hashlib,importlib.util,json
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--reports',type=Path,nargs='+',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 verifier=Path(__file__).resolve().parents[1]/'platforms/android/verify_story.py'
 spec=importlib.util.spec_from_file_location('story_verify',verifier);v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
 results=[]
 for path in a.reports:
  d=json.loads(path.read_text(encoding='utf8'));games=[]
  for game in d['games']:
   host=game['host_events'];begins=[e for e in host if e['event']=='begin' and e['kind'] in ['bgm','env','se','voice']]
   stop_count=sum(effect=='bgm_stop' for page in game['pages']+([game['boundary']['checkpoint']] if game['boundary'] else []) for effect in page['effects'])
   assert sum(e['event']=='begin' and e['kind']=='bgm_stop' for e in host)==stop_count
   assert sum(e['event']=='begin' and e['kind']=='env_stop' for e in host)==sum(effect=='env_stop' for page in game['pages']+([game['boundary']['checkpoint']] if game['boundary'] else []) for effect in page['effects'])
   assert sum(e['event']=='begin' and e['kind']=='se_stop' for e in host)==sum(effect=='se_stop' for page in game['pages']+([game['boundary']['checkpoint']] if game['boundary'] else []) for effect in page['effects'])
   assert len(begins)==len(game['audio']);audio=[];stops=[]
   for begin,metadata in zip(begins,game['audio']):
    token=begin['token'];assert begin['kind']==metadata['kind']
    playing=[e for e in host if e['event']=='playing' and e['token']==token];assert len(playing)==1 and playing[0]['value']>0
    progress=any(e['event'] in ['progress','completed'] and e['token']==token and e['value']>0 for e in host)
    completed=v.completed_audio(host,token);replacement=v.replacement_audio_token(host,token)
    assert progress==metadata['progress_observed'] and completed==metadata['completed_after_duration']
    terminal=metadata['ended_in_terminal_span']
    assert progress or completed or replacement is not None or terminal
    audio.append(dict(token=token,kind=begin['kind'],progress_observed=progress,completed_after_duration=completed,replaced_by_token=replacement,terminal_span_only=terminal and not progress and not completed and replacement is None))
   for begin in [e for e in host if e['event']=='begin' and e['kind'] in ['bgm_stop','env_stop','se_stop']]:
    acks=[e for e in host if e['event']=='ack' and e['token']==begin['token']];assert len(acks)==1
    v.verify_channel_stop(host,begin,acks[0],begin['kind'].removesuffix('_stop'));stops.append(dict(token=begin['token'],ack_elapsed_ms=acks[0]['value'],release_before_ack=True,channel=begin['kind'].removesuffix('_stop'),no_late_released_channel_events=True))
   games.append(dict(index=game['index'],audio=audio,stops=stops))
  results.append(dict(report=path.name,report_sha256_lf_utf8=hashlib.sha256(path.read_text(encoding='utf8').encode('utf8')).hexdigest(),execution=d['execution'],apk_sha256=d['build']['sha256']['nekopack-arm64-probe.apk'],games=games))
  print('PASS audio evidence',path.name,flush=True)
 a.output.write_text(json.dumps(dict(scope='Replay of captured device events; replacement is interruption evidence, not proof of audible progress; terminal spans retain their original bounded-observation limitation',verifier_sha256_lf_utf8=hashlib.sha256(verifier.read_text(encoding='utf8').encode('utf8')).hexdigest(),reports=results),indent=2)+'\n')
if __name__=='__main__':main()
