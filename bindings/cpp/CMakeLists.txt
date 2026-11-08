cmake_minimum_required(VERSION 3.16)
project(geoda_cpp LANGUAGES CXX)

# Envoltorio C++17 de solo cabeceras sobre libgeoda (ABI 1.x). Uso:
#   cargo build -p geoda-capi
#   cmake -S bindings/cpp -B build/cpp -DGEODA_LIB_DIR=$PWD/target/debug
#   cmake --build build/cpp && ctest --test-dir build/cpp --output-on-failure
set(GEODA_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug" CACHE PATH "carpeta con libgeoda.so")
set(GEODA_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/capi/include" CACHE PATH "carpeta con geoda.h")
option(GEODA_SANITIZE "compilar con ASan y UBSan (incluye LeakSanitizer)" ON)

find_library(GEODA_LIB geoda PATHS "${GEODA_LIB_DIR}" NO_DEFAULT_PATH REQUIRED)

add_library(geoda_cpp INTERFACE)
target_compile_features(geoda_cpp INTERFACE cxx_std_17)
target_include_directories(geoda_cpp INTERFACE
  "${CMAKE_CURRENT_SOURCE_DIR}/include" "${GEODA_INCLUDE_DIR}")
target_link_libraries(geoda_cpp INTERFACE "${GEODA_LIB}")
set_property(TARGET geoda_cpp PROPERTY INTERFACE_LINK_OPTIONS "-Wl,-rpath,${GEODA_LIB_DIR}")

enable_testing()
add_executable(geoda_cpp_test tests/test_geoda.cpp)
target_link_libraries(geoda_cpp_test PRIVATE geoda_cpp)
target_compile_options(geoda_cpp_test PRIVATE -Wall -Wextra -Wpedantic -Werror)
if(GEODA_SANITIZE)
  target_compile_options(geoda_cpp_test PRIVATE -fsanitize=address,undefined -fno-omit-frame-pointer)
  target_link_options(geoda_cpp_test PRIVATE -fsanitize=address,undefined)
endif()
add_test(NAME geoda_cpp_test COMMAND geoda_cpp_test)

install(DIRECTORY include/geoda/ DESTINATION include/geoda FILES_MATCHING PATTERN "*.hpp")
install(FILES "${GEODA_INCLUDE_DIR}/geoda.h" DESTINATION include/geoda)
