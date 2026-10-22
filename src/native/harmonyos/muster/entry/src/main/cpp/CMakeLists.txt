# The NAPI shell plus the Rust core as libmuster.so. libmuster_ffi.a comes from
# scripts/build-ffi.sh (the OpenHarmony NDK's clang as the cargo linker)
# under src/muster/target/<triple>/release.
cmake_minimum_required(VERSION 3.5.0)
project(muster CXX)

set(MUSTER_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../../muster")
if(OHOS_ARCH STREQUAL "arm64-v8a")
    set(MUSTER_TRIPLE aarch64-unknown-linux-ohos)
elseif(OHOS_ARCH STREQUAL "x86_64")
    set(MUSTER_TRIPLE x86_64-unknown-linux-ohos)
else()
    message(FATAL_ERROR "no Rust target for ${OHOS_ARCH}")
endif()
set(MUSTER_FFI "${MUSTER_ROOT}/target/${MUSTER_TRIPLE}/release/libmuster_ffi.a")
if(NOT EXISTS "${MUSTER_FFI}")
    message(FATAL_ERROR "missing ${MUSTER_FFI}: run scripts/build-ffi.sh first")
endif()

set(CMAKE_CXX_STANDARD 17)
add_library(muster SHARED muster_napi.cpp)
target_include_directories(muster PRIVATE "${MUSTER_ROOT}/crates/muster-ffi/include" ${NATIVERENDER_ROOT_PATH})
target_link_libraries(muster PUBLIC libace_napi.z.so libhilog_ndk.z.so libhuks_ndk.z.so "${MUSTER_FFI}")
