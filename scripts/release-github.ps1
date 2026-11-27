# Builds the Windows release and adds it to the GitHub release for this version: the app, the installer, and its
# entry in the latest.json that installed copies read. Needs the GitHub CLI (gh), logged in.
param(
    [string]$Notes = "",
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$official = "sleepdisrupter/notchbase"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Fail([string]$message) {
    Write-Host $message -ForegroundColor Red
    exit 1
}

# Runs a program for its exit code and output alone. Windows PowerShell turns anything a program writes to stderr
# into an error, which would stop the script, so stderr is dropped with errors allowed.
function Quietly([scriptblock]$run) {
    $saved = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $run 2>$null } finally { $ErrorActionPreference = $saved }
}

$platform = switch ($Target) {
    "x86_64-pc-windows-msvc" { "windows-x86_64" }
    "aarch64-pc-windows-msvc" { "windows-aarch64" }
    default { Fail "This script releases Windows targets only, not $Target." }
}

$repo = "$env:NOTCHBASE_RELEASES_REPO"
if (-not $repo) {
    $origin = Quietly { git remote get-url origin }
    if ($origin -match '^(https://github\.com/|git@github\.com:|ssh://git@github\.com/)([^/]+/[^/]+?)(\.git)?$') {
        $repo = $Matches[2]
    }
}
if (-not $repo) { $repo = $official }
# The app and the installer read their updates from this repository.
$env:NOTCHBASE_RELEASES_REPO = $repo

$version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$tag = "v$version"
$download = "https://github.com/$repo/releases/download/$tag"

if (-not $DryRun) {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        Fail "Install the GitHub CLI (https://cli.github.com) and run gh auth login first."
    }
    Quietly { gh auth status } | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail "Log in to GitHub first: gh auth login" }
}

Write-Host "Building notchbase $version for $platform, updating from $repo..." -ForegroundColor Cyan
rustup target add $Target | Out-Null
# notchbase dock is macOS only: the Windows build carries nothing of it.
cargo build --release --target $Target -p notchbase
if ($LASTEXITCODE -ne 0) { Fail "The build failed." }
cargo build --release --target $Target -p notchbase-setup
if ($LASTEXITCODE -ne 0) { Fail "The installer's build failed." }

$out = Join-Path $root "target\$Target\release"
$stage = Join-Path $root "target\github-release\$tag-$platform"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $stage | Out-Null
$name = "notchbase-$version-$platform.exe"
$file = Join-Path $stage $name
Copy-Item (Join-Path $out "notchbase.exe") $file
$setup = Join-Path $stage "notchbase-setup.exe"
Copy-Item (Join-Path $out "notchbase-setup.exe") $setup
$sha = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
$size = (Get-Item $file).Length

# A release that already exists (the Mac went first) keeps its other platforms in latest.json.
$existing = $false
$manifest = [ordered]@{ version = $version; notes = $Notes; builds = [ordered]@{} }
if (-not $DryRun) {
    Quietly { gh release view $tag --repo $repo } | Out-Null
    if ($LASTEXITCODE -eq 0) {
        $existing = $true
        $previous = Join-Path $stage "previous"
        Quietly { gh release download $tag --repo $repo --pattern latest.json --dir $previous } | Out-Null
        $old = Join-Path $previous "latest.json"
        if (Test-Path $old) {
            $parsed = Get-Content $old -Raw | ConvertFrom-Json
            if ($parsed.version -eq $version) {
                foreach ($build in $parsed.builds.PSObject.Properties) { $manifest.builds[$build.Name] = $build.Value }
                if (-not $Notes) { $manifest.notes = $parsed.notes }
            }
        }
    }
}
$manifest.builds[$platform] = [ordered]@{ url = "$download/$name"; sha256 = $sha; size = $size }
$latest = Join-Path $stage "latest.json"
[IO.File]::WriteAllText($latest, ($manifest | ConvertTo-Json -Depth 5) + "`n")

Write-Host ""
Write-Host "  release   $tag on $repo"
Write-Host "  file      $name ($([math]::Round($size / 1MB, 1)) MB)"
Write-Host "  sha256    $sha"
Write-Host "  also      notchbase-setup.exe, latest.json"
Write-Host ""
if ($DryRun) {
    Write-Host "Dry run: nothing was uploaded. The files are in $stage." -ForegroundColor Yellow
    exit 0
}

$assets = @($file, $setup, $latest)
if ($existing) {
    gh release upload $tag --repo $repo --clobber @assets
} else {
    $text = if ($Notes) { $Notes } else { "notchbase $version" }
    gh release create $tag --repo $repo --title "notchbase $version" --notes $text --latest @assets
}
if ($LASTEXITCODE -ne 0) { Fail "Uploading to GitHub failed." }
Write-Host "Published https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green
