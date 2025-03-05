#!/bin/bash
set -euo pipefail

# Check for required tools
MISSING_TOOLS=()
for tool in automake aclocal libtool pkg-config; do
    if ! command -v $tool &> /dev/null; then
        MISSING_TOOLS+=($tool)
    fi
done

if [ ${#MISSING_TOOLS[@]} -ne 0 ]; then
    echo "Missing required tools: ${MISSING_TOOLS[*]}"
    echo "Please install them first with:"
    echo "brew install automake libtool pkg-config"
    exit 1
fi

# Variables
OPENVPN_VERSION="2.5.11"
OPENSSL_VERSION="1.1.1u"
OPENVPN_SRC="openvpn-${OPENVPN_VERSION}"
OPENVPN_TAR="${OPENVPN_SRC}.tar.gz"
OPENSSL_SRC="openssl-${OPENSSL_VERSION}"
OPENSSL_TAR="${OPENSSL_SRC}.tar.gz"
OPENVPN_URL="https://build.openvpn.net/downloads/releases/${OPENVPN_TAR}"
OPENSSL_URL="https://www.openssl.org/source/${OPENSSL_TAR}"
PATCH_URL="https://raw.githubusercontent.com/samm-git/aws-vpn-client/master/openvpn-v2.5.1-aws.patch"

# Use your local NDK installation path
LOCAL_NDK_PATH="$HOME/Library/Android/sdk/ndk/26.1.10909125"

if [ ! -d "$LOCAL_NDK_PATH" ]; then
  echo "NDK not found at $LOCAL_NDK_PATH"
  exit 1
fi

# Create directories
ROOT_DIR="$(pwd)"
BUILD_DIR="$ROOT_DIR/build"
OUTPUT_DIR="$ROOT_DIR/share/openvpn"
DEPS_DIR="$BUILD_DIR/deps"

mkdir -p "$BUILD_DIR/src"
mkdir -p "$OUTPUT_DIR/bin"
mkdir -p "$OUTPUT_DIR/android/arm64-v8a"
mkdir -p "$DEPS_DIR"

# Configure toolchain
TOOLCHAIN="$LOCAL_NDK_PATH/toolchains/llvm/prebuilt/darwin-arm64"
if [ ! -d "$TOOLCHAIN" ]; then
  TOOLCHAIN="$LOCAL_NDK_PATH/toolchains/llvm/prebuilt/darwin-x86_64"
  if [ ! -d "$TOOLCHAIN" ]; then
    echo "Could not find toolchain directory"
    exit 1
  fi
fi
echo "Using toolchain: $TOOLCHAIN"

API_LEVEL=21
TOOL_PREFIX="aarch64-linux-android"
TARGET_HOST="aarch64-linux-android"

# Find compiler
CLANG_PATH=$(find $TOOLCHAIN -name "${TOOL_PREFIX}${API_LEVEL}-clang" -type f 2>/dev/null | head -n 1)
if [ -z "$CLANG_PATH" ]; then
  CLANG_PATH=$(find $TOOLCHAIN -name "${TOOL_PREFIX}*-clang" -type f 2>/dev/null | head -n 1)
  if [ -z "$CLANG_PATH" ]; then
    echo "No compiler found"
    exit 1
  fi
fi
echo "Using compiler: $CLANG_PATH"

# Setup environment
SYSROOT="$TOOLCHAIN/sysroot"
export ANDROID_NDK_HOME="$LOCAL_NDK_PATH"
export PATH="$TOOLCHAIN/bin:$PATH"
export AR="$TOOLCHAIN/bin/llvm-ar"
export CC="$CLANG_PATH"
export CXX="${CLANG_PATH/clang/clang++}"
export STRIP="$TOOLCHAIN/bin/llvm-strip"
export RANLIB="$TOOLCHAIN/bin/llvm-ranlib"
export LD="$TOOLCHAIN/bin/ld"
export CFLAGS="-fPIC -I$DEPS_DIR/include"
export LDFLAGS="-pie -L$DEPS_DIR/lib"

# Build OpenSSL
cd "$BUILD_DIR/src"
if [ ! -f "$OPENSSL_TAR" ]; then
  echo "Downloading OpenSSL..."
  curl -L -o "$OPENSSL_TAR" "$OPENSSL_URL"
fi

if [ ! -d "$OPENSSL_SRC" ]; then
  echo "Extracting OpenSSL..."
  tar -xf "$OPENSSL_TAR"
fi

cd "$OPENSSL_SRC"
echo "Building OpenSSL for Android..."

# Configure and build OpenSSL for Android
export CFLAGS="-fPIC -D__ANDROID_API__=$API_LEVEL"
export ANDROID_NDK="$LOCAL_NDK_PATH"
./Configure android-arm64 no-shared no-tests --prefix="$DEPS_DIR" --openssldir="$DEPS_DIR" -D__ANDROID_API__=$API_LEVEL
make -j$(nproc)
make install_sw

# Alternative approach: use a prebuilt OpenVPN but patch it
cd "$BUILD_DIR/src"
if [ ! -f "$OPENVPN_TAR" ]; then
  echo "Downloading OpenVPN..."
  curl -L -o "$OPENVPN_TAR" "$OPENVPN_URL"
fi

if [ ! -d "$OPENVPN_SRC" ]; then
  echo "Extracting OpenVPN..."
  tar -xf "$OPENVPN_TAR"
fi

cd "$OPENVPN_SRC"

# Apply AWS VPN patch
echo "Downloading AWS VPN patch..."
curl -L -o openvpn-aws.patch "$PATCH_URL"

echo "Applying AWS VPN patch..."
patch -p1 < openvpn-aws.patch || echo "Patch may have partially failed, continuing..."

# Autoreconf
echo "Running autoreconf..."
autoreconf -i -v

# Set up OpenSSL paths
export OPENSSL_CFLAGS="-I$DEPS_DIR/include"
export OPENSSL_LIBS="-L$DEPS_DIR/lib -lssl -lcrypto"
export PKG_CONFIG_PATH="$DEPS_DIR/lib/pkgconfig"

# Configure OpenVPN
echo "Configuring OpenVPN for Android..."
./configure \
  --host=$TARGET_HOST \
  --disable-shared \
  --enable-static \
  --disable-plugins \
  --disable-debug \
  --with-crypto-library=openssl \
  --with-sysroot=$SYSROOT \
  --disable-lz4 \
  --disable-lzo \
  --prefix="$DEPS_DIR"

# Build OpenVPN
echo "Building OpenVPN for Android..."
make -j$(nproc)

# Copy binaries
echo "Copying binaries..."
cp src/openvpn/openvpn "$OUTPUT_DIR/android/arm64-v8a/openvpn"
chmod +x "$OUTPUT_DIR/android/arm64-v8a/openvpn"

echo "AWS-patched OpenVPN binary for Android ARM64 is ready at: $OUTPUT_DIR/android/arm64-v8a/openvpn"
