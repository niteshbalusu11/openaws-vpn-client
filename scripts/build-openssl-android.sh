#!/usr/bin/env bash
# Build OpenSSL for Android
set -e

if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "Error: ANDROID_NDK_HOME environment variable not set"
    exit 1
fi

OPENSSL_VERSION="1.1.1q"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$ROOT_DIR/build-mobile/openssl"
OUTPUT_DIR="$ROOT_DIR/mobile/libs/openssl"

# Create build directories
mkdir -p "$BUILD_DIR"
mkdir -p "$OUTPUT_DIR"

# Download OpenSSL if needed
if [ ! -d "$BUILD_DIR/openssl-$OPENSSL_VERSION" ]; then
    echo "Downloading OpenSSL $OPENSSL_VERSION..."
    cd "$BUILD_DIR"
    curl -L -o "openssl-$OPENSSL_VERSION.tar.gz" "https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz"
    tar -xf "openssl-$OPENSSL_VERSION.tar.gz"
    rm -f "openssl-$OPENSSL_VERSION.tar.gz"
fi

# Build for all Android architectures
cd "$BUILD_DIR/openssl-$OPENSSL_VERSION"

for ARCH in arm64-v8a armeabi-v7a x86 x86_64; do
    echo "Building OpenSSL for $ARCH..."
    
    case "$ARCH" in
        arm64-v8a)
            OPENSSL_TARGET="android-arm64"
            ANDROID_TOOLCHAIN="aarch64-linux-android"
            ;;
        armeabi-v7a)
            OPENSSL_TARGET="android-arm"
            ANDROID_TOOLCHAIN="armv7a-linux-androideabi"
            ;;
        x86)
            OPENSSL_TARGET="android-x86"
            ANDROID_TOOLCHAIN="i686-linux-android"
            ;;
        x86_64)
            OPENSSL_TARGET="android-x86_64"
            ANDROID_TOOLCHAIN="x86_64-linux-android"
            ;;
    esac

    # Set up build environment
    export ANDROID_API=${ANDROID_API:-24}
    export PATH="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin:$PATH"
    export CC="$ANDROID_TOOLCHAIN$ANDROID_API-clang"
    export CXX="$ANDROID_TOOLCHAIN$ANDROID_API-clang++"
    export LD="$ANDROID_TOOLCHAIN-ld"
    export AR="llvm-ar"
    export RANLIB="llvm-ranlib"
    
    # Clean previous build
    make clean || true
    
    # Configure and build
    ./Configure $OPENSSL_TARGET \
        -D__ANDROID_API__=$ANDROID_API \
        --prefix="$OUTPUT_DIR/$ARCH" \
        no-shared \
        no-ssl2 \
        no-ssl3 \
        no-comp \
        no-hw \
        no-engine
    
    make -j$(nproc)
    make install_sw
    
    echo "OpenSSL build for $ARCH completed successfully"
done

echo "All OpenSSL builds completed"
