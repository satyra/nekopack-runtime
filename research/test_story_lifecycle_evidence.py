"""A millisecond timestamp tie must not hide a completion logged after release."""
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('lifecycle',Path(__file__).parents[1]/'platforms/android/verify_story_lifecycle.py')
lifecycle=importlib.util.module_from_spec(spec);spec.loader.exec_module(lifecycle)

class LifecycleEvidence(unittest.TestCase):
 def test_same_tick_late_ack_and_frame_are_rejected(self):
  for event in ['ack','frame']:
   self.assertFalse(lifecycle.no_late_visual_completion([dict(event='released',time_ms=100),dict(event=event,time_ms=100)]))
 def test_release_is_required_once_and_order_matters(self):
  self.assertFalse(lifecycle.no_late_visual_completion([]))
  self.assertFalse(lifecycle.no_late_visual_completion([dict(event='released'),dict(event='released')]))
  self.assertTrue(lifecycle.no_late_visual_completion([dict(event='frame',time_ms=100),dict(event='released',time_ms=100)]))

if __name__=='__main__':unittest.main()
