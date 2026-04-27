param(
  [Parameter(Mandatory = $true)]
  [string]$Version,

  [string]$SigningKeyPath = "$env:USERPROFILE\.tauri\derek-mouse-updater.key",

  [string]$SigningKeyPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD,

  [switch]$SkipBuild,

  [switch]$AllowDirty
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Info($Message) {
  Write-Host "[release] $Message" -ForegroundColor Cyan
}

function Write-Success($Message) {
  Write-Host "[release] $Message" -ForegroundColor Green
}

function Assert-Semver($Value) {
  if ($Value -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version must use x.y.z format, for example 0.1.4."
  }
}

function Replace-FirstMatch {
  param(
    [string]$Text,
    [string]$Pattern,
    [string]$Replacement
  )

  $Regex = [regex]::new($Pattern, [System.Text.RegularExpressions.RegexOptions]::Multiline -bor [System.Text.RegularExpressions.RegexOptions]::Singleline)
  if (-not $Regex.IsMatch($Text)) {
    throw "Could not find content to replace. Pattern: $Pattern"
  }
  return $Regex.Replace($Text, $Replacement, 1)
}

function Set-CargoPackageVersion {
  param(
    [string]$Text,
    [string]$Version
  )

  $Lines = [regex]::Split($Text, "`r?`n")
  $InPackage = $false
  for ($i = 0; $i -lt $Lines.Length; $i++) {
    if ($Lines[$i] -eq "[package]") {
      $InPackage = $true
      continue
    }

    if ($InPackage -and $Lines[$i] -match '^\[') {
      break
    }

    if ($InPackage -and $Lines[$i] -match '^version\s*=\s*"[^"]+"') {
      $Lines[$i] = 'version = "{0}"' -f $Version
      return ($Lines -join "`r`n")
    }
  }

  throw "Could not find package version in Cargo.toml."
}

function Write-Utf8NoBomFile {
  param(
    [string]$Path,
    [string]$Content
  )

  $Encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($Path, $Content, $Encoding)
}

Assert-Semver $Version

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$PackageJsonPath = Join-Path $ProjectRoot "package.json"
$CargoTomlPath = Join-Path $ProjectRoot "src-tauri\Cargo.toml"
$TauriConfigPath = Join-Path $ProjectRoot "src-tauri\tauri.conf.json"

Push-Location $ProjectRoot
try {
  if (-not $AllowDirty) {
    $Dirty = git status --short
    if ($Dirty) {
      throw "Working tree is dirty. Commit changes first or use -AllowDirty."
    }
  }

  $PackageJson = Get-Content -Raw -Path $PackageJsonPath
  $PackageJson = Replace-FirstMatch $PackageJson '"version"\s*:\s*"[^"]+"' ('"version": "{0}"' -f $Version)
  Write-Utf8NoBomFile $PackageJsonPath $PackageJson

  $CargoToml = Get-Content -Raw -Path $CargoTomlPath
  $CargoToml = Set-CargoPackageVersion -Text $CargoToml -Version $Version
  Write-Utf8NoBomFile $CargoTomlPath $CargoToml

  $TauriConfig = Get-Content -Raw -Path $TauriConfigPath
  $TauriConfig = Replace-FirstMatch $TauriConfig '"version"\s*:\s*"[^"]+"' ('"version": "{0}"' -f $Version)
  Write-Utf8NoBomFile $TauriConfigPath $TauriConfig

  Write-Success "Synced version to package.json, src-tauri/Cargo.toml, and src-tauri/tauri.conf.json -> $Version"
  Write-Info "Suggested Git tag: v$Version"

  if ($SkipBuild) {
    Write-Info "Build skipped."
    return
  }

  if (-not (Test-Path -LiteralPath $SigningKeyPath)) {
    throw "Updater signing key not found: $SigningKeyPath"
  }

  $SigningKeyContent = Get-Content -Raw -LiteralPath $SigningKeyPath
  $env:TAURI_SIGNING_PRIVATE_KEY = $SigningKeyContent.Trim()
  $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $SigningKeyPath
  if ($SigningKeyPassword) {
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $SigningKeyPassword
  }

  Write-Info "Running npm run tauri:build"
  npm run tauri:build
  if ($LASTEXITCODE -ne 0) {
    throw "tauri build failed with exit code: $LASTEXITCODE"
  }

  $BundleDir = Join-Path $ProjectRoot "src-tauri\target\release\bundle"
  $Assets = @()
  if (Test-Path -LiteralPath $BundleDir) {
    $Assets += Get-ChildItem -Path $BundleDir -Recurse -File |
      Where-Object {
        $_.Name -eq "latest.json" -or
        $_.Name -like "*.exe" -or
        $_.Name -like "*.exe.sig" -or
        $_.Name -like "*.msi" -or
        $_.Name -like "*.msi.sig"
      } |
      Sort-Object FullName
  }

  Write-Success "Build finished. Suggested assets to upload to GitHub Release:"
  foreach ($Asset in $Assets) {
    Write-Host (" - " + $Asset.FullName)
  }
}
finally {
  Pop-Location
}
