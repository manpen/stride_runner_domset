#!/bin/bash
set -e
set -x
TARGET=x86_64-unknown-linux-musl
PROFILE=smaller-release
rustup target add $TARGET
cargo build --profile $PROFILE --target=$TARGET
ls -ahl target/$TARGET/$PROFILE/runner