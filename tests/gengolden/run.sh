#!/bin/sh

# Usually run by gengolden

set -euo pipefail

if [ $# -ne 3 ]; then
	echo "Usage: run.sh <OUTPUT_FILENAME> <OUTPUT_SIZE> <MEM_SIZE>"
	exit 1
fi

trap "rm -f golden_test.n64 golden.raw" EXIT

bass golden_test.asm
chksum64 golden_test.n64 >/dev/null
64drive -q -c auto -u golden_test.n64

echo "Reset the N64 and press ENTER to continue..."
read -r

sleep 2
64drive -q -o 0x1000000 -s "$3" -d golden.raw
head -c "$2" <golden.raw >"$1"
