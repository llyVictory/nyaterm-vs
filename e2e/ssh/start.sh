#!/bin/sh
set -eu
printf 'test:%s\n' "$(cat /run/secrets/ssh-password)" | chpasswd
ssh-keygen -A
printf 'fixture text\n' > /home/test/readme.txt
printf '\000\001\002\003' > /home/test/binary.bin
mkdir -p /home/test/folder
chown -R test:test /home/test
exec /usr/sbin/sshd -D -e
