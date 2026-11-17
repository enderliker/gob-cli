#!/usr/bin/env python3
"""Regenerate consts/*.bin from Jonathan Basile's published algorithm source.

Upstream: https://github.com/librarianofbabel/libraryofbabel.info-algo
          (CC BY-SA-NC, by the author of babelia.libraryofbabel.info)

Usage: git clone <upstream> /tmp/lobalgo && python3 tools/gen_consts.py /tmp/lobalgo
"""
import re, sys, pathlib

sys.set_int_max_str_digits(4000000)

src = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/lobalgo")
out = pathlib.Path(__file__).resolve().parent.parent / "consts"
out.mkdir(exist_ok=True)

def grab(path, name):
    text = pathlib.Path(path).read_text(errors="replace")
    return int(re.search(r'mpz_int ' + name + r'\("(\d+)"\)', text).group(1))

want = {
    "m": ("PRNG", "m"),
    "a": ("PRNG", "a"),
    "c": ("PRNG", "c"),
    "ainverse": ("imagesearch.cpp", "ainverse"),
    "maskone": ("PRNG", "maskone"),
    "masktwo": ("PRNG", "masktwo"),
    "divver": ("PRNG", "divver"),
}
for key, (fname, cname) in want.items():
    v = grab(src / fname, cname)
    if key in ("maskone", "masktwo", "divver"):
        assert v & (v - 1) == 0, f"{key} is not a power of two"
        (out / f"{key}.txt").write_text(str(v.bit_length() - 1) + "\n")  # exponent k, value = 2**k
        print(f"{key}: 2^{v.bit_length()-1}")
    else:
        n = (v.bit_length() + 7) // 8
        (out / f"{key}.bin").write_bytes(v.to_bytes(n, "big"))
        print(f"{key}: {v.bit_length()} bits -> {n} bytes")
