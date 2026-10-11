$ErrorActionPreference = "Stop"
$pfx = $env:SIGN_PFX
if (-not $pfx) {
  Write-Host "SIGN_PFX not set; skip Authenticode."
  exit 0
}
$pwd = $env:SIGN_PASSWORD
$timestamp = "http://timestamp.digicert.com"
$sha1ts = "http://timestamp.digicert.com"
$root = Split-Path -Parent $PSScriptRoot
if (-not $root) { $root = (Get-Location).Path }
$msis = Get-ChildItem -Path $root -Recurse -Filter *.msi -ErrorAction SilentlyContinue
$exes = @()
$exes += Get-ChildItem -Path (Join-Path $root "target") -Recurse -Include ftracker.exe,ftracker-svc.exe -ErrorAction SilentlyContinue
$files = @($msis) + @($exes)
if ($files.Count -eq 0) {
  Write-Host "No MSI/exe found to sign."
  exit 0
}
foreach ($f in $files) {
  $signtool = "signtool"
  $args256 = @("sign", "/fd", "SHA256", "/td", "SHA256", "/tr", $timestamp, "/f", $pfx)
  if ($pwd) { $args256 += @("/p", $pwd) }
  $args256 += $f.FullName
  & $signtool @args256
  # Dual-sign SHA-1 for Windows 7 without the SHA-256 hotfix when possible.
  $args1 = @("sign", "/as", "/fd", "SHA1", "/td", "SHA1", "/tr", $sha1ts, "/f", $pfx)
  if ($pwd) { $args1 += @("/p", $pwd) }
  $args1 += $f.FullName
  try { & $signtool @args1 } catch { Write-Host "SHA-1 dual-sign skipped for $($f.Name)" }
}
