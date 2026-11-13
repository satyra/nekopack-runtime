"""Pin original EnumFiles order using PE metadata/instructions only.

Does not execute the EXE, read scenario resources, or inspect images.
"""
import argparse
import hashlib
from pathlib import Path

import capstone
import pefile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    args = parser.parse_args()
    raw = args.exe.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == "9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f"
    pe = pefile.PE(data=raw)
    base = pe.OPTIONAL_HEADER.ImageBase
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    checks = [
        (0x51EE13, "push", "0x57e40c"),
        (0x51EE18, "push", "0x75d4c9"),
        (0x57F2A6, "call", "0x4e5c74"),
        # WIN32_FIND_DATAA at ebp-0x354: ftLastWriteTime offset 0x14.
        (0x4E5D8C, "lea", "ecx, [ebp - 0x354]"),
        (0x4E5DCA, "call", "0x7416a2"),
        (0x4E5EE9, "test", "byte ptr [ebp - 0x354], 0x10"),
        (0x4E5F05, "mov", "ecx, dword ptr [ebp - 0x340]"),
        (0x4E5F11, "mov", "ecx, dword ptr [ebp - 0x33c]"),
        (0x4E66F5, "call", "0x4e6c8c"),
        # Comparison key of new value vs existing node; newer traverses left.
        (0x4E6CAB, "mov", "ebx, edi"),
        (0x4E6CAD, "add", "ebx, 0x10"),
        (0x4E6CB3, "mov", "eax, dword ptr [ebp + 0x10]"),
        (0x4E6CBC, "push", "edx"),
        (0x4E6CC0, "push", "ecx"),
        (0x4E6CC1, "call", "0x74164e"),
        (0x4E6CC8, "setg", "al"),
        (0x4E6CD4, "mov", "edx, dword ptr [edi]"),
        # Begin at tree's leftmost entry, then append each basename in order.
        (0x4E67B5, "mov", "eax, dword ptr [ebp - 8]"),
        (0x4E67B8, "mov", "eax, dword ptr [eax]"),
        (0x4E6978, "call", "0x4e6e04"),
        (0x57F31A, "call", "0x69ae84"),
        (0x57F329, "call", "0x69b534"),
        (0x74164E, "jmp", "dword ptr [0x7d023c]"),
        (0x7416A2, "jmp", "dword ptr [0x7d0274]"),
    ]
    assert pe.get_data(0x75D4C9 - base, 10) == b"EnumFiles\0"
    for address, mnemonic, operands in checks:
        ins = next(md.disasm(pe.get_data(address - base, 15), address))
        assert (ins.mnemonic, ins.op_str) == (mnemonic, operands), hex(address)
    imports = {item.address: item.name for dll in pe.DIRECTORY_ENTRY_IMPORT for item in dll.imports}
    assert imports[0x7D023C] == b"CompareFileTime"
    assert imports[0x7D0274] == b"FindFirstFileA"
    print(f"PASS {len(checks)} pinned instructions: EnumFiles sorts file last-write time descending; basenames returned")


if __name__ == "__main__":
    main()
