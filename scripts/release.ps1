param(
  [Parameter(Mandatory = $true)]
  [string]$Version,

  [string]$SigningKeyPath = "$env:USERPROFILE\.tauri\derek-mouse-updater.key",

  [string]$SigningKeyPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD,

  [string]$Repo = "derekLee999/derek-mouse",

  [string]$ReleaseNotes = "",

  [string]$NotesFile = "",

  [switch]$SkipBuild,

  [switch]$Publish,

  [switch]$Draft,

  [switch]$Prerelease,

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

function Get-ReleaseNotesText {
  param(
    [string]$InlineNotes,
    [string]$NotesFilePath,
    [string]$Version
  )

  if ($NotesFilePath) {
    if (-not (Test-Path -LiteralPath $NotesFilePath)) {
      throw "Notes file not found: $NotesFilePath"
    }
    return (Get-Content -Raw -LiteralPath $NotesFilePath).Trim()
  }

  if ($InlineNotes) {
    return $InlineNotes.Trim()
  }

  return "Release v$Version"
}

function Get-SetupExe {
  param(
    [string]$NsisDir,
    [string]$Version
  )

  $Matches = Get-ChildItem -Path $NsisDir -File -Filter "*_${Version}_x64-setup.exe" | Sort-Object Name
  if (-not $Matches) {
    throw "Could not find setup exe for version $Version in $NsisDir"
  }
  return $Matches[0]
}

function New-LatestJson {
  param(
    [string]$NsisDir,
    [string]$Version,
    [string]$Repo,
    [string]$NotesText
  )

  $SetupExe = Get-SetupExe -NsisDir $NsisDir -Version $Version
  $SigPath = "$($SetupExe.FullName).sig"
  if (-not (Test-Path -LiteralPath $SigPath)) {
    throw "Could not find updater signature file: $SigPath"
  }

  $Signature = (Get-Content -Raw -LiteralPath $SigPath).Trim()
  $Tag = "v$Version"
  $LatestJsonPath = Join-Path $NsisDir "latest.json"
  $Json = [ordered]@{
    version  = $Version
    notes    = $NotesText
    pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = @{
      "windows-x86_64" = @{
        url       = "https://github.com/$Repo/releases/download/$Tag/$($SetupExe.Name)"
        signature = $Signature
      }
    }
  } | ConvertTo-Json -Depth 6

  Write-Utf8NoBomFile -Path $LatestJsonPath -Content $Json
  return [pscustomobject]@{
    SetupExePath   = $SetupExe.FullName
    SetupSigPath   = $SigPath
    LatestJsonPath = $LatestJsonPath
  }
}

function Ensure-GhAvailable {
  $Gh = Get-Command gh -ErrorAction SilentlyContinue
  if (-not $Gh) {
    throw "GitHub CLI (gh) is not installed or not on PATH."
  }
}

Assert-Semver $Version

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$PackageJsonPath = Join-Path $ProjectRoot "package.json"
$CargoTomlPath = Join-Path $ProjectRoot "src-tauri\Cargo.toml"
$TauriConfigPath = Join-Path $ProjectRoot "src-tauri\tauri.conf.json"
$BundleDir = Join-Path $ProjectRoot "src-tauri\target\release\bundle"
$NsisDir = Join-Path $BundleDir "nsis"
$Tag = "v$Version"

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
  Write-Info "Suggested Git tag: $Tag"

  if (-not $SkipBuild) {
    if (-not (Test-Path -LiteralPath $SigningKeyPath)) {
      throw "Updater signing key not found: $SigningKeyPath"
    }

    $SigningKeyContent = Get-Content -Raw -LiteralPath $SigningKeyPath
    $env:TAURI_SIGNING_PRIVATE_KEY = $SigningKeyContent.Trim()
    $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $SigningKeyPath
    if ($SigningKeyPassword -ne $null) {
      $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $SigningKeyPassword
    }

    Write-Info "Running npm run tauri:build"
    npm run tauri:build
    if ($LASTEXITCODE -ne 0) {
      throw "tauri build failed with exit code: $LASTEXITCODE"
    }
  }
  else {
    Write-Info "Build skipped."
  }

  if (-not (Test-Path -LiteralPath $NsisDir)) {
    throw "NSIS bundle directory not found: $NsisDir"
  }

  $NotesText = Get-ReleaseNotesText -InlineNotes $ReleaseNotes -NotesFilePath $NotesFile -Version $Version
  $Artifacts = New-LatestJson -NsisDir $NsisDir -Version $Version -Repo $Repo -NotesText $NotesText

  Write-Success "Generated latest.json:"
  Write-Host (" - " + $Artifacts.LatestJsonPath)
  Write-Success "Suggested assets to upload to GitHub Release:"
  Write-Host (" - " + $Artifacts.SetupExePath)
  Write-Host (" - " + $Artifacts.SetupSigPath)
  Write-Host (" - " + $Artifacts.LatestJsonPath)

  if ($Publish) {
    Ensure-GhAvailable

    $RemoteTag = git ls-remote --tags origin $Tag
    if (-not $RemoteTag) {
      throw "Remote tag $Tag was not found. Push the tag before using -Publish."
    }

    $NotesTempPath = Join-Path $ProjectRoot ".codex-run\release-$Tag-notes.md"
    New-Item -ItemType Directory -Force -Path (Split-Path $NotesTempPath) | Out-Null
    Write-Utf8NoBomFile -Path $NotesTempPath -Content $NotesText

    $Arguments = @(
      "release", "create", $Tag,
      $Artifacts.SetupExePath,
      $Artifacts.SetupSigPath,
      $Artifacts.LatestJsonPath,
      "--repo", $Repo,
      "--title", $Tag,
      "--notes-file", $NotesTempPath,
      "--verify-tag"
    )
    if ($Draft) {
      $Arguments += "--draft"
    }
    if ($Prerelease) {
      $Arguments += "--prerelease"
    }

    Write-Info "Running gh $($Arguments -join ' ')"
    & gh @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "gh release create failed with exit code: $LASTEXITCODE"
    }

    Write-Success "GitHub Release created for $Tag"
  }
}
finally {
  Pop-Location
}
