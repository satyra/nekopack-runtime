"""Verify original BRAVE background/image load failure control flow from its PE.

Only reads PE bytes and instructions; never opens scenario text or image assets.
"""
import argparse
import hashlib
import json
from pathlib import Path

import capstone
import pefile

from verify_original_missing_mask import SHA256


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    raw = args.exe.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == SHA256, "unexpected original PE"
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase
    assert pe.FILE_HEADER.Machine == 0x14C and base == 0x400000
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    checks = []

    def instruction(address, mnemonic, operands):
        ins = next(md.disasm(pe.get_data(address - base, 15), address))
        assert (ins.mnemonic, ins.op_str) == (mnemonic, operands), hex(address)
        checks.append(dict(va=hex(address), mnemonic=mnemonic, operands=operands))

    assert pe.get_data(0x6456B5 - base, 11) == b"image/back\0"
    # Background folder initialization and the generic layer loading path.
    instruction(0x45EEC2, "push", "0x6456b5")
    instruction(0x46D734, "lea", "eax, [ebx + 0x30]")
    instruction(0x46D738, "call", "0x4c14f8")
    instruction(0x46D740, "test", "al, al")
    instruction(0x46D742, "jne", "0x46d776")
    instruction(0x46D769, "push", "0x64574b")
    instruction(0x46D76E, "call", "0x58065c")
    instruction(0x46D773, "add", "esp, 0xc")
    instruction(0x46D776, "mov", "al, byte ptr [ebx + 0x2c]")

    # Only successful loads decode/copy the replacement into the target image.
    instruction(0x4C4248, "call", "0x4b1084")
    instruction(0x4C4250, "test", "al, al")
    instruction(0x4C4252, "je", "0x4c42f5")
    instruction(0x4C42B4, "call", "0x598ee8")
    instruction(0x4C42C3, "push", "dword ptr [ebp + 8]")
    instruction(0x4C42C6, "call", "0x4bec34")
    instruction(0x4C42E4, "call", "0x597ef0")
    instruction(0x4C42F5, "dec", "dword ptr [ebp - 0x1a8]")
    instruction(0x4BEC3E, "mov", "ebx, dword ptr [ebp + 8]")
    instruction(0x4BECB0, "push", "ebx")
    instruction(0x4BECB1, "call", "0x5982d8")

    # Unlike the fatal error helper at 0x580694, this logger simply returns.
    instruction(0x580678, "call", "0x5f24b4")
    instruction(0x580687, "call", "0x580470")
    instruction(0x580690, "ret", "")
    instruction(0x580479, "call", "0x63996e")
    instruction(0x580483, "call", "0x6399a4")
    instruction(0x5804F4, "ret", "")
    imports = {i.address: i.name.decode() if i.name else None
               for dll in pe.DIRECTORY_ENTRY_IMPORT for i in dll.imports}
    imported_helpers = {}
    for address, name in [(0x63996E, "IsDebuggerPresent"),
                          (0x6399A4, "OutputDebugStringA")]:
        ins = next(md.disasm(pe.get_data(address - base, 6), address))
        assert ins.mnemonic == "jmp" and ins.op_str.startswith("dword ptr [0x")
        iat = int(ins.op_str.removeprefix("dword ptr [").removesuffix("]"), 16)
        assert imports[iat] == name
        imported_helpers[hex(address)] = name

    result = dict(
        scope="read-only PE imports/disassembly; no scenario/image access",
        executable=args.exe.name, sha256=SHA256,
        conclusion="Missing background/image load skips replacement and logs an error, retaining the previous image; no filename-prefix repair occurs in this failure path.",
        evidence_kind="static control-flow evidence, not a live rendering observation",
        instruction_checks=checks, imported_helpers=imported_helpers,
    )
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"PASS original missing-background load/log/retain path ({len(checks)} instructions)")


if __name__ == "__main__":
    main()
