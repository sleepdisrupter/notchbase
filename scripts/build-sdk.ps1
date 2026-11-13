$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$stage = Join-Path ([System.IO.Path]::GetTempPath()) "notchbase-sdk-$PID"
$kit = Join-Path $stage "notchbase-tools"
$zip = Join-Path $root "Distributable\notchbase-sdk.zip"

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $kit | Out-Null

$sdk = New-Item -ItemType Directory -Force (Join-Path $kit "notchbase-sdk")
Copy-Item (Join-Path $root "sdk\Cargo.toml") $sdk
Copy-Item (Join-Path $root "sdk\src") $sdk -Recurse
Copy-Item (Join-Path $root "sdk\KIT.md") (Join-Path $kit "README.md")

$notes = New-Item -ItemType Directory -Force (Join-Path $kit "notes")
Copy-Item (Join-Path $root "tools\notes\src") $notes -Recurse
$manifest = (Get-Content (Join-Path $root "tools\notes\Cargo.toml") -Raw).Replace('path = "../../sdk"', 'path = "../notchbase-sdk"')
if ($manifest -notmatch 'path = "\.\./notchbase-sdk"') { throw "The notes example no longer depends on ../../sdk" }
[System.IO.File]::WriteAllText((Join-Path $notes "Cargo.toml"), $manifest.TrimEnd() + "`n`n[workspace]`n")

$docs = New-Item -ItemType Directory -Force (Join-Path $kit "docs")
Copy-Item (Join-Path $root "wiki\tools\*.md") $docs
Copy-Item (Join-Path $root "skill\notchbase\scripts\install.ps1") $kit
Copy-Item (Join-Path $root "skill\notchbase\scripts\install.sh") $kit

New-Item -ItemType Directory -Force (Split-Path $zip -Parent) | Out-Null
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::Open($zip, "Create")
try {
    foreach ($file in Get-ChildItem $kit -Recurse -File) {
        $entry = "notchbase-tools/" + $file.FullName.Substring($kit.Length + 1).Replace("\", "/")
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.FullName, $entry) | Out-Null
    }
}
finally {
    $archive.Dispose()
}
Remove-Item $stage -Recurse -Force
Write-Host "Built $zip"
