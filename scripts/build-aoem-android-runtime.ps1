# Reproduce the original mobile AOEM build; this does not qualify or install a runtime.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$AoemRepository,
    [Parameter(Mandatory)][string]$SourceOverlay,
    [Parameter(Mandatory)][string]$QualifiedLock,
    [Parameter(Mandatory)][string]$NdkRoot,
    [Parameter(Mandatory)][string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$baseCommit = '4c57372ad0cc12123df5ca9e163a130039bfb599'
$overlayHash = '8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb'
$lockHash = '46625b6be8ad25c323fbba6e23263fe9f9f1b649805303ebdb89a7fe7f7abbec'
foreach ($inputFile in @(@($SourceOverlay, $overlayHash), @($QualifiedLock, $lockHash))) {
    if ((Get-FileHash -LiteralPath $inputFile[0] -Algorithm SHA256).Hash -ine $inputFile[1]) {
        throw "Fixed source input mismatch: $($inputFile[0])"
    }
}
$NdkRoot = (Resolve-Path -LiteralPath $NdkRoot).Path
if ((Get-Content -LiteralPath (Join-Path $NdkRoot 'source.properties') -Raw) -notmatch 'Pkg.Revision\s*=\s*28\.2\.13676358\s') {
    throw 'NDK 28.2.13676358 required'
}
$ndkBin = Join-Path $NdkRoot 'toolchains/llvm/prebuilt/windows-x86_64/bin'
$compiler = Join-Path $ndkBin 'aarch64-linux-android24-clang.cmd'
$glslc = Join-Path $NdkRoot 'shader-tools/windows-x86_64/glslc.exe'
foreach ($tool in @($compiler, $glslc, (Join-Path $ndkBin 'llvm-readelf.exe'))) {
    if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) { throw "Missing tool: $tool" }
}
if (Test-Path -LiteralPath $OutputRoot) { throw 'Use a new output directory; prior failure evidence is not overwritten' }
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
New-Item -ItemType Directory -Path $OutputRoot | Out-Null
$source = Join-Path $OutputRoot 'source'
$package = Join-Path $OutputRoot 'package'
New-Item -ItemType Directory -Path $source, $package | Out-Null
# Export the original build inputs, not historical documentation/distribution trees.
# ZIP preserves UTF-8 names on Windows; system tar rejected historical CJK names.
$sourcePaths = @('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates', 'config', 'AGENTS.md', 'LICENSE.COMMERCIAL', 'LICENSE.RESEARCH', 'THIRD_PARTY_NOTICES.txt')
& git -C $AoemRepository archive --format=zip "--output=$OutputRoot/source-base.zip" $baseCommit -- @sourcePaths
if ($LASTEXITCODE -ne 0) { throw 'Fixed source export failed' }
# The overlay is an independently pinned existing AOEM input, not the dirty worktree.
[IO.Compression.ZipFile]::ExtractToDirectory((Join-Path $OutputRoot 'source-base.zip'), $source)
& tar -xf $SourceOverlay -C $source 2>&1 | Tee-Object -FilePath (Join-Path $OutputRoot 'overlay-extract.log')
if ($LASTEXITCODE -ne 0) { throw 'Fixed overlay extraction failed' }
Copy-Item -LiteralPath $QualifiedLock -Destination (Join-Path $source 'Cargo.lock')
$environment = @{
    CARGO_TARGET_DIR = (Join-Path $OutputRoot 'build')
    CARGO_BUILD_JOBS = '4'
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $compiler
    CC_aarch64_linux_android = $compiler
    AR_aarch64_linux_android = (Join-Path $ndkBin 'llvm-ar.exe')
    AOEM_GLSLC_PATH = $glslc
}
$saved = @{}
foreach ($name in $environment.Keys) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    [Environment]::SetEnvironmentVariable($name, $environment[$name], 'Process')
}
try {
    & rustc +1.94.0 -Vv | Tee-Object -FilePath (Join-Path $OutputRoot 'rustc.txt')
    if ($LASTEXITCODE -ne 0) { throw 'Pinned Rust compiler unavailable' }
    & cargo +1.94.0 build --manifest-path (Join-Path $source 'Cargo.toml') --locked --release -p aoem-ffi --no-default-features --features mldsa --target aarch64-linux-android 2>&1 |
        Tee-Object -FilePath (Join-Path $OutputRoot 'build.log')
    $buildExit = $LASTEXITCODE
    $buildExit | Set-Content -LiteralPath (Join-Path $OutputRoot 'build-exit.txt')
    if ($buildExit -ne 0) { throw "Android build failed ($buildExit); original log retained" }
} finally {
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
$library = Join-Path $package 'libaoem_ffi.so'
Copy-Item -LiteralPath (Join-Path $OutputRoot 'build/aarch64-linux-android/release/libaoem_ffi.so') -Destination $library
Copy-Item -LiteralPath (Join-Path $source 'crates/ffi/aoem-ffi/include/aoem.h') -Destination $package
foreach ($notice in @('LICENSE.COMMERCIAL', 'LICENSE.RESEARCH', 'THIRD_PARTY_NOTICES.txt')) {
    Copy-Item -LiteralPath (Join-Path $source $notice) -Destination $package
}
Copy-Item -LiteralPath (Join-Path $source 'crates/adapter/aoem-adapter-crypto/vendor/mldsa-native/LICENSE') -Destination (Join-Path $package 'mldsa-native-LICENSE')
& (Join-Path $ndkBin 'llvm-readelf.exe') --file-header --dynamic --dyn-syms $library |
    Tee-Object -FilePath (Join-Path $OutputRoot 'elf.txt') | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'ELF inspection failed' }
$elf = Get-Content -LiteralPath (Join-Path $OutputRoot 'elf.txt') -Raw
if ($elf -notmatch 'Machine:\s+AArch64' -or $elf -notmatch 'Type:\s+DYN' -or $elf -match 'libc\.so\.6') {
    throw 'Not an Android AArch64 shared library'
}
foreach ($symbol in @('aoem_execute_ops_wire_v1', 'aoem_compute_result_take_v1', 'aoem_capabilities_json', 'aoem_mldsa_keygen_v1', 'aoem_mldsa_sign_v1')) {
    if ($elf -notmatch "\b$symbol\b") { throw "Required original export missing: $symbol" }
}
[ordered]@{
    schema_version = 1
    scope = 'Android ARM64 mobile mldsa build candidate; not a full node or desktop fullmax package'
    aoem_base_commit = $baseCommit
    source_paths = $sourcePaths
    source_archive_sha256 = (Get-FileHash -LiteralPath (Join-Path $OutputRoot 'source-base.zip') -Algorithm SHA256).Hash.ToLowerInvariant()
    source_overlay_sha256 = $overlayHash
    cargo_lock_sha256 = $lockHash
    target = 'aarch64-linux-android'
    android_api = 24
    ndk = '28.2.13676358'
    rust = '1.94.0'
    default_features = $false
    features = @('mldsa')
    core_sha256 = (Get-FileHash -LiteralPath $library -Algorithm SHA256).Hash.ToLowerInvariant()
    core_bytes = (Get-Item -LiteralPath $library).Length
    header_sha256 = (Get-FileHash -LiteralPath (Join-Path $package 'aoem.h') -Algorithm SHA256).Hash.ToLowerInvariant()
    android_execution_verified = $false
    unified_mldsa_positive_negative_verified = $false
    app_custody_verified = $false
    installed_or_published = $false
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $package 'candidate-manifest.json') -Encoding utf8
Write-Output "BUILD_ONLY: $package; require original Android consumer and device evidence before qualification"
