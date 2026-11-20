cmake_minimum_required(VERSION 3.24)
project(muster_remote_desktop_checks LANGUAGES C)

set(MOONLIGHT_CORE "${CMAKE_CURRENT_SOURCE_DIR}/vendor/moonlight-pc/moonlight-common-c/moonlight-common-c")
add_subdirectory("${MOONLIGHT_CORE}" moonlight-common-c)

enable_testing()
add_executable(moonlight_loopback_test tests/moonlight_loopback.c)
target_include_directories(moonlight_loopback_test PRIVATE "${MOONLIGHT_CORE}/src")
target_link_libraries(moonlight_loopback_test PRIVATE moonlight-common-c)
add_test(NAME moonlight_loopback COMMAND moonlight_loopback_test)
