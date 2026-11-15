$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item target/release/aula_l99_linux.exe 'dist/AULA-L99.exe'
    Write-Host 'Created dist/AULA-L99.exe (desktop preview; Linux hardware features are unavailable)'
} finally { Pop-Location }
