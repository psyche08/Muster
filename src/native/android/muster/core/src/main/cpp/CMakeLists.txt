# The JNI shell plus the Rust core as one shared library per ABI.
# libmuster_ffi.a comes from scripts/build-ffi.sh (cargo ndk) under
# src/muster/target/<triple>/release.
cmake_minimum_required(VERSION 3.22)
project(muster_jni C)

set(MUSTER_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../muster")
if(ANDROID_ABI STREQUAL "arm64-v8a")
    set(MUSTER_TRIPLE aarch64-linux-android)
elseif(ANDROID_ABI STREQUAL "x86_64")
    set(MUSTER_TRIPLE x86_64-linux-android)
elseif(ANDROID_ABI STREQUAL "armeabi-v7a")
    set(MUSTER_TRIPLE armv7-linux-androideabi)
else()
    message(FATAL_ERROR "no Rust target for ABI ${ANDROID_ABI}")
endif()
set(MUSTER_PROFILE "release" CACHE STRING "cargo profile the core was built with")
set(MUSTER_FFI "${MUSTER_ROOT}/target/${MUSTER_TRIPLE}/${MUSTER_PROFILE}/libmuster_ffi.a")
if(NOT EXISTS "${MUSTER_FFI}")
    message(FATAL_ERROR "missing ${MUSTER_FFI}: run scripts/build-ffi.sh first")
endif()

add_library(muster_jni SHARED muster_jni.c)
target_include_directories(muster_jni PRIVATE "${MUSTER_ROOT}/crates/muster-ffi/include")
target_link_libraries(muster_jni PRIVATE "${MUSTER_FFI}" log dl m)
# The Rust archive exports only muster_*; keep the JNI symbols and nothing else.
target_link_options(muster_jni PRIVATE -Wl,--exclude-libs,ALL)
