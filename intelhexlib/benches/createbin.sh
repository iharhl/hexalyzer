#!/bin/bash

# This script generates a 1MB random binary file and converts it to hex.
# Call from the root of the repo.

BIN_FILENAME="random_data_1MB.bin"
HEX_FILENAME="random_data_1MB.hex"

# ================ ONE BIG FILE ================

echo "Generating 1 MB random bin file: $BIN_FILENAME..."
dd if=/dev/urandom of="build/$BIN_FILENAME" bs=1m count=1
echo "Done! File saved to: $(pwd)/build/$BIN_FILENAME"

echo "Converting $BIN_FILENAME to $HEX_FILENAME..."
objcopy -I binary -O ihex "build/$BIN_FILENAME" "build/$HEX_FILENAME"
echo "Done! File saved to: $(pwd)/build/$HEX_FILENAME"

# ================ SPARSE MEMORY FILE ================

BS=4096
MERGED_HEX_NAME="random_data_sparse.hex"

echo "Generating random bin files..."
# Generate first chunk
dd if=/dev/urandom of="build/part1.bin" bs=$BS count=16
# Generate second chunk
dd if=/dev/urandom of="build/part2.bin" bs=$BS count=2
# Generate third chunk
dd if=/dev/urandom of="build/part3.bin" bs=$BS count=24

echo "Converting parts to hex with offsets..."
# Convert part 1
objcopy -I binary -O ihex --change-addresses 0x0 "build/part1.bin" "build/part1.hex"
# Convert part 2 and move it to 0x16000
objcopy -I binary -O ihex --change-addresses 0x16000 "build/part2.bin" "build/part2.hex"
# Convert part 3 and move it to 0x4F000
objcopy -I binary -O ihex --change-addresses 0x4F000 "build/part3.bin" "build/part3.hex"

# Prepping files for merge (stripping start addr and EOF records)
sed '$d' "build/part1.hex" > "build/part1_stripped.hex"
sed '$d' "build/part2.hex" | sed '$d' > "build/part2_stripped.hex"
sed '$d' "build/part3.hex" | sed '$d' > "build/part3_stripped.hex"

echo "Merging into sparse hex file..."
cat "build/part1_stripped.hex" > "build/$MERGED_HEX_NAME"
cat "build/part2_stripped.hex" >> "build/$MERGED_HEX_NAME"
cat "build/part3_stripped.hex" >> "build/$MERGED_HEX_NAME"
echo ":00000001FF" >> "build/$MERGED_HEX_NAME"

# Cleanup
rm "build/part1.hex" "build/part2.hex" "build/part3.hex"
rm "build/part1_stripped.hex" "build/part2_stripped.hex" "build/part3_stripped.hex"
rm "build/part1.bin" "build/part2.bin" "build/part3.bin"