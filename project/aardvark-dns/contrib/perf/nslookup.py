#!/usr/bin/env python

import socket
import sys

for i in range(0, 10_000):
    socket.getaddrinfo(sys.argv[1], 0)
