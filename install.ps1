<#
.SYNOPSIS
  One-click installer for the Antigravity 2.0 Git Widget sidecar plugin.
.DESCRIPTION
  Builds (if cargo is installed and binary is missing) or copies the release binary
  into %USERPROFILE%\.gemini\config\plugins\git-widget and configures sidecar.json
  with the current user's absolute path.
#>
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$PluginDir = Join-Path $env:USERPROFILE ".gemini\config\plugins\git-widget"
$AssetsDir = Join-Path $PluginDir "assets"
$SidecarDir = Join-Path $PluginDir "sidecars\git_widget"
$TargetExe = Join-Path $SidecarDir "git-widget-sidecar.exe"

$LocalExe = Join-Path $RepoRoot "git-widget-sidecar.exe"
$TargetBuildExe = Join-Path $RepoRoot "target\release\git-widget-sidecar.exe"
$ReleaseExe = if (Test-Path $LocalExe) { $LocalExe } else { $TargetBuildExe }

if (-not (Test-Path $ReleaseExe)) {
    Write-Host "[git-widget] Building release binary..." -ForegroundColor Cyan
    Push-Location (Join-Path $RepoRoot "ui")
    npm install
    npm run build
    Pop-Location
    Push-Location $RepoRoot
    cargo build --release --workspace
    Pop-Location
    $ReleaseExe = $TargetBuildExe
}

New-Item -ItemType Directory -Force -Path $AssetsDir | Out-Null
New-Item -ItemType Directory -Force -Path $SidecarDir | Out-Null

Stop-Process -Name "git-widget-sidecar" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

Copy-Item -Force $ReleaseExe $TargetExe

$LogoSvg = @'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f05032" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="6" cy="6" r="2.5"/>
  <circle cx="6" cy="18" r="2.5"/>
  <circle cx="18" cy="8" r="2.5"/>
  <path d="M6 8.5v7"/>
  <path d="M18 10.5c0 4-6 3.5-12 5"/>
</svg>
'@
function Write-Utf8NoBom($Path, $Content) {
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

Write-Utf8NoBom (Join-Path $AssetsDir "logo.svg") $LogoSvg

$PluginJson = @{
    name        = "git-widget"
    displayName = "Git Widget"
    description = "Native VS Code-style Git Source Control & Diff Review for Antigravity AUX_PANE"
    logo        = "assets/logo.svg"
} | ConvertTo-Json -Depth 4
Write-Utf8NoBom (Join-Path $PluginDir "plugin.json") $PluginJson

$SidecarJson = @{
    command           = $TargetExe
    args              = @()
    restart_policy    = "always"
    display_name      = "Git Widget"
    description       = "Antigravity AUX_PANE Git workflow sidecar"
    has_web_ui        = $true
    agent_permissions = @{
        allows_user_interactions = $true
    }
    ui_config         = @{
        display_name = "Git Widget"
        views        = @(
            @{
                path       = "/"
                entrypoint = "SIDECAR_UI_ENTRYPOINT_AUX_PANE"
                title      = "Git Widget"
            }
        )
    }
} | ConvertTo-Json -Depth 6
Write-Utf8NoBom (Join-Path $SidecarDir "sidecar.json") $SidecarJson

Write-Host "[git-widget] Installed successfully to $PluginDir" -ForegroundColor Green
Write-Host "Restart Antigravity 2.0 or open the Git Widget tab in the right auxiliary pane."
