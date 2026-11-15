# ORVYN release packaging: browser, installer, release folder, ZIPs, SHA-256.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File C:\orbit\tools\package-release.ps1
#   powershell ... -SkipTests
#
# Steps:
#   1. build browser and installer (release);
#   2. put WebView2Loader.dll next to ORVYN.exe (Windows needs it to start the exe);
#   3. refresh C:\orbit\releases\ORVYN-2.2.0 and both ZIPs;
#   4. write SHA-256 sums.
#
# Nothing is uploaded anywhere; no paid services involved.

param(
    [switch]$SkipTests
)

$ErrorActionPreference = "Stop"

$root = "C:\orbit"
$version = "3.0.2"
$rel = Join-Path $root "releases\ORVYN-$version"
$browserDir = Join-Path $root "rustbrowser"
$setupDir = Join-Path $root "rustbrowser-setup"
$browserTarget = "D:\build\rustbrowser"
$setupTarget = "D:\build\orbit-setup"

$env:PATH = "D:\toolchain\mingw64\bin;D:\Rust\cargo\bin;$env:PATH"
$env:CARGO_HOME = "D:\Rust\cargo"
$env:RUSTUP_HOME = "D:\Rust\rustup"

function Step([string]$text) { Write-Host "== $text" }

# A running ORVYN holds the exe and blocks the build.
Get-Process orvyn, OrvynSetup -ErrorAction SilentlyContinue |
    ForEach-Object { taskkill /PID $_.Id /T /F | Out-Null }
Start-Sleep -Seconds 1

Step "Check WebView2Loader.dll"
$loader = Join-Path $setupDir "WebView2Loader.dll"
if (-not (Test-Path $loader)) {
    throw "missing $loader (take x64\WebView2Loader.dll from webview2-com-sys)"
}

Step "Tests"
if (-not $SkipTests) {
    Push-Location $browserDir
    try {
        cargo test --target-dir $browserTarget
        if ($LASTEXITCODE -ne 0) { throw "tests failed" }
    }
    finally { Pop-Location }
}

Step "Build browser"
Push-Location $browserDir
try {
    cargo build --release --target-dir $browserTarget
    if ($LASTEXITCODE -ne 0) { throw "browser build failed" }
}
finally { Pop-Location }

$browserExe = Join-Path $browserTarget "release\orvyn.exe"
Copy-Item $browserExe (Join-Path $setupDir "browser.exe") -Force

Step "Build installer"
Push-Location $setupDir
try {
    cargo build --release --target-dir $setupTarget
    if ($LASTEXITCODE -ne 0) { throw "installer build failed" }
}
finally { Pop-Location }

$setupExe = Join-Path $setupTarget "release\OrvynSetup.exe"

Step "Assemble release folder"
New-Item -ItemType Directory -Force $rel | Out-Null
$targetExe = Join-Path $rel "ORVYN.exe"
$targetSetup = Join-Path $rel "OrvynSetup-$version.exe"
$targetLoader = Join-Path $rel "WebView2Loader.dll"
Copy-Item $browserExe $targetExe -Force
Copy-Item $setupExe $targetSetup -Force
Copy-Item $loader $targetLoader -Force

$installTemplate = Join-Path $root "tools\INSTALL-template.txt"
if (-not (Test-Path $installTemplate)) { throw "missing $installTemplate" }
$installText = Get-Content $installTemplate -Raw -Encoding UTF8
$installText | Out-File (Join-Path $rel "INSTALL.txt") -Encoding utf8

Step "Source archive"
$sourceZip = Join-Path $rel "ORVYN-$version-source.zip"
if (Test-Path $sourceZip) { Remove-Item $sourceZip -Force }
Push-Location $root
try {
    tar -a -c -f $sourceZip --exclude "rustbrowser/target" --exclude "rustbrowser-setup/browser.exe" rustbrowser rustbrowser-setup tools
    if ($LASTEXITCODE -ne 0) { throw "source archive failed" }
}
finally { Pop-Location }

Step "Checksums"
$sums = @()
foreach ($name in @("ORVYN.exe", "WebView2Loader.dll", "OrvynSetup-$version.exe", "ORVYN-$version-source.zip")) {
    $hash = (Get-FileHash (Join-Path $rel $name) -Algorithm SHA256).Hash.ToLower()
    $sums += "$hash  $name"
}
$sums | Out-File (Join-Path $rel "SHA256SUMS.txt") -Encoding utf8

Add-Type -AssemblyName System.IO.Compression.FileSystem

Step "Browser ZIP"
$mainZip = Join-Path $root "releases\ORVYN-$version.zip"
if (Test-Path $mainZip) { Remove-Item $mainZip -Force }
[IO.Compression.ZipFile]::CreateFromDirectory($rel, $mainZip, [IO.Compression.CompressionLevel]::Optimal, $false)

Step "Installer ZIP"
$staging = Join-Path $env:TEMP "orvyn-setup-zip"
Remove-Item $staging -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $staging | Out-Null
Copy-Item $targetSetup (Join-Path $staging "OrvynSetup-$version.exe")
Copy-Item (Join-Path $rel "INSTALL.txt") (Join-Path $staging "INSTALL.txt")
$setupHash = (Get-FileHash $targetSetup -Algorithm SHA256).Hash.ToLower()
"$setupHash  OrvynSetup-$version.exe" | Out-File (Join-Path $staging "SHA256.txt") -Encoding utf8
$setupZip = Join-Path $root "releases\ORVYN-Setup-$version.zip"
if (Test-Path $setupZip) { Remove-Item $setupZip -Force }
[IO.Compression.ZipFile]::CreateFromDirectory($staging, $setupZip, [IO.Compression.CompressionLevel]::Optimal, $false)
Remove-Item $staging -Recurse -Force

Step "Done"
Get-ChildItem $rel | Select-Object Name, Length
Write-Host ""
Get-Content (Join-Path $rel "SHA256SUMS.txt")
"ZIP: $mainZip  $((Get-Item $mainZip).Length) bytes  $((Get-FileHash $mainZip).Hash.ToLower())"
"ZIP: $setupZip  $((Get-Item $setupZip).Length) bytes  $((Get-FileHash $setupZip).Hash.ToLower())"
