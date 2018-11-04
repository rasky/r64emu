#!/bin/sh
set -euo pipefail

if [ $# -ne 1 ]; then
	echo "Usage: run.sh <NUM_BYTES>"
	exit 1
fi

rm -f golden.raw magic.raw

bass rsp_stress_test.asm
chksum64 rsp_stress_test.n64 >/dev/null
64drive -q -c auto -u rsp_stress_test.n64

echo "Reset the N64 and press ENTER to continue..."
read -r

sleep 2
64drive -q -o 0x1000000 -s 1024 -d golden.raw

echo "Input:"
xxd vectors.bin | head -n 10

echo "Golden:"
xxd golden.raw | head -n 10
