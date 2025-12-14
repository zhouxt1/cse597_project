#!/bin/bash
# Find where the rustc libraries live
SYSROOT=$(rustc --print sysroot)
# macOS uses DYLD_LIBRARY_PATH, Linux uses LD_LIBRARY_PATH
export DYLD_LIBRARY_PATH=$SYSROOT/lib:$DYLD_LIBRARY_PATH
export LD_LIBRARY_PATH=$SYSROOT/lib:$LD_LIBRARY_PATH

# Run your tool, passing the target file as an argument
cargo run --bin static_analysis -- tests/dummy.rs --sysroot $SYSROOT