#!/usr/bin/env python3
# Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
# Use of this source is governed by Apache-2.0 License that can be found
# in the LICENSE file.

import os
import re
import sys


def main():
    if len(sys.argv) != 2:
        print("Usage: %s input-file" % sys.argv[0])
        sys.exit(1)

    macro_pattern = re.compile(r"#define\s+(\w+)\s+([a-zA-Z0-9_\-]+)\s*(.*)")
    consts_pattern = re.compile(r"\s*([A-Z_0-9]+)\s*=\s*([0-9\-\.xXa-fA-F]+)(.*)")
    comments_pattern = re.compile(r"(:?,)\s*/\*(.*)\*/")
    comments2_pattern = re.compile(r"^/\*(.*)\*/$")
    comments3_pattern = re.compile(r"\s*/\*(.*)\*/")
    with open(sys.argv[1]) as fh:
        for line in fh:
            m = macro_pattern.match(line)
            if m:
                m2 = comments3_pattern.match(m.group(3))
                if m2:
                    print("/// {}".format(m2.groups()[-1].strip()))
                    print("pub const {}: i32 = {};".format(m.group(1), m.group(2)))
                else:
                    print("pub const {}: i32 = {};{}".format(m.group(1), m.group(2), m.group(3)))
                continue

            m = consts_pattern.match(line)
            if m:
                m2 = comments3_pattern.match(m.group(3))
                if m2:
                    print("/// {}".format(m2.groups()[-1].strip()))
                    print("pub const {}: i32 = {};".format(m.group(1), m.group(2)))
                else:
                    print("pub const {}: i32 = {};{}".format(m.group(1), m.group(2), m.group(3)))
                continue

            m = comments2_pattern.match(line)
            if m:
                print("/// {}".format(m.groups()[-1].strip()))
                continue

            print(line, end="")

if __name__ == "__main__":
    main()
