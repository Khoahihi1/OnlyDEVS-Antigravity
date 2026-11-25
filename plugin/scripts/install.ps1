# Install antigravity-filter-bypass into Antigravity plugin directory.
param(
    [string]$Dest = "",
    [string]$PythonExe = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$PyArgs = @()

if (-not $Dest) {
    $Dest = Join-Path $env:USERPROFILE ".gemini\config\plugins\antigravity-filter-bypass"
}

if (-not $PythonExe) {
    if (Get-Command python -ErrorAction SilentlyContinue) {
        $PythonExe = (Get-Command python).Source
    } elseif (Test-Path "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe") {
        $PythonExe = "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe"
    } elseif (Get-Command py -ErrorAction SilentlyContinue) {
        $PythonExe = (Get-Command py).Source
        $PyArgs = @("-3.12")
    } else {
        throw "Python not found. Install Python 3.12+ or pass -PythonExe."
    }
}

New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$files = @(
    "filter_bypass.py",
    "hooks.json",
    "plugin.json",
    "test_filter_bypass.py",
    "README.md",
    "CHANGELOG.md"
)
foreach ($f in $files) {
    $src = Join-Path $Root $f
    if (Test-Path $src) {
        Copy-Item -Force $src (Join-Path $Dest $f)
    }
}

$hooksPath = Join-Path $Dest "hooks.json"
$hooks = Get-Content $hooksPath -Raw | ConvertFrom-Json
$pluginKey = "antigravity-filter-bypass"
foreach ($event in @("SessionStart", "PreInvocation", "PostInvocation", "Stop")) {
    $handlers = $hooks.$pluginKey.$event
    if (-not $handlers) { continue }
    foreach ($handler in $handlers) {
        $phase = switch ($event) {
            "SessionStart" { "session" }
            "PreInvocation" { "pre" }
            "PostInvocation" { "post" }
            "Stop" { "stop" }
        }
        $prefix = "`"$PythonExe`""
        if ($PyArgs.Count -gt 0) {
            $prefix = "`"$PythonExe`" $($PyArgs -join ' ')"
        }
        $handler.command = "$prefix ./filter_bypass.py $phase"
        $handler.commandWindows = "$prefix .\filter_bypass.py $phase"
    }
}
$hooks | ConvertTo-Json -Depth 10 | Set-Content $hooksPath -Encoding utf8

Write-Host "Installed to: $Dest"
Write-Host "Python: $PythonExe"
& $PythonExe @PyArgs (Join-Path $Dest "test_filter_bypass.py")