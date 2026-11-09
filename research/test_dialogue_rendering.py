"""Analytic inputs for the screenshot oracle; no game/font fixtures."""
import unittest
import numpy as np
from PIL import Image
from verify_dialogue_rendering import displayed_difference


class DisplayOracleTests(unittest.TestCase):
    def test_premultiplied_overlay_and_missing_layer(self):
        scene=Image.new('RGBA',(1280,720),(20,20,20,255))
        overlay=Image.new('RGBA',(1280,720),(200,80,40,128))
        layout={'bounds':[0,0,1280,720]}
        # Rounded premultiplied RGB = (100,40,20); background contributes 20*127/255.
        shot=Image.new('RGB',(1280,720),(110,50,30))
        diff,_=displayed_difference(scene,overlay,shot,(0,0,1280,720),layout)
        self.assertLess(diff.max(),.1)
        missing,_=displayed_difference(scene,overlay,scene.convert('RGB'),(0,0,1280,720),layout)
        self.assertGreater(missing.mean(),40)

    def test_fractional_origin_with_single_column_impulse(self):
        pixels=np.zeros((720,1280,4),dtype=np.uint8);pixels[:,:,3]=255
        pixels[:,400,:3]=255
        scene=Image.fromarray(pixels);overlay=Image.new('RGBA',(1280,720))
        # A single source column has a triangular bilinear reconstruction kernel.
        origin=507+1/3;scale=7/6
        source_x=(np.arange(2412)+.5-origin)/scale-.5
        line=np.round(255*np.maximum(0,1-np.abs(source_x-400))).astype(np.uint8)
        shot=Image.fromarray(np.broadcast_to(line[None,:,None],(1080,2412,3)).copy())
        diff,transform=displayed_difference(scene,overlay,shot,(96,96,2412,936),{'bounds':[0,0,1280,720]})
        self.assertAlmostEqual(transform['origin'][0],origin)
        self.assertLessEqual(diff.max(),.500001)
        shifted=Image.fromarray(np.roll(np.array(shot),2,axis=1))
        wrong,_=displayed_difference(scene,overlay,shifted,(96,96,2412,936),{'bounds':[0,0,1280,720]})
        self.assertGreater(wrong.max(),200)


if __name__=='__main__':unittest.main()
