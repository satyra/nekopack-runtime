"""Verify BIRTH KA names using original speaker metadata, without printing dialogue.

Example:
  python research/verify_birth_ka_original.py --birth-dir ORIGINAL_BIRTH_DIR \
      --cli target/x86_64-pc-windows-gnu/release/nekopack-cli.exe
"""
import argparse
import hashlib
import json
import struct
import subprocess
from pathlib import Path


FIRST_SHA256 = 'a4ec2d6cc1bf1abff0089d82574d8789e6ecdffae5b456e01906b5823f8cf21e'
EXPECTED_CUES = [f'KA_EP02_{number:04d}' for number in range(21)]


def resource(cli, archives, directory, name):
    lines = subprocess.check_output(
        [str(cli), 'lookup-name', directory.encode().hex(), name.encode().hex(),
         *map(str, archives)], timeout=45,
    ).decode().splitlines()
    selected = lines[0].split('\t')
    assert selected[0] == 'selected', (directory, name, lines)
    archive = archives[int(selected[1])]
    ordinal = int(selected[2])
    raw = subprocess.check_output(
        [str(cli), 'read-entry', str(archive), str(ordinal)], timeout=45,
    )
    return raw, dict(archive=archive.name, ordinal=ordinal, directory=directory,
                     name=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())


def ogg_metadata(raw):
    assert raw.startswith(b'OggS'), 'Expected original Ogg audio'
    identification = raw.index(b'\x01vorbis')
    channels = raw[identification + 11]
    rate = struct.unpack_from('<I', raw, identification + 12)[0]
    assert channels > 0 and rate > 0, 'Invalid Vorbis identification metadata'
    offset = 0
    samples = 0
    serials = set()
    pages = 0
    while offset < len(raw):
        assert raw[offset:offset + 4] == b'OggS', ('Invalid Ogg page', offset)
        assert raw[offset + 4] == 0, 'Unsupported Ogg page version'
        granule = struct.unpack_from('<Q', raw, offset + 6)[0]
        if granule != 0xffffffffffffffff:
            samples = granule
        serials.add(struct.unpack_from('<I', raw, offset + 14)[0])
        segments = raw[offset + 26]
        lacing = raw[offset + 27:offset + 27 + segments]
        assert len(lacing) == segments, 'Truncated Ogg lacing table'
        offset += 27 + segments + sum(lacing)
        assert offset <= len(raw), 'Truncated Ogg page payload'
        pages += 1
    assert len(serials) == 1 and samples > 0, 'Expected one nonempty original audio stream'
    return dict(codec='Vorbis', channels=channels, sample_rate=rate,
                total_samples=samples, duration_seconds=samples / rate, pages=pages)


def verify(birth_dir, cli):
    # These two original archives own every resource used here. Other .dat files
    # in an installed game directory can include the unrelated uninstaller data.
    archives = [birth_dir / 'system.dat', birth_dir / 'voice.dat']
    voice_map, map_meta = resource(cli, archives, 'system', 'voicemap.txt')
    mappings = []
    for line_number, line in enumerate(voice_map.splitlines(), 1):
        fields = line.split()
        if len(fields) == 2 and fields[1] == b'KA':
            mappings.append(dict(line=line_number, speaker_bytes_hex=fields[0].hex(), tag='KA'))
    assert len(mappings) == 1, ('Expected one original KA speaker mapping', mappings)
    speaker = mappings[0]['speaker_bytes_hex']

    script, script_meta = resource(cli, archives, 'script', 'EP02.txt')
    headers = []
    # Inspect only colon-prefixed speaker records; never decode or output dialogue lines.
    for line_number, line in enumerate(script.splitlines(), 1):
        if not line.startswith(b':'):
            continue
        fields = line.split(b':')
        if len(fields) > 2 and fields[2].upper().startswith(b'KA_'):
            headers.append(dict(line=line_number, speaker_bytes_hex=fields[1].hex(),
                                cue=fields[2].decode('ascii'),
                                face_tags_bytes_hex=fields[3].hex() if len(fields) > 3 else ''))
    assert [header['cue'] for header in headers] == EXPECTED_CUES, 'Unexpected original KA cue sequence'
    assert all(header['speaker_bytes_hex'] == speaker for header in headers), 'KA speaker mapping mismatch'
    assert all(bytes.fromhex(header['face_tags_bytes_hex']).split() == [b'KA']
               for header in headers), 'Original KA face-tag metadata mismatch'

    audio = []
    for cue in EXPECTED_CUES:
        raw, metadata = resource(cli, archives, 'voice', cue + '.ogg')
        metadata.update(cue=cue, **ogg_metadata(raw))
        audio.append(metadata)
    selected_entries = {(row['archive'], row['ordinal']) for row in audio}
    assert len(selected_entries) == len(EXPECTED_CUES), 'KA cue names unexpectedly alias the same archive entry'
    assert audio[0]['sha256'] == FIRST_SHA256, 'Original KA_EP02_0000 audio differs from verified evidence'
    return dict(
        scope='BIRTH original KA: original system voice map, EP02 speaker headers, exact resource lookup and Ogg metadata; no dialogue text or images output',
        original_game='BIRTH', borrowed=False,
        archives=[archive.name for archive in archives],
        source_script=script_meta, voice_map_resource=map_meta, voice_map_rows=mappings,
        speaker_headers=headers, voice_resources=audio, cue_count=len(audio),
        collision_checks=dict(
            names_taken_from_original_speaker_headers=True,
            all_speakers_match_original_KA_voice_map=True,
            all_21_cues_select_distinct_archive_entries=True,
            first_audio_sha256_matches_verified_resource=True,
            interpretation='Original script cross-references establish intended names; this is not a guessed 32-bit hash match. Arbitrary other strings can still share a 32-bit hash.'),
        previous_gap_reason='Earlier scene candidate scans omitted the nonnumeric EP02 scene name',
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--birth-dir', type=Path, required=True)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    report = verify(args.birth_dir, args.cli)
    serialized = json.dumps(report, indent=2) + '\n'
    if args.output:
        args.output.write_text(serialized, encoding='utf-8')
    print(serialized, end='')


if __name__ == '__main__':
    main()
