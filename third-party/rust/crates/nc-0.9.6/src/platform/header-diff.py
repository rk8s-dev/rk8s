#!/usr/bin/env python3
# Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
# Use of this source is governed by Apache-2.0 License that can be found
# in the LICENSE file.

import os
import sys


def list_nc_dir(dir_name):
    result = []

    for filename in os.listdir(dir_name):
        if filename.endswith('.rs') and filename != "mod.rs" and filename != "lib.rs":
            name, ext = os.path.splitext(filename)
            name = name.replace("uapi_", "")
            result.append(name)
    return result

def list_linux_dir(dir_name):
    result = []
    for root, dirs, files in os.walk(dir_name):
        for filename in files:
            if filename.endswith('.h'):
                result.append(os.path.splitext(filename)[0])
    return result

def diff_folder(arch_name, nc_dir, linux_src_dir):
    rs_files = list_nc_dir(nc_dir)
    sorted(rs_files)
    nc_arch_dir = os.path.join(nc_dir, arch_name)
    rs_arch_files = list_nc_dir(nc_arch_dir)

    arch_names_map = {
        "x86": "x86",
        "x86_64": "x86",
        "arm": "arm",
        "aarch64": "arm64",
        "mips": "mips",
        "mipsel": "mips",
        "mips64": "mips",
        "mips64el": "mips",
        "powerpc64": "powerpc",
        "s390x": "s390",
    }
    linux_arch_name = arch_names_map[arch_name]

    linux_arch_dir = os.path.join(linux_src_dir, "arch", linux_arch_name, "include")
    linux_files = list_linux_dir(linux_arch_dir)
    for rs_file in rs_files:
        if rs_file in linux_files and rs_file not in rs_arch_files:
            print("NEED:", rs_file)


def main():
    if len(sys.argv) != 3:
        print("Usage: %s arch linux-src-dir" % sys.argv[0])
        sys.exit(1)

    nc_dir = os.path.join(os.path.dirname(sys.argv[0]), "linux-types")
    arch_name = sys.argv[1]
    linux_src_dir = sys.argv[2]
    diff_folder(arch_name, nc_dir, linux_src_dir)

if __name__ == "__main__":
    main()
