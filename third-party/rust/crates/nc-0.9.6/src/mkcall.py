#!/usr/bin/env python3
# Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
# Use of this source is governed by Apache-2.0 License that can be found
# in the LICENSE file.

import os
import platform
import sys

from mkcall_util import (check_system, generate_call_file)


SUPPORTED_ARCHES = {
    "linux": [
        "aarch64",
        "arm",
        "loongarch64",
        "mips",
        "mips64",
        "ppc64",
        "riscv64",
        "s390x",
        "x86",
        "x86_64",
    ],
    "freebsd": [
        "x86_64",
    ],
    "netbsd": [
        "x86_64",
    ],
    "darwin": [
        "x86_64",
    ],
}

SYSTEM_NAMES = (
    "linux",
    "freebsd",
    "netbsd",
    "darwin",
)


def generate_call_file_helper(system_name, arch_name):
    root_dir = F"platform/{system_name}-{arch_name}"
    if arch_name not in SUPPORTED_ARCHES[system_name]:
        print(F"arch name {arch_name} is not supported on system: {system_name}")
        sys.exit(1)
    generate_call_file(root_dir, system_name, arch_name)


def main():
    system_name = ""
    arch_name = ""
    if len(sys.argv) == 1:
        system_name = platform.system().lower()
    elif len(sys.argv) == 2:
        system_name = sys.argv[1]
    elif len(sys.argv) == 3:
        system_name = sys.argv[1]
        arch_name = sys.argv[2]
        if arch_name == "all":
            arch_name = ""

    if not system_name:
        print("Usage: %s system-name arch-name" % sys.argv[0])
        print("system-name might be:", SYSTEM_NAMES)
        sys.exit(1)

    if not check_system(system_name):
        sys.exit(1)
    if not arch_name:
        for arch_name in SUPPORTED_ARCHES[system_name]:
            generate_call_file_helper(system_name, arch_name)
    else:
        generate_call_file_helper(system_name, arch_name)


if __name__ == "__main__":
    main()
