#!/usr/bin/env bash
# Check Android NDK environment
set -e

echo "Checking Android NDK environment..."

# Check if ANDROID_NDK_HOME is set
if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "Error: ANDROID_NDK_HOME is not set"
    echo "Please set it to your Android NDK path, for example:"
    echo "export ANDROID_NDK_HOME=/Users/username/Library/Android/sdk/ndk/25.1.8937393"
    exit 1
fi

# Check if NDK directory exists
if [ ! -d "$ANDROID_NDK_HOME" ]; then
    echo "Error: Android NDK directory does not exist: $ANDROID_NDK_HOME"
    exit 1
fi

# Check if clang exists
CLANG_PATH="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android24-clang"
if [ ! -f "$CLANG_PATH" ]; then
    echo "Error: Android clang compiler not found at: $CLANG_PATH"
    echo "Please check your NDK installation"
    exit 1
fi

# Set ANDROID_API if not set
if [ -z "$ANDROID_API" ]; then
    echo "ANDROID_API not set, defaulting to 24"
    export ANDROID_API=24
fi

# Try simple compile test
echo "Testing compiler..."
echo "#include <stdio.h>" > test.c
echo "int main() { printf(\"Hello\"); return 0; }" >> test.c

"$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android$ANDROID_API-clang" test.c -o test

if [ $? -ne 0 ]; then
    echo "Error: Failed to compile a simple test program"
    exit 1
else
    echo "Compiler test successful!"
    rm test.c test
fi

# Check for OpenSSL development files
echo "Looking for OpenSSL development files..."
if [ -d "$ANDROID_NDK_HOME/sysroot/usr/include/openssl" ]; then
    echo "OpenSSL headers found in NDK sysroot"
else
    echo "Warning: OpenSSL headers not found in NDK sysroot"
    echo "You may need to provide OpenSSL headers and libraries"
fi

echo "Environment check completed successfully"
