$ErrorActionPreference = "Stop"
$usage = 'Usage: install.ps1 [<tool folder>] [-Native]'
$Crate = "."
$Native = $false
foreach ($argument in $args) {
    $argument = [string]$argument
    if ($argument -match '^--?native$') {
        $Native = $true
    }
    elseif ($argument.StartsWith("-")) {
        throw "install.ps1 doesn't take $argument. $usage"
    }
    else {
        $Crate = $argument
    }
}
if (-not (Test-Path $Crate -PathType Container)) {
    throw "There is no folder named $Crate here. Run this from the notchbase-tools folder with the name of the tool's folder. $usage"
}
Push-Location $Crate
try {
    if (-not (Test-Path "Cargo.toml" -PathType Leaf)) {
        throw "$((Get-Location).Path) has no Cargo.toml, so it isn't a tool. Name the tool's own folder."
    }
    $ownCargo = if ($env:USERPROFILE) { Join-Path $env:USERPROFILE ".cargo\bin" } else { "" }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -and $ownCargo -and (Test-Path (Join-Path $ownCargo "cargo.exe"))) {
        $env:PATH = "$ownCargo;$env:PATH"
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "Rust isn't installed (there is no cargo). Get it from https://rustup.rs, open a new terminal and run this again."
    }
    $manifest = Get-Content "Cargo.toml" -Raw
    if ($manifest -notmatch '(?m)^\s*name\s*=\s*"([^"]+)"') { throw "Cargo.toml has no package name" }
    $name = $Matches[1]
    $crateName = $name -replace "-", "_"
    $library = "$crateName.dll"
    $portable = "$crateName.wasm"
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }

    if ($Native) {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "The build failed: the first error above says where. Nothing was installed. If it says link.exe was not found, install the Visual Studio C++ build tools."
        }
        $built = Join-Path $target "release\$library"
        $kept = $library
        $other = $portable
    }
    else {
        try { rustup target add wasm32-unknown-unknown *> $null } catch { }
        cargo build --release --target wasm32-unknown-unknown
        if ($LASTEXITCODE -ne 0) {
            throw "The portable build failed: the first error above says where. Nothing was installed. If it says the wasm32-unknown-unknown target may not be installed, run: rustup target add wasm32-unknown-unknown. If the error is inside a crate the tool depends on, that crate can't be built portable: add -Native."
        }
        $built = Join-Path $target "wasm32-unknown-unknown\release\$portable"
        $kept = $portable
        $other = $library
    }
    if (-not (Test-Path $built)) { throw "Couldn't find $built. Is crate-type set to cdylib?" }

    $data = Join-Path $env:APPDATA "notchbase"
    $fresh = -not $env:NOTCHBASE_TOOLS_DIR -and -not (Test-Path $data)
    $tools = if ($env:NOTCHBASE_TOOLS_DIR) { $env:NOTCHBASE_TOOLS_DIR } else { Join-Path $data "tools" }
    $folder = Join-Path $tools $name
    New-Item -ItemType Directory -Force $folder | Out-Null
    Remove-Item (Join-Path $folder $other) -Force -ErrorAction SilentlyContinue
    Copy-Item $built (Join-Path $folder $kept) -Force
    Write-Host "Installed $name into $folder as $kept"
    if ($fresh) {
        Write-Host "notchbase hasn't run on this computer yet. Start it and the tool is there."
    }
    else {
        Write-Host "notchbase picks it up within a second. Look for 'Reloaded $name' in the notch."
    }
}
finally {
    Pop-Location
}
