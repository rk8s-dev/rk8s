#!/usr/bin/env python3

import os
import pprint
import sys

from mkcall_util import (parse_template, rust_fmt)


def main():
    template_file = "linux_call.rs"
    syscalls, header = parse_template(template_file)

    for (name, call) in syscalls.items():
        real_name = name.replace("SYS_", "").lower()
        print("Handle", real_name)
        filename = real_name + ".rs"
        out_file = os.path.join("calls", filename)
        if os.path.exists(out_file):
            print("File exists error:", out_file, ", call name:", name)
            break

        calls = call[0]
        is_unimpl = False
        for line in calls:
            if "unimplemented" in line:
                is_unimpl = True
        if is_unimpl:
            print("Ignore", name)
            continue

        with open(out_file, "w") as fh:
            fh.writelines(calls)
        rust_fmt(out_file)


if __name__ == "__main__":
    main()
