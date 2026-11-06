"""Check the distinction between startup, short-clip completion and progress."""
import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('story_verify',Path(__file__).resolve().parents[1]/'platforms/android/verify_story.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class AudioEvidence(unittest.TestCase):
 def test_zero_position_completion_requires_elapsed_duration_and_matching_token(self):
  start=dict(event='playing',token=6,value=324,time_ms=1000)
  self.assertFalse(module.completed_audio([start],6))
  for token,time in [(6,1323),(7,1391)]:
   self.assertFalse(module.completed_audio([start,dict(event='completed',token=token,value=0,time_ms=time)],6))
  self.assertTrue(module.completed_audio([start,dict(event='completed',token=6,value=0,time_ms=1391)],6))
  self.assertFalse(module.completed_audio([dict(start,value=0),dict(event='completed',token=6,value=0,time_ms=1391)],6))
if __name__=='__main__':unittest.main()
