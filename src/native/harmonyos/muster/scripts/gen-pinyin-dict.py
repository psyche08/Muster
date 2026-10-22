#!/usr/bin/env python3
"""Converts AOSP PinyinIME's rawdict_utf16_65105_freq.txt (Apache-2.0) into
the keyboard's dictionary resource.

    gen-pinyin-dict.py rawdict_utf16_65105_freq.txt > entry/src/main/resources/rawfile/pinyin.txt

Source: https://android.googlesource.com/platform/packages/inputmethods/PinyinIME/
        +/refs/heads/main/jni/data/rawdict_utf16_65105_freq.txt
Kept: GB2312 entries (flag 0). Each line: syllables joined by "'", a tab, the
word, a tab, the frequency rounded to an integer (at least 1).
"""
import sys

raw = open(sys.argv[1], 'rb').read()
text = raw.decode('utf-16')
print("# Derived from AOSP PinyinIME rawdict_utf16_65105_freq.txt, Copyright (c) 2009 The Android")
print("# Open Source Project, Apache License 2.0 (docs/third-party/aosp-pinyinime-NOTICE.txt).")
print("# Changed: UTF-8, GB2312 entries only, integer frequencies, one tab-separated entry per line.")
rows = []
for line in text.splitlines():
    parts = line.lstrip('﻿').split()
    if len(parts) < 4 or parts[2] != '0':
        continue
    word, freq, syllables = parts[0], float(parts[1]), parts[3:]
    if len(syllables) != len(word):
        continue
    rows.append(("'".join(syllables), word, max(1, round(freq))))
rows.sort(key=lambda r: (r[0], -r[2]))
for key, word, freq in rows:
    print(f"{key}\t{word}\t{freq}")
