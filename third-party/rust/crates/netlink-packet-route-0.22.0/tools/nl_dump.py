#!/usr/bin/env python3

# SPDX-License-Identifier: MIT
# Simple tool to dump rtnetlink messages (or part of messages)

from ctypes import c_ubyte
from ctypes import c_uint
from ctypes import c_ushort
from ctypes import Structure
from ctypes import sizeof
from enum import Enum
import socket
import struct
import argparse


def roundup2(val: int, num: int) -> int:
    if val % num:
        return (val | (num - 1)) + 1
    else:
        return val


def align4(val: int) -> int:
    return roundup2(val, 4)


def print_struct(struct):
    kv = []
    for pair in struct._fields_:
        kv.append("{}: {}".format(pair[0], getattr(struct, pair[0])))
    return "{ " + ", ".join(kv) + " }"


def print_bytes(descr, data):
    print("===vv {} (len:{:3d}) vv===".format(descr, len(data)))
    off = 0
    step = 16
    while off < len(data):
        for i in range(step):
            if off + i < len(data):
                print("{}0x{:02X},".format(" " * int(bool(i)), data[off + i]), end="")
        print("")
        off += step
    print("--------------------")


class Nlmsghdr(Structure):
    _fields_ = [
        ("nlmsg_len", c_uint),
        ("nlmsg_type", c_ushort),
        ("nlmsg_flags", c_ushort),
        ("nlmsg_seq", c_uint),
        ("nlmsg_pid", c_uint),
    ]

    def __str__(self):
        return print_struct(self)


class Nlattr(Structure):
    _fields_ = [
        ("nla_len", c_ushort),
        ("nla_type", c_ushort),
    ]


class NlAttr(object):
    def __init__(self, nla_type, data):
        if isinstance(nla_type, Enum):
            self._nla_type = nla_type.value
            self._enum = nla_type
        else:
            self._nla_type = nla_type
            self._enum = None
        self.nla_list = []
        self._data = data

    def parse_nla_list(self, attr_map):
        ret = []
        off = 0
        data = self._data
        while len(data) - off >= 4:
            nla_len, raw_nla_type = struct.unpack("@HH", data[off : off + 4])
            if nla_len + off > len(data):
                raise ValueError(
                    "attr length {} > than the remaining length {}".format(
                        nla_len, len(data) - off
                    )
                )
            nla_type = raw_nla_type & 0x3FFF
            if nla_type in attr_map:
                v = attr_map[nla_type]
                val = v["ad"].cls.from_bytes(data[off : off + nla_len], v["ad"].val)
                if "child" in v:
                    # nested
                    child_data = data[off + 4 : off + nla_len]
                    if v.get("is_array", False):
                        # Array of nested attributes
                        val = self.parse_child_array(
                            child_data, v["ad"].val, v["child"]
                        )
                    else:
                        val = self.parse_child(child_data, v["ad"].val, v["child"])
            else:
                # unknown attribute
                val = NlAttr(raw_nla_type, data[off + 4 : off + nla_len])
            ret.append(val)
            off += align4(nla_len)
        self.nla_list = ret
        assert off == len(self._data)
        return self

    def get_attr(self, nla_type):
        return [a for a in self.nla_list if a._nla_type == nla_type][0]

    def __bytes__(self):
        ret = self._data
        if align4(len(ret)) != len(ret):
            ret = self._data + bytes(align4(len(ret)) - len(ret))
        return struct.pack("@HH", len(self._data) + 4, self._nla_type) + ret

    def __repr__(self):
        return (
            "{ "
            + "nla_type: {}, nla_len: {}".format(self._nla_type, len(self._data))
            + " }"
        )


class BaseNetlinkMessage(object):
    nl_attr_map = {}

    def __init__(self, nlmsg_type):
        pass

    def _parse_hdr(self, data):
        return data, len(data)

    def parse(self):
        data_hdr, hdr_off = self._parse_hdr(self._orig_data[16:])
        self.data_hdr = data_hdr
        if len(self._orig_data) > 16 + hdr_off:
            fake_nla = NlAttr(0, self._orig_data[16 + hdr_off :])
            fake_nla.parse_nla_list(self.nl_attr_map)
            self.root = fake_nla
            # assert 16 + hdr_off + off == len(self._orig_data)

    def parse_attrs(self, data: bytes, attr_map):
        ret = []
        off = 0
        while len(data) - off >= 4:
            nla_len, raw_nla_type = struct.unpack("@HH", data[off : off + 4])
            if nla_len + off > len(data):
                raise ValueError(
                    "attr length {} > than the remaining length {}".format(
                        nla_len, len(data) - off
                    )
                )
            nla_type = raw_nla_type & 0x3FFF
            if nla_type in attr_map:
                v = attr_map[nla_type]
                val = v["ad"].cls.from_bytes(data[off : off + nla_len], v["ad"].val)
                if "child" in v:
                    # nested
                    child_data = data[off + 4 : off + nla_len]
                    if v.get("is_array", False):
                        # Array of nested attributes
                        val = self.parse_child_array(
                            child_data, v["ad"].val, v["child"]
                        )
                    else:
                        val = self.parse_child(child_data, v["ad"].val, v["child"])
            else:
                # unknown attribute
                val = NlAttr(raw_nla_type, data[off + 4 : off + nla_len])
            ret.append(val)
            off += align4(nla_len)
        return ret, off

    @classmethod
    def from_bytes(cls, data):
        nl_hdr = Nlmsghdr.from_buffer_copy(data[:16])
        self = cls(nl_hdr.nlmsg_type)
        self._orig_data = data
        self.nl_hdr = nl_hdr
        return self

    def print_message(self):
        print("==")
        print(self.nl_hdr)
        print(self.data_hdr)
        # print(self.attrs)

    def print_as_bytes(self, descr: str, skip_nlhdr=True):
        print_bytes(descr, self._orig_data[16:])


class Tcmsg(Structure):
    _fields_ = [
        ("tcm_family", c_ubyte),
        ("tcm__pad1", c_ubyte),
        ("tcm__pad2", c_ushort),
        ("tcm_ifindex", c_uint),
        ("tcm_handle", c_uint),
        ("tcm_parent", c_uint),
        ("tcm_info", c_uint),
    ]

    def __str__(self):
        return print_struct(self)


class TcNetlinkMessage(BaseNetlinkMessage):
    nl_attr_map = {}

    def _parse_hdr(self, data):
        if len(data) < sizeof(Tcmsg):
            raise ValueError("message too short for Tcmsg")
        data_hdr = Tcmsg.from_buffer_copy(data)
        return data_hdr, sizeof(Tcmsg)


class Actmsg(Structure):
    _fields_ = [
        ("tca_family", c_ubyte),
        ("tca__pad1", c_ubyte),
        ("tca__pad2", c_ushort),
    ]

    def __str__(self):
        return print_struct(self)


class ActNetlinkMessage(BaseNetlinkMessage):
    nl_attr_map = {}

    def _parse_hdr(self, data):
        if len(data) < sizeof(Actmsg):
            raise ValueError("message too short for Tcmsg")
        data_hdr = Actmsg.from_buffer_copy(data)
        return data_hdr, sizeof(Actmsg)


class Rtnlsock:
    AF_NETLINK = 16
    NETLINK_ROUTE = 0

    msg_map = {
        44: TcNetlinkMessage,  # RTM_NEWTFILTER
        45: TcNetlinkMessage,  # RTM_DELTFILTER
        46: TcNetlinkMessage,  # RTM_GETTFILTER
        48: ActNetlinkMessage,  # RTM_NEWACTION
        49: ActNetlinkMessage,  # RTM_DELACTION
        50: ActNetlinkMessage,  # RTM_GETACTION
    }

    def __init__(self):
        s = socket.socket(self.AF_NETLINK, socket.SOCK_RAW, self.NETLINK_ROUTE)
        s.setsockopt(270, 10, 1)  # NETLINK_CAP_ACK
        s.setsockopt(270, 11, 1)  # NETLINK_EXT_ACK
        self.sock_fd = s
        self._data = bytes()

    def write_data(self, data: bytes):
        self.sock_fd.sendmsg([data])

    def read_data(self):
        while True:
            self._data += self.sock_fd.recv(65535)
            if len(self._data) >= sizeof(Nlmsghdr):
                break

    def read_message(self):
        if len(self._data) < sizeof(Nlmsghdr):
            self.read_data()
        hdr = Nlmsghdr.from_buffer_copy(self._data)
        while hdr.nlmsg_len > len(self._data):
            self.read_data()
        raw_msg = self._data[: hdr.nlmsg_len]
        self._data = self._data[hdr.nlmsg_len :]
        cls = self.msg_map.get(hdr.nlmsg_type, BaseNetlinkMessage)
        return cls.from_bytes(raw_msg)

    def read_dump(self):
        ret = []
        while True:
            msg = self.read_message()
            if msg.nl_hdr.nlmsg_type == 3:
                break
            ret.append(msg)
        return ret


# Netlink attribute definitions
TCA_ACT_TAB = 1
TCA_ACT_KIND = 1


def dump_tc_rules(ifname: str, is_flower=False, chain=None):
    ifindex = socket.if_nametoindex(ifname)
    nl = Rtnlsock()

    parent = int(is_flower) * 4294967282
    data = bytes(Tcmsg(tcm_ifindex=ifindex, tcm_parent=parent))
    if chain is not None:
        chain_nla = Nlattr(nla_len=8, nla_type=11)
        data = data + bytes(chain_nla) + struct.pack("@I", chain)

    nl_len = len(data) + sizeof(Nlmsghdr)
    nl_hdr = Nlmsghdr(nlmsg_len=nl_len, nlmsg_type=46, nlmsg_flags=0x305, nlmsg_seq=1)
    nl.write_data(bytes(nl_hdr) + data)
    while True:
        msg = nl.read_message()
        if msg.nl_hdr.nlmsg_type == 3:
            # end of dump
            break
        if len(msg._orig_data) <= 64:
            # skip all header-only / chain-only TC message nonsense
            continue
        msg.parse()
        msg.print_message()
        msg.print_as_bytes("test")


def dump_tc_actions(act_type: str):
    nl = Rtnlsock()

    data = bytes(Actmsg())

    data += bytes(
        NlAttr(
            TCA_ACT_TAB,
            bytes(
                NlAttr(
                    1, bytes(NlAttr(TCA_ACT_KIND, act_type.encode() + b"\x00"))  # idx 1
                )
            ),
        )
    )
    data += bytes(
        NlAttr(2, b"\x01\x00\x00\x00\x01\x00\x00\x00")
    )  # TCA_ROOT_FLAGS bitfield32 - TCA_ACT_FLAG_LARGE_DUMP_ON

    nl_len = len(data) + sizeof(Nlmsghdr)
    nl_hdr = Nlmsghdr(nlmsg_len=nl_len, nlmsg_type=50, nlmsg_flags=0x305, nlmsg_seq=1)
    nl.write_data(bytes(nl_hdr) + data)
    msg = nl.read_dump()[0]
    msg.parse()
    msg.print_message()
    for a in msg.root.get_attr(1).parse_nla_list({}).nla_list:
        print(a)
        action_type = a.parse_nla_list({}).get_attr(1)._data.decode("utf-8")
        s = "order: {} type: {}".format(a._nla_type, action_type)
        print_bytes(s, bytes(a))


def main():
    p = argparse.ArgumentParser(prog="nl_dump")
    subparsers = p.add_subparsers(dest="cmd", required=True, help="sub-command help")
    p_actions = subparsers.add_parser(
        "dump_actions", help="dump actions of a certain type"
    )
    p_actions.add_argument(
        "-a", "--action", type=str, required=True, help="action type"
    )
    p_flower = subparsers.add_parser("dump_flower", help="dump tc-flower(8) rules")
    p_flower.add_argument("-i", "--interface", type=str, required=True, help="ifname")
    p_flower.add_argument("-c", "--chain", type=int, help="chain#")
    p_u32 = subparsers.add_parser("dump_u32", help="dump tc-u32(8) rules")
    p_u32.add_argument("-i", "--interface", type=str, required=True, help="interface")

    args = p.parse_args()

    if args.cmd == "dump_actions":
        dump_tc_actions(args.action)
    elif args.cmd == "dump_flower":
        dump_tc_rules(args.interface, True, args.chain)
    elif args.cmd == "dump_u32":
        dump_tc_rules(args.interface, False, None)


if __name__ == "__main__":
    main()
