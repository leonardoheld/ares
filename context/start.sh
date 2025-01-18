#!/bin/bash
/usr/sbin/sshd
gdbserver --once :2159 /usr/local/bin/hello
