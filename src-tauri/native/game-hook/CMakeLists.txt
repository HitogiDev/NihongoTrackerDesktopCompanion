cmake_minimum_required(VERSION 3.20)
project(ntdc_game_hook LANGUAGES CXX)

if(MSVC)
  set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
endif()

if(NOT WIN32)
  message(FATAL_ERROR "Native game hooking is Windows-only")
endif()

if(NOT DEFINED NTDC_OUTPUT_DIR)
  message(FATAL_ERROR "Pass -DNTDC_OUTPUT_DIR=<resources/game-hook/x64|x86>")
endif()

if(CMAKE_SIZEOF_VOID_P EQUAL 8)
  set(NTDC_LUNA_ARCH 64)
  set(NTDC_OUTPUT_ARCH x64)
elseif(CMAKE_SIZEOF_VOID_P EQUAL 4)
  set(NTDC_LUNA_ARCH 32)
  set(NTDC_OUTPUT_ARCH x86)
else()
  message(FATAL_ERROR "Unsupported target architecture")
endif()

add_executable(ntdc_game_hook main.cpp)
target_compile_features(ntdc_game_hook PRIVATE cxx_std_17)
target_compile_definitions(ntdc_game_hook PRIVATE UNICODE _UNICODE NOMINMAX WIN32_LEAN_AND_MEAN)
if(MSVC)
  target_compile_options(ntdc_game_hook PRIVATE /W4 /utf-8)
  target_link_libraries(ntdc_game_hook PRIVATE kernel32)
else()
  target_compile_options(ntdc_game_hook PRIVATE -Wall -Wextra -Wpedantic -Wno-cast-function-type)
  target_link_options(ntdc_game_hook PRIVATE -static -static-libgcc -static-libstdc++)
endif()

set_target_properties(ntdc_game_hook PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${NTDC_OUTPUT_DIR}"
  RUNTIME_OUTPUT_DIRECTORY_RELEASE "${NTDC_OUTPUT_DIR}"
  RUNTIME_OUTPUT_DIRECTORY_RELWITHDEBINFO "${NTDC_OUTPUT_DIR}"
  RUNTIME_OUTPUT_DIRECTORY_MINSIZEREL "${NTDC_OUTPUT_DIR}"
  RUNTIME_OUTPUT_DIRECTORY_DEBUG "${NTDC_OUTPUT_DIR}"
  PDB_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}"
  PDB_OUTPUT_DIRECTORY_RELEASE "${CMAKE_CURRENT_BINARY_DIR}"
  PDB_OUTPUT_DIRECTORY_RELWITHDEBINFO "${CMAKE_CURRENT_BINARY_DIR}"
  PDB_OUTPUT_DIRECTORY_MINSIZEREL "${CMAKE_CURRENT_BINARY_DIR}"
  PDB_OUTPUT_DIRECTORY_DEBUG "${CMAKE_CURRENT_BINARY_DIR}"
)

set(NTDC_LUNA_DIR "${CMAKE_CURRENT_SOURCE_DIR}/third_party/lunahook")
foreach(NTDC_LUNA_DLL IN ITEMS "LunaHost${NTDC_LUNA_ARCH}.dll" "LunaHook${NTDC_LUNA_ARCH}.dll")
  if(NOT EXISTS "${NTDC_LUNA_DIR}/${NTDC_LUNA_DLL}")
    message(FATAL_ERROR "Missing vendored dependency: ${NTDC_LUNA_DIR}/${NTDC_LUNA_DLL}")
  endif()
  add_custom_command(TARGET ntdc_game_hook POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
      "${NTDC_LUNA_DIR}/${NTDC_LUNA_DLL}" "${NTDC_OUTPUT_DIR}/${NTDC_LUNA_DLL}"
    VERBATIM)
endforeach()
