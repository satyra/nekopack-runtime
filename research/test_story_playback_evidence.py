"""Check the distinction between startup, short-clip completion and progress."""
import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('story_verify',Path(__file__).resolve().parents[1]/'platforms/android/verify_story.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class AudioEvidence(unittest.TestCase):
 def test_replacement_requires_same_channel_started_before_duration_and_no_late_events(self):
  start=dict(event='playing',kind='se',token=1,value=1098,time_ms=1000)
  request=dict(event='begin',kind='se',token=2,value=0,time_ms=1265)
  playing=dict(request,event='playing',value=733,time_ms=1375)
  self.assertEqual(module.replacement_audio_token([start,request,playing],1),2)
  invalid=[[start,request],[start,dict(request,kind='voice'),dict(playing,kind='voice')],[start,dict(request,time_ms=2098),dict(playing,time_ms=2100)],[start,request,playing,dict(start,event='progress',time_ms=1400)], [start,request,dict(playing,value=0)], [start,dict(start,event='completed',time_ms=1200),request,playing]]
  for events in invalid:self.assertIsNone(module.replacement_audio_token(events,1))
  self.assertIsNone(module.replacement_audio_token([request,start,playing],1))
 def test_stop_requires_release_before_ack_even_at_same_timestamp(self):
  begin=dict(event='begin',kind='bgm_stop',token=2,value=0,time_ms=100)
  stopped=dict(begin,event='stopped');ack=dict(begin,event='ack')
  module.verify_bgm_stop([begin,stopped,ack],begin,ack)
  for events in [[begin,ack],[begin,ack,stopped],[begin,stopped,stopped,ack],[begin,dict(stopped,value=1),ack]]:
   with self.assertRaises(AssertionError):module.verify_bgm_stop(events,begin,ack)
 def test_stopped_bgm_cannot_report_late_playback_but_other_channels_can(self):
  playing=dict(event='playing',kind='bgm',token=1,value=1000)
  begin=dict(event='begin',kind='bgm_stop',token=2,value=0)
  stopped=dict(begin,event='stopped');ack=dict(begin,event='ack')
  events=[playing,begin,stopped,ack]
  module.verify_bgm_stop(events+[dict(event='progress',kind='voice',token=3,value=100)],begin,ack)
  for event in ['progress','completed']:
   with self.assertRaises(AssertionError):module.verify_bgm_stop(events+[dict(playing,event=event)],begin,ack)
 def test_environment_stop_rejects_late_loop_but_preserves_bgm_progress(self):
  playing=dict(event='playing',kind='env',token=1,value=3994)
  begin=dict(event='begin',kind='env_stop',token=2,value=0)
  stopped=dict(begin,event='stopped');ack=dict(begin,event='ack')
  events=[playing,begin,stopped,ack,dict(event='progress',kind='bgm',token=3,value=100)]
  module.verify_channel_stop(events,begin,ack,'env')
  for event in ['progress','completed','looped']:
   with self.assertRaises(AssertionError):module.verify_channel_stop(events+[dict(playing,event=event)],begin,ack,'env')
 def test_zero_position_completion_requires_elapsed_duration_and_matching_token(self):
  start=dict(event='playing',token=6,value=324,time_ms=1000)
  self.assertFalse(module.completed_audio([start],6))
  for token,time in [(6,1323),(7,1391)]:
   self.assertFalse(module.completed_audio([start,dict(event='completed',token=token,value=0,time_ms=time)],6))
  self.assertTrue(module.completed_audio([start,dict(event='completed',token=6,value=0,time_ms=1391)],6))
  self.assertFalse(module.completed_audio([dict(start,value=0),dict(event='completed',token=6,value=0,time_ms=1391)],6))
if __name__=='__main__':unittest.main()
