# Downloads the Actors packs (bodies, clothes, hair, clips) that people are
# built from into assets\packs. The version is in assets\actors-packs.version;
# nothing happens if that version is already there. The packs are large
# (~440 MB) and never committed. start.bat runs this.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $root 'assets\actors-packs.version') -Raw).Trim()
$dest = Join-Path $root 'assets\packs'
$stamp = Join-Path $dest '.version'
if ((Test-Path $stamp) -and ((Get-Content $stamp -Raw).Trim() -eq $version)) { exit 0 }
$url = "https://github.com/ArndRosemeier/Actors/releases/download/v$version/actors-packs-$version.tar.gz"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("hw-packs-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Write-Host "Downloading Actors packs $version (about 440 MB, once)..."
  $ProgressPreference = 'SilentlyContinue'
  Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp 'packs.tar.gz')
  New-Item -ItemType Directory -Path (Join-Path $tmp 'x') | Out-Null
  tar -xzf (Join-Path $tmp 'packs.tar.gz') -C (Join-Path $tmp 'x')
  if ($LASTEXITCODE -ne 0) { throw "tar failed" }
  if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
  Move-Item (Join-Path $tmp 'x\packs') $dest
  Set-Content -Path $stamp -Value $version
  Write-Host "Actors packs $version are in $dest"
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
