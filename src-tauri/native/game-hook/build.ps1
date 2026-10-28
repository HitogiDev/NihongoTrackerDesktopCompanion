param([switch]$RequireAll)

$ErrorActionPreference = 'Stop'

$sourceRoot = [IO.Path]::GetFullPath($PSScriptRoot)
$tauriRoot = [IO.Path]::GetFullPath((Join-Path $sourceRoot '..\..'))
$outputRoot = [IO.Path]::GetFullPath((Join-Path $tauriRoot 'resources\game-hook'))
$buildRoot = [IO.Path]::GetFullPath((Join-Path $tauriRoot 'target\native-game-hook'))
$cmakePath = (Get-Command cmake -ErrorAction Stop).Source
$generator = $env:CMAKE_GENERATOR
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = if ($programFilesX86) {
  Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'
} else { $null }
$vsInstall = if ($vswhere -and (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
  & $vswhere -latest -prerelease -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
} else { $null }
$vsInstall = $vsInstall | Select-Object -First 1
$vsCmake = $null
$vsNinja = $null
if (-not [string]::IsNullOrWhiteSpace($vsInstall)) {
  $vsCmake = Join-Path $vsInstall 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
  $vsNinja = Join-Path $vsInstall 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe'
  if (Test-Path -LiteralPath $vsCmake -PathType Leaf) {
    $cmakePath = $vsCmake
  }
}

if ([string]::IsNullOrWhiteSpace($generator)) {
  if (-not [string]::IsNullOrWhiteSpace($vsInstall)) {
    $generator = 'Ninja'
  } else {
    $generator = 'Ninja'
  }
}
$isVisualStudio = $generator.StartsWith('Visual Studio', [StringComparison]::OrdinalIgnoreCase)
$useMsvc = -not $isVisualStudio -and -not [string]::IsNullOrWhiteSpace($vsInstall)

foreach ($arch in @(
  @{ Name = 'x64'; GeneratorArch = 'x64' },
  @{ Name = 'x86'; GeneratorArch = 'Win32' }
)) {
  $name = $arch.Name
  $lunaArch = if ($name -eq 'x64') { '64' } else { '32' }
  $generatorKey = $generator -replace '[^A-Za-z0-9]+', '-'
  $toolchainKey = if ($useMsvc) { 'MSVC' } elseif ($isVisualStudio) { 'MSVC' } else { 'MinGW' }
  $buildDir = Join-Path $buildRoot "$name-$generatorKey-$toolchainKey"
  $outputDir = Join-Path $outputRoot $name
  New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

  $configureArgs = @('-G', $generator, '-S', $sourceRoot, '-B', $buildDir)
  if ($isVisualStudio) {
    $configureArgs += @('-A', $arch.GeneratorArch)
  } else {
    $configureArgs += '-DCMAKE_BUILD_TYPE=Release'
  }
  if (-not $isVisualStudio -and $useMsvc) {
    $vsDevCmd = Join-Path $vsInstall 'Common7\Tools\VsDevCmd.bat'
    if (-not (Test-Path -LiteralPath $vsDevCmd -PathType Leaf)) {
      throw "Visual Studio developer tools were not found at $vsDevCmd."
    }
    $targetArch = if ($name -eq 'x64') { 'x64' } else { 'x86' }
    $envLines = & $env:ComSpec /d /s /c "call `"$vsDevCmd`" -arch=$targetArch -host_arch=x64 >nul && set"
    if ($LASTEXITCODE -ne 0) { throw "Could not initialize the Visual Studio $targetArch compiler environment." }
    foreach ($line in $envLines) {
      if ($line -match '^([^=]+)=(.*)$') {
        [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
      }
    }
    $compiler = Get-Command cl.exe -ErrorAction SilentlyContinue
    if (-not $compiler) { throw "Visual Studio did not provide the $targetArch C++ compiler." }
    if (-not (Test-Path -LiteralPath $vsNinja -PathType Leaf)) {
      throw "Visual Studio Ninja was not found at $vsNinja."
    }
    $configureArgs += @("-DCMAKE_CXX_COMPILER=$($compiler.Source)", "-DCMAKE_MAKE_PROGRAM=$vsNinja")
  } else {
    $compilerName = if ($name -eq 'x64') { 'x86_64-w64-mingw32-g++.exe' } else { 'i686-w64-mingw32-g++.exe' }
    $compiler = Get-Command $compilerName -ErrorAction SilentlyContinue
    if (-not $compiler) {
      if ($RequireAll) {
        throw "Cannot build the $name helper with generator '$generator'. Install Visual Studio C++ build tools or $compilerName."
      }
      Write-Warning "Skipping the $name game hook helper because $compilerName is unavailable."
      continue
    }
    $configureArgs += "-DCMAKE_CXX_COMPILER=$($compiler.Source)"
  }
  $configureArgs += "-DNTDC_OUTPUT_DIR=$outputDir"
  & $cmakePath @configureArgs
  if ($LASTEXITCODE -ne 0) { throw "CMake configure failed for $name (exit $LASTEXITCODE)" }
  & $cmakePath --build $buildDir --config Release --target ntdc_game_hook
  if ($LASTEXITCODE -ne 0) { throw "Native hook build failed for $name (exit $LASTEXITCODE)" }

  $helper = Join-Path $outputDir 'ntdc_game_hook.exe'
  if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) {
    throw "Native hook helper was not produced for $name"
  }
  foreach ($file in @("LunaHost$lunaArch.dll", "LunaHook$lunaArch.dll")) {
    if (-not (Test-Path -LiteralPath (Join-Path $outputDir $file) -PathType Leaf)) {
      throw "Architecture-matched dependency missing for $name`: $file"
    }
  }
}
