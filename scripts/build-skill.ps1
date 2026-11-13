$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$stage = Join-Path ([System.IO.Path]::GetTempPath()) "notchbase-skill-$PID"
$skill = Join-Path $stage "notchbase"
$zip = Join-Path $root "Distributable\notchbase-skill.zip"

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item (Join-Path $root "skill\notchbase") $skill -Recurse
$references = New-Item -ItemType Directory -Force (Join-Path $skill "references")
$sdk = New-Item -ItemType Directory -Force (Join-Path $references "tools")
Copy-Item (Join-Path $root "wiki\*.md") $references
Copy-Item (Join-Path $root "wiki\tools\*.md") $sdk

$notes = Join-Path $root "tools\notes"
$example = @("# Example: the Notes plugin", "", "A complete plugin from the notchbase repository (tools/notes): a page with a text field and a list, pinning a note as a live activity, a folder setting shown only while its toggle is on, reading the notch state, holding the notch open, writing a file in the background, and full Harness tests.", "")
foreach ($file in @("Cargo.toml") + (Get-ChildItem (Join-Path $notes "src") -Filter *.rs | Sort-Object Name | ForEach-Object { "src/" + $_.Name })) {
    $language = if ($file.EndsWith(".toml")) { "toml" } else { "rust" }
    $example += "## $file", "", ('```' + $language), (Get-Content (Join-Path $notes $file) -Raw).TrimEnd(), '```', ""
}
[System.IO.File]::WriteAllText((Join-Path $sdk "example-notes.md"), ($example -join "`n"))

New-Item -ItemType Directory -Force (Split-Path $zip -Parent) | Out-Null
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::Open($zip, "Create")
try {
    foreach ($file in Get-ChildItem $skill -Recurse -File) {
        $entry = "notchbase/" + $file.FullName.Substring($skill.Length + 1).Replace("\", "/")
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.FullName, $entry) | Out-Null
    }
}
finally {
    $archive.Dispose()
}
Remove-Item $stage -Recurse -Force
Write-Host "Built $zip"
