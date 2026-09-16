# Run only on a disposable GitHub Windows runner: exercises the real installer.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (!$IsWindows -or $env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installation test on a disposable GitHub Actions Windows runner.'
}
Add-Type -Path (Join-Path $PSScriptRoot '../tests/native/windows_shortcut.cs')

$config = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$release = Join-Path $metadata.target_directory 'release'
$installer = Join-Path $release "bundle/nsis/$($config.productName)_$($config.version)_x64-setup.exe"
$install = Join-Path $env:LOCALAPPDATA 'Parley install test 中文'
$data = Join-Path $env:LOCALAPPDATA 'org.parley.desktop'
$shortcuts = Join-Path ([Environment]::GetFolderPath('Programs')) 'Parley'
$evidence = Join-Path $PWD 'artifacts/windows-install'
New-Item -ItemType Directory -Force $evidence | Out-Null
$payloadHashes = Get-Content (Join-Path $evidence 'payload-hashes.json') -Raw | ConvertFrom-Json -AsHashtable
if ((Test-Path $install) -or (Test-Path $data) -or (Test-Path $shortcuts)) {
    throw 'Refusing to touch an existing Parley installation or workspace.'
}

function Assert($Condition, [string]$Message) {
    if (!$Condition) { throw $Message }
}
function Run-Installer([string]$File, [string]$Arguments) {
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -PassThru
    if (!$process.WaitForExit(180000)) {
        Stop-Process -Id $process.Id -Force
        throw "Installer timed out: $File"
    }
    Assert ($process.ExitCode -eq 0) "Installer failed: $($process.ExitCode)"
}
function Assert-Payload {
    foreach ($name in @('parley.exe', 'parley-cli.exe')) {
        $actual = (Get-FileHash (Join-Path $install $name) -Algorithm SHA256).Hash
        $expected = $payloadHashes[$name]
        Assert ($actual -eq $expected) "Installed payload mismatch: $name"
    }
    Write-Host 'Installed payload hashes match.'
    $shortcutDetails = foreach ($root in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('CommonPrograms'))) {
        Get-ChildItem $root -Filter '*Parley*.lnk' -Recurse | ForEach-Object {
            $link = [ParleyValidation.Shortcut]::Read($_.FullName)
            @{ path = $_.FullName; target = $link.TargetPath; arguments = $link.Arguments }
        }
    }
    $shortcutDetails | ConvertTo-Json -Depth 3 | Tee-Object -FilePath (Join-Path $evidence 'shortcuts.json') | Write-Host
    $main = [ParleyValidation.Shortcut]::Read((Join-Path $shortcuts 'Parley.lnk'))
    Assert ($main.TargetPath -eq (Join-Path $install 'parley.exe')) "GUI shortcut target '$($main.TargetPath)' differs from '$install\parley.exe'"
    $tutor = [ParleyValidation.Shortcut]::Read((Join-Path $shortcuts 'Parley Tutor.lnk'))
    Assert ($tutor.TargetPath -eq $main.TargetPath -and $tutor.Arguments -eq '--tutor-only') 'Tutor shortcut wrong'
    $terminal = [ParleyValidation.Shortcut]::Read((Join-Path $shortcuts 'Parley Terminal.lnk'))
    Assert ($terminal.TargetPath -eq "$env:SystemRoot\System32\cmd.exe") 'Terminal shortcut target wrong'
    Assert ($terminal.Arguments -eq "/d /k `"`"$install\parley-cli.exe`"`"") 'Terminal shortcut quoting wrong'
}

function Wait-Gui {
    $deadline = (Get-Date).AddSeconds(60)
    do {
        $process = Get-Process -Name parley -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq (Join-Path $install 'parley.exe') } |
            Select-Object -First 1
        if ($process -and $process.MainWindowHandle -ne 0) { return $process }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    throw 'Installed GUI did not open a native window'
}

$gui = $null
try {
    Run-Installer $installer "/S /D=$install"
    Assert-Payload
    $cli = Join-Path $install 'parley-cli.exe'
    $help = & $cli --help
    Assert ($LASTEXITCODE -eq 0 -and ($help -join "`n").Contains('--reconnect')) 'Installed CLI help failed'
    $shimDirectory = Join-Path $evidence 'npm shim 中文'
    New-Item -ItemType Directory -Force $shimDirectory | Out-Null
    $unixShim = Join-Path $shimDirectory 'codex'
    Set-Content $unixShim "#!/bin/sh`nexit 1"
    Set-Content (Join-Path $shimDirectory 'codex.cmd') "@echo off`r`necho codex-cli fixture"
    $shimVersion = & $cli --no-gui --codex $unixShim -- --version
    Assert ($LASTEXITCODE -eq 0 -and ($shimVersion -join "`n").Contains('codex-cli fixture')) 'Installed launcher did not resolve the Windows npm shim'
    & $cli --no-gui --codex "$env:SystemRoot\System32\cmd.exe" -- /d /c exit 23
    Assert ($LASTEXITCODE -eq 23) 'Native argument forwarding / exit status failed'

    # where.exe is a harmless native stand-in. No model credentials or API calls.
    # Omitting --gui-bin proves the installed CLI finds its adjacent GUI itself.
    & $cli --codex "$env:SystemRoot\System32\where.exe" -- cmd.exe
    Assert ($LASTEXITCODE -eq 0) 'CLI companion launch failed'
    $gui = Wait-Gui
    # Native window creation precedes the frontend's first storage_load IPC.
    $database = Join-Path $data 'parley.sqlite3'
    $deadline = (Get-Date).AddSeconds(30)
    do {
        if ((Test-Path $database) -and (Get-Item $database).Length -gt 0) { break }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)
    Assert ((Test-Path $database) -and (Get-Item $database).Length -gt 0) 'GUI did not initialize the workspace'
    $gui.CloseMainWindow() | Out-Null
    Assert ($gui.WaitForExit(15000)) 'GUI did not close normally'
    $gui = $null
    # Also ask Windows itself to resolve and launch the installed Unicode link.
    Start-Process -FilePath (Join-Path $shortcuts 'Parley Tutor.lnk')
    $gui = Wait-Gui
    $gui.CloseMainWindow() | Out-Null
    Assert ($gui.WaitForExit(15000)) 'Shortcut-launched GUI did not close normally'
    $gui = $null
    Assert (Test-Path (Join-Path $data 'parley.sqlite3')) 'GUI did not initialize the workspace'
    $databaseHash = (Get-FileHash (Join-Path $data 'parley.sqlite3')).Hash
    $sentinel = Join-Path $data 'installer-preservation.txt'
    Set-Content $sentinel 'Keep existing learning data' -NoNewline

    # First Windows release: no older published Windows installer exists yet.
    # Test same-version replacement, without claiming a version-to-version migration.
    Run-Installer $installer "/S /D=$install"
    Assert-Payload
    Assert ((Get-FileHash (Join-Path $data 'parley.sqlite3')).Hash -eq $databaseHash) 'Reinstall changed the workspace'
    Assert ((Get-Content $sentinel -Raw) -eq 'Keep existing learning data') 'Reinstall removed user data'

    Run-Installer (Join-Path $install 'uninstall.exe') "/S _?=$install"
    foreach ($name in @('parley.exe', 'parley-cli.exe')) {
        Assert (!(Test-Path (Join-Path $install $name))) "Uninstall left $name behind"
    }
    foreach ($name in @('Parley.lnk', 'Parley Terminal.lnk', 'Parley Tutor.lnk')) {
        Assert (!(Test-Path (Join-Path $shortcuts $name))) "Uninstall left $name behind"
    }
    Assert ((Get-FileHash (Join-Path $data 'parley.sqlite3')).Hash -eq $databaseHash) 'Uninstall changed the workspace'
    Assert (Test-Path $sentinel) 'Default uninstall removed user data'
    @{
        version = $config.version
        runner = $env:ImageOS
        commit = $env:GITHUB_SHA
        freshInstall = $true
        payloadHashes = $true
        shortcuts = $true
        nativeExitStatus = $true
        npmShimSelection = $true
        siblingGuiWindow = $true
        shortcutGuiWindow = $true
        sameVersionReinstall = $true
        uninstall = $true
        workspacePreserved = $true
        realModelCalls = $false
    } | ConvertTo-Json | Tee-Object -FilePath (Join-Path $evidence 'result.json')
} catch {
    ($_ | Out-String) + $_.ScriptStackTrace | Set-Content (Join-Path $evidence 'failure.txt')
    throw
} finally {
    if ($gui -and !$gui.HasExited) { Stop-Process -Id $gui.Id -Force }
}
