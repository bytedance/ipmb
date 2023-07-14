cmake_minimum_required(VERSION 3.25)
project(ipmb)

set(CMAKE_CXX_STANDARD 14)
set(EXECUTABLE_OUTPUT_PATH "target/debug")

include_directories(ipmb-ffi/include)

link_directories(target/debug)

add_executable(cc_client_plus
        ipmb-ffi/examples/cc_client_plus.cc
        ipmb-ffi/ipmb.cc)
target_link_libraries(cc_client_plus ipmb_ffi)

add_executable(cc_client
        ipmb-ffi/examples/cc_client.cc)
target_link_libraries(cc_client ipmb_ffi)
