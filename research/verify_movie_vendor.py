"""Verify that the pinned MIT header differs only by the documented local fixes."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--upstream',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
meta_path=Path('vendor/pl_mpeg/UPSTREAM.json');meta=json.loads(meta_path.read_text())
raw=a.upstream.read_bytes();assert hashlib.sha256(raw).hexdigest()==meta['sha256']
s=raw.decode().replace('\r\n','\n')
s=s.replace('SPDX-License-Identifier: MIT','SPDX-License-Identifier: MIT\n\nNekoPack modified version: see NEKOPACK_CHANGES.md (EOF and MP2 corrections).',1)
s=s.replace('''plm_buffer_has_ended(self->buffer) && (
						self->picture_type == PLM_VIDEO_PICTURE_TYPE_INTRA ||
						self->picture_type == PLM_VIDEO_PICTURE_TYPE_PREDICTIVE
					)''','plm_buffer_has_ended(self->buffer)')
s=s.replace('// If we reached the end of the file and the previously decoded\n\t\t\t\t// frame was a reference frame, we still have to return it.','// NekoPack: flush the pending reference even after a final B picture.\n\t\t\t\t// That picture was displayed, but frame_backward is still pending.')
s=s.replace('int scale = 65536 / (adj + 1);','// NekoPack: normalize by the level count after multiplication,\n\t// retaining precision instead of truncating a shared scale coefficient.\n\tint levels = adj;')
for i in range(3):s=s.replace(f'val = (adj - sample[{i}]) * scale;',f'val = ((adj - sample[{i}]) * 65536) / levels;')
s=s.replace('// Output samples','// NekoPack: unit-gain Q30 synthesis; PCM adapter performs saturation.\n\t\t\t\t\t// Output samples').replace('-1090519040.0f','-1073741824.0f')
actual=Path('vendor/pl_mpeg/pl_mpeg.h').read_text(encoding='utf8');assert actual==s,'undocumented upstream changes'
assert hashlib.sha256(actual.encode()).hexdigest()==meta['local_sha256']
a.output.write_text(json.dumps(dict(upstream_revision=meta['revision'],upstream_sha256=meta['sha256'],local_sha256_lf=meta['local_sha256'],only_documented_changes=True,license='MIT',algorithm_source_copied_from_ffmpeg=False),indent=2)+'\n')
