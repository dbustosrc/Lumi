#requires -Version 7.0
[CmdletBinding(SupportsShouldProcess)]
param(
    [ValidateScript({ $_ -gt 0 })]
    [int]$Jobs = 2,
    [switch]$Installer
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows) { throw 'Lumi requiere Windows para generar y ejecutar esta build.' }

$projectRoot = [IO.Path]::GetFullPath($PSScriptRoot)

function Assert-OwnedPath([string]$Root, [string]$Path) {
    $base = [IO.Path]::GetFullPath($Root).TrimEnd([IO.Path]::DirectorySeparatorChar)
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($base + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Ruta fuera del directorio permitido: $full"
    }
    $baseEntry = Get-Item -LiteralPath $base -Force -ErrorAction SilentlyContinue
    if ($null -ne $baseEntry -and ($baseEntry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "El directorio permitido no puede ser un enlace o junction: $base"
    }
    $cursor = $full
    while ($cursor -ne $base) {
        $entry = Get-Item -LiteralPath $cursor -Force -ErrorAction SilentlyContinue
        if ($null -ne $entry -and ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "No se permiten enlaces o junctions en rutas de build: $cursor"
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
    return $full
}

function Get-LumiInstances {
    $prefix = $projectRoot + [IO.Path]::DirectorySeparatorChar
    Get-CimInstance Win32_Process -Filter "Name = 'lumi.exe' OR Name LIKE 'lumi-running-%.exe'" |
        Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) } |
        Sort-Object CreationDate -Descending
}

function Remove-GeneratedPath([string]$Root, [string]$Path) {
    $full = Assert-OwnedPath $Root $Path
    if (-not (Test-Path -LiteralPath $full)) { return }
    $entry = Get-Item -LiteralPath $full -Force
    if ($entry.PSIsContainer) {
        $links = @(Get-ChildItem -LiteralPath $full -Recurse -Force |
            Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })
        if ($links.Count -gt 0) { throw "La limpieza no seguirá enlaces dentro de: $full" }
    }
    Remove-Item -LiteralPath $full -Recurse -Force -ErrorAction Stop
}

function Clear-GeneratedDirectory([string]$Path, [string[]]$Keep) {
    $directory = Assert-OwnedPath $projectRoot $Path
    foreach ($entry in Get-ChildItem -LiteralPath $directory -Force) {
        if ($entry.FullName -notin $Keep) { Remove-GeneratedPath $projectRoot $entry.FullName }
    }
}

$manifest = Join-Path $projectRoot 'Cargo.toml'
if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) { throw "No existe $manifest" }
$cargo = (Get-Command cargo -CommandType Application -ErrorAction Stop).Source
$target = Assert-OwnedPath $projectRoot (Join-Path $projectRoot 'target')
$release = Assert-OwnedPath $projectRoot (Join-Path $target 'release')
$executable = Assert-OwnedPath $projectRoot (Join-Path $release 'lumi.exe')
$work = Assert-OwnedPath $projectRoot (Join-Path $projectRoot '.build')
$stage = Assert-OwnedPath $projectRoot (Join-Path $work 'staging')
$backup = Assert-OwnedPath $projectRoot (Join-Path $work 'previous.exe')
$temporary = Assert-OwnedPath $projectRoot (Join-Path $projectRoot 'tmp')
$lockPath = Assert-OwnedPath $projectRoot (Join-Path $work 'build.lock')
if ([string]::IsNullOrWhiteSpace($env:APPDATA)) { throw 'APPDATA no está definido.' }
$data = Join-Path $env:APPDATA 'Lumi'
$logs = Assert-OwnedPath $data (Join-Path $data 'logs')
$action = "Compilar Lumi y limpiar los archivos generados y logs"
if ($Installer) { $action += ', generar el instalador y reemplazar las distribuciones anteriores de dist' }
if (-not $PSCmdlet.ShouldProcess(
    $projectRoot,
    $action
)) { return }
$installerCompiler = $null
if ($Installer) {
    $installerCompiler = Get-Command ISCC.exe -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty Source -First 1
    if (-not $installerCompiler) {
        $installerCompiler = @(
            $env:ProgramFiles
            [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
            (Join-Path $env:LOCALAPPDATA 'Programs')
        ) | Where-Object { $_ } | ForEach-Object { Join-Path $_ 'Inno Setup 7/ISCC.exe' } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    }
    if (-not $installerCompiler) { throw 'Instala Inno Setup 7 para generar el instalador.' }
}
New-Item -ItemType Directory -Path $work -Force | Out-Null

# El bloqueo se elimina al cerrar el handle, también si el proceso es terminado.
$lock = [IO.FileStream]::new(
    $lockPath, [IO.FileMode]::OpenOrCreate,
    [IO.FileAccess]::ReadWrite, [IO.FileShare]::None, 1, [IO.FileOptions]::DeleteOnClose
)
$failure = $null
$published = $false
$resumePath = $null

try {
    Remove-GeneratedPath $projectRoot $stage
    $running = @(Get-LumiInstances)
    foreach ($instance in $running) {
        $process = Get-Process -Id $instance.ProcessId -ErrorAction SilentlyContinue
        if ($null -eq $process -or $process.Path -ne $instance.ExecutablePath) { continue }
        $stoppedPath = $process.Path
        if ($null -eq $resumePath) { $resumePath = $stoppedPath }
        Write-Host "Deteniendo Lumi (PID $($process.Id))..."
        Stop-Process -InputObject $process -Force -ErrorAction Stop
        if (-not $process.WaitForExit(15000)) { throw "Lumi no terminó: PID $($process.Id)." }
    }

    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    Write-Host "Compilando Lumi en release desde $projectRoot..."
    $artifacts = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $cargoTemporary = Join-Path $stage '.tmp'
    New-Item -ItemType Directory -Path $cargoTemporary -Force | Out-Null
    $previousTemp = [Environment]::GetEnvironmentVariable('TEMP', 'Process')
    $previousTmp = [Environment]::GetEnvironmentVariable('TMP', 'Process')
    Push-Location -LiteralPath $projectRoot
    try {
        [Environment]::SetEnvironmentVariable('TEMP', $cargoTemporary, 'Process')
        [Environment]::SetEnvironmentVariable('TMP', $cargoTemporary, 'Process')
        & $cargo build --manifest-path $manifest --locked --release --bin lumi --jobs $Jobs --target-dir $stage --message-format=json-render-diagnostics |
            ForEach-Object {
                $message = $_ | ConvertFrom-Json -ErrorAction Stop
                if ($message.reason -eq 'compiler-artifact' -and $message.target.name -eq 'lumi' -and
                    $message.target.kind -contains 'bin' -and $message.executable) {
                    [void]$artifacts.Add([string]$message.executable)
                }
            }
        $cargoExit = $LASTEXITCODE
    }
    finally {
        [Environment]::SetEnvironmentVariable('TEMP', $previousTemp, 'Process')
        [Environment]::SetEnvironmentVariable('TMP', $previousTmp, 'Process')
        Pop-Location
    }
    if ($cargoExit -ne 0) { throw "Cargo falló con código $cargoExit. La build anterior se conserva." }
    if ($artifacts.Count -ne 1) { throw 'Cargo no entregó exactamente un ejecutable Lumi.' }
    $built = Assert-OwnedPath $stage (@($artifacts)[0])
    $binary = Get-Item -LiteralPath $built -ErrorAction Stop
    if ($binary.PSIsContainer -or $binary.Length -eq 0) { throw 'El ejecutable generado no es válido.' }

    if ($Installer) {
        $installerStage = Assert-OwnedPath $stage (Join-Path $stage 'installer')
        $dist = Assert-OwnedPath $projectRoot (Join-Path $projectRoot 'dist')
        $metadata = [Diagnostics.FileVersionInfo]::GetVersionInfo($built)
        if (-not $metadata.ProductVersion -or -not $metadata.CompanyName) {
            throw 'El ejecutable debe incluir versión y autor para el instalador.'
        }
        $installerName = "Lumi-$($metadata.ProductVersion)-windows-x64-setup.exe"
        $stagedInstaller = Assert-OwnedPath $stage (Join-Path $installerStage $installerName)
        $publishedInstaller = Assert-OwnedPath $projectRoot (Join-Path $dist $installerName)
        New-Item -ItemType Directory -Path $installerStage,$dist -Force | Out-Null
        & $installerCompiler '/Qp' "/O$installerStage" "/DLumiBinary=$built" "/DProjectDir=$projectRoot" `
            "/DLumiVersion=$($metadata.ProductVersion)" "/DLumiAuthor=$($metadata.CompanyName)" `
            (Join-Path $projectRoot 'installer/Lumi.iss')
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $stagedInstaller -PathType Leaf)) {
            throw 'No se pudo generar el instalador. La build anterior se conserva.'
        }
    }

    New-Item -ItemType Directory -Path $release -Force | Out-Null
    $hadExecutable = Test-Path -LiteralPath $executable -PathType Leaf
    if ($hadExecutable) {
        Remove-GeneratedPath $projectRoot $backup
        [IO.File]::Replace($built, $executable, $backup)
    } else {
        [IO.File]::Move($built, $executable)
    }
    if ($Installer) {
        try {
            [IO.File]::Move($stagedInstaller, $publishedInstaller, $true)
        }
        catch {
            if ($hadExecutable) {
                [IO.File]::Move($backup, $executable, $true)
            } else {
                Remove-GeneratedPath $projectRoot $executable
            }
            throw
        }
        Write-Host "Instalador listo: $publishedInstaller"
    }
    $published = $true

    Write-Host 'Build generada. Eliminando artefactos anteriores y temporales...'
    Clear-GeneratedDirectory $target @($release)
    Clear-GeneratedDirectory $release @($executable)
    if ($Installer) { Clear-GeneratedDirectory $dist @($publishedInstaller) }
    Clear-GeneratedDirectory $work @($stage, $backup, $lockPath)
    Remove-GeneratedPath $data $logs
}
catch {
    $failure = $_
}
finally {
    try {
        # El ejecutable publicado nunca se borra durante la limpieza de un intento fallido.
        $cleanupPaths = @($stage, $temporary)
        if ($published) { $cleanupPaths += $backup }
        foreach ($path in $cleanupPaths) {
            try { Remove-GeneratedPath $projectRoot $path }
            catch {
                if ($null -eq $failure) { $failure = $_ }
                else { Write-Warning "No se pudo limpiar $path : $($_.Exception.Message)" }
            }
        }
        if ($null -ne $resumePath) {
            try {
                if (@(Get-LumiInstances).Count -eq 0) {
                    $launch = if ($published) { $executable } else { $resumePath }
                    if (-not (Test-Path -LiteralPath $launch -PathType Leaf)) { throw "No existe la build para reabrir: $launch" }
                    Start-Process -FilePath $launch -WorkingDirectory $projectRoot -WindowStyle Hidden -ErrorAction Stop | Out-Null
                }
            }
            catch {
                if ($null -eq $failure) { $failure = $_ }
                else { Write-Warning "No se pudo reabrir Lumi: $($_.Exception.Message)" }
            }
        }
    }
    finally { $lock.Dispose() }
}

if ($null -ne $failure) {
    if ($published) { Write-Warning "La nueva build está publicada en $executable, pero no se completaron todas las operaciones." }
    throw $failure
}
Write-Host "Ejecutable local listo: $executable"
if ($Installer) {
    Write-Host 'Ejecuta el instalador indicado para actualizar la copia del menú Inicio.'
} else {
    Write-Host 'Para generar un instalador, vuelve a ejecutar este script con -Installer.'
}
