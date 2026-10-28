param([string]$TestExecutable, [switch]$ExpectDeadlock)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskCache = Join-Path $taskRoot 'node_modules/.cache/reading-overlay'
New-Item -ItemType Directory -Path $taskCache -Force | Out-Null
$taskManifest = Join-Path $taskCache 'runtime-test.manifest'
@'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="amd64" publicKeyToken="6595b64144ccf1df" language="*" />
  </dependentAssembly></dependency>
</assembly>
'@ | Set-Content -LiteralPath $taskManifest -Encoding utf8
$taskManifestTool = Get-ChildItem 'C:/Program Files (x86)/Windows Kits/10/bin' -Directory -Filter '10.*' |
    Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName 'x64/mt.exe' } |
    Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (!$taskManifestTool) { throw 'Install the Windows SDK to run the native overlay regression.' }
if (!$TestExecutable) {
    $taskBuildLog = Join-Path $taskCache 'runtime-build.log'
    $taskPreviousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $taskArtifacts = & cargo test --offline --manifest-path (Join-Path $taskRoot 'src-tauri/Cargo.toml') --features native-runtime-test --lib --no-run --message-format=json 2> $taskBuildLog
        $taskBuildExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $taskPreviousPreference }
    if ($taskBuildExit -ne 0) {
        Get-Content -LiteralPath $taskBuildLog
        throw 'Could not compile the native overlay regression.'
    }
    $TestExecutable = $taskArtifacts | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } |
        Select-Object -Last 1 -ExpandProperty executable
}
$taskExe = Join-Path $taskCache 'ntdc-overlay-runtime-test.exe'
Copy-Item -LiteralPath $TestExecutable -Destination $taskExe
# Unit test executables do not receive Tauri's Common Controls application manifest.
& $taskManifestTool -nologo -manifest $taskManifest "-outputresource:$taskExe;#1"
if ($LASTEXITCODE -ne 0) { throw 'Could not attach the native test manifest.' }
$taskOut = Join-Path $taskCache 'runtime-test.stdout.txt'
$taskErr = Join-Path $taskCache 'runtime-test.stderr.txt'
$taskTest = Start-Process -FilePath $taskExe -ArgumentList @('reading_overlay::tests::visibility_poll_and_window_callbacks_do_not_deadlock', '--exact', '--ignored', '--nocapture', '--test-threads=1') -WindowStyle Hidden -PassThru -RedirectStandardOutput $taskOut -RedirectStandardError $taskErr
$taskFinished = $taskTest.WaitForExit(30000)
if (!$taskFinished) {
    $taskTest.Kill()
    $taskTest.WaitForExit()
}
Get-Content -LiteralPath $taskOut
Get-Content -LiteralPath $taskErr
if ($ExpectDeadlock) {
    if ($taskFinished) { throw 'The original code did not deadlock.' }
    Write-Output 'CONFIRMED: original visibility polling deadlocked the native Windows event loop.'
} else {
    if (!$taskFinished) { throw 'Native Windows event loop deadlocked.' }
    if ($taskTest.ExitCode -ne 0) { throw "Native regression failed: $($taskTest.ExitCode)" }
    Write-Output 'PASS: 100 overlapping focus queries completed; settings stay above the dictionary after reopening.'
}
