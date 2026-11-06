import unittest
from PIL import Image
from config_scene import unique_sprite
from compare_bless_config import metrics


class SceneTests(unittest.TestCase):
    def test_selected_and_hover_are_distinct_and_duplicates_fail(self):
        sprites = [(0,0,0,b'+ Screen#0'),(1,0,0,b'@ Screen#0')]
        self.assertEqual(unique_sprite(sprites,b'@ Screen#0'),1)
        with self.assertRaises(ValueError):
            unique_sprite(sprites,b'@ Screen#1')
        with self.assertRaises(ValueError):
            unique_sprite(sprites+sprites,b'@ Screen#0')

    def test_metrics_measure_edges_including_single_channel_errors(self):
        a = Image.new('RGB',(2,1),(10,20,30))
        b = a.copy()
        b.putpixel((1,0),(10,25,30))
        result = metrics(a,b)
        self.assertEqual(result['different_pixels'],1)
        self.assertEqual(result['max_channel_error'],5)
        self.assertAlmostEqual(result['mean_absolute_channel_error'],5/6)
