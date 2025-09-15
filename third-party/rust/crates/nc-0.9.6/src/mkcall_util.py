#!/usr/bin/env python3
# Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
# Use of this source is governed by Apache-2.0 License that can be found
# in the LICENSE file.


import os
import platform
import re
import subprocess
import sys


def check_system(expected_system):
    expected_lower = expected_system.lower()
    real_system = platform.system().lower()
    if real_system != expected_lower:
        print(f"system not match, expected `{expected_lower}`, real `{real_system}`")
        return False
    return True


def rust_fmt(filename):
    subprocess.run(["rustfmt", filename])


def escape_func_name(func_name):
    """Escape keyword in Rust."""
    keywords = (
        "break",
        "yield",
    )
    if func_name in keywords:
        return "r#" + func_name
    return func_name


def print_unimplemented_syscalls(sysnos):
    template = """
pub unsafe fn {0}() {{
    core::unimplemented!();
    // syscall0({1});
}}
"""
    for sysno in sorted(sysnos):
        func_name = sysno[4:].lower()
        func_name = escape_func_name(func_name)
        print(template.format(func_name, sysno), end="")


def read_sysnos(filepath):
    sysnos = []
    with open(filepath) as fh:
        sysno_pattern = re.compile("^pub const (SYS_\w+)")
        for line in fh:
            m = sysno_pattern.match(line)
            if m:
                sysnos.append(m.group(1))
    return sysnos


def parse_template(template_file):
    """Parse syscall template file.

    Returns syscall map of `{ sysno: function_body }` and template headers.
    """
    with open(template_file) as fh:
        new_func_name = ""
        new_func_start = False
        headers_end = False
        headers = []
        syscalls = {}
        sysno = ""
        func_name = ""
        stack = []
        syscall_pattern = re.compile("[^S]*(SYS_\w+)")
        func_name_pattern = re.compile("pub unsafe fn ([a-z0-9_#]+)")
        for line in fh:
            if line.startswith("pub unsafe fn") or line.startswith("///"):
                headers_end = True
                new_func_start = True
            if not headers_end:
                headers.append(line)
                continue

            if line:
                stack.append(line)

            if new_func_start and line == "}\n":
                for i, line in enumerate(stack):
                    if line.startswith("pub unsafe fn"):
                        m = func_name_pattern.match(line)
                        func_name = m.group(1)
                        # Remove raw identifier
                        if func_name.startswith("r#"):
                            func_name = func_name[2:]

                    elif "SYS_" in line:
                        m = syscall_pattern.match(line)
                        if m:
                            sysno = m.group(1)
                        else:
                            print("INVALID sysno:", line)
                            sys.exit(1)
                        if sysno[4:].lower() != func_name:
                            print("func name and sysno mismatch :%s:%s:" % (sysno, func_name))
                            print(line)
                            sys.exit(1)
                new_func_start = False
                if sysno not in syscalls:
                    syscalls[sysno] = list()
                syscalls[sysno].append(stack)
                stack = []
    return syscalls, headers


def check_call_file_exists(sysno, system_name, arch_name):
    CALLS_DIR = "calls"
    real_sysno = sysno.replace("SYS_", "").lower()

    # Like clone_linux_aarch64.rs
    arch_filename = "".join([real_sysno, "_", system_name, "_", arch_name,".rs"])
    arch_filepath = os.path.join(CALLS_DIR, arch_filename)
    if os.path.exists(arch_filepath):
        return (arch_filepath, True)

    # Like clone_linux.rs
    system_filename = "".join([real_sysno, "_", system_name, ".rs"])
    system_filepath = os.path.join(CALLS_DIR, system_filename)
    if os.path.exists(system_filepath):
        return (system_filepath, True)

    # Like clone.rs
    general_filename = "".join([real_sysno, ".rs"])
    general_filepath = os.path.join(CALLS_DIR, general_filename)
    if os.path.exists(general_filepath):
        return (general_filepath, True)

    return ("", False)


def generate_call_file(root_dir, system_name, arch_name):
    sysno_file = os.path.join(root_dir, "sysno.rs")
    call_file = os.path.join(root_dir, "call.rs")

    with open("call_header.rs") as fh:
        headers = fh.read()
    sysnos = read_sysnos(sysno_file)
    matched_sysno = []
    unmatched_sysno = []

    with open(call_file, "w") as fh:
        fh.writelines(headers)
        fh.write("\n")

        for sysno in sorted(sysnos):
            in_filepath, exists = check_call_file_exists(sysno, system_name, arch_name)
            if exists:
                matched_sysno.append(sysno)
                with open(in_filepath) as in_fh:
                    fh.write(in_fh.read())
                fh.write("\n")
            else:
                unmatched_sysno.append(sysno)
    rust_fmt(call_file)

    sysno_percentage = len(matched_sysno) * 100.0 / len(sysnos)
    if unmatched_sysno:
        print("-" * 80)
        print("root_dir:", root_dir, ", system_name:", system_name)
        unmatched_sysno_lower = [name.replace("SYS_", "").lower() for name in unmatched_sysno]
        print("unmatched sysnos:", unmatched_sysno_lower)
        print("Percentage of implemented syscalls: {:.2f}%".format(sysno_percentage))
        print("\n")
