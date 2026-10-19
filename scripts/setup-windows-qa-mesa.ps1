$ErrorActionPreference = 'Stop'
if ($env:CI -ne 'true' -or -not $env:RUNNER_TEMP -or -not $env:GITHUB_ENV) {
    throw 'Mesa setup is only for the disposable Windows CI runner'
}
# Pinned primary-publisher release/digest. Do not execute its deployment utilities
# or install a global driver: only the owned GUI fixture will receive these DLLs.
$qaMesaRoot = Join-Path $env:RUNNER_TEMP ('mod01-mesa-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $qaMesaRoot | Out-Null
$qaArchive = Join-Path $qaMesaRoot 'mesa3d-26.2.4-release-msvc.7z'
Invoke-WebRequest -Uri 'https://github.com/pal1000/mesa-dist-win/releases/download/26.2.4/mesa3d-26.2.4-release-msvc.7z' -OutFile $qaArchive
$qaExpectedHash = '351fc8c8b695878ffb3eaa044b3ead08672a48b1a045e3c3e3975811df0f6695'
if ((Get-FileHash -Algorithm SHA256 $qaArchive).Hash.ToLowerInvariant() -ne $qaExpectedHash) {
    throw 'Mesa download SHA-256 does not match the pinned release'
}
$qaDistribution = Join-Path $qaMesaRoot 'distribution'
& 7z.exe x $qaArchive "-o$qaDistribution" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not extract the verified Mesa release' }
$qaDriver = Join-Path $qaDistribution 'x64'
foreach ($qaRequired in @('opengl32.dll', 'libgallium_wgl.dll')) {
    if (-not (Test-Path (Join-Path $qaDriver $qaRequired) -PathType Leaf)) {
        throw "Missing 64-bit Mesa runtime: $qaRequired"
    }
}
"MOD01_QA_MESA_DIR=$qaDriver" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
Write-Output "Prepared pinned Mesa 26.2.4 for CI-only llvmpipe window QA ($qaExpectedHash)"
