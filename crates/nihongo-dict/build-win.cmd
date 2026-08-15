@echo off
REM Spike helper: configure and build hoshidicts with the MSVC toolchain the
REM Tauri app already uses. Not part of the app build; the real integration
REM would go through build.rs.
REM
REM Two flags are mandatory on MSVC, and neither is optional polish:
REM  /utf-8            the deconjugation rules are a 35 KB UTF-8 raw string
REM                    literal; without this MSVC decodes the source with the
REM                    system code page (CP932 here) and mangles the Japanese.
REM  /Zc:__cplusplus   without it MSVC reports __cplusplus as 199711L, so utfcpp
REM                    hides its C++17/20 API and text_processor.cpp cannot find
REM                    utf8::utf8to32(string_view).
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set CMAKE="C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
set NINJA="C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe"
%CMAKE% -S "%~dp0hoshidicts" -B "%~dp0build-win" -G Ninja -DCMAKE_MAKE_PROGRAM=%NINJA% -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS="/utf-8 /Zc:__cplusplus" || exit /b 1
%CMAKE% --build "%~dp0build-win" --target hoshidicts hoshidicts-cli || exit /b 1
echo BUILD-OK
