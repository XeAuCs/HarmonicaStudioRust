$ErrorActionPreference = 'Stop'
$ProjectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
function Get-Cargo {
    $localCargo = Join-Path $ProjectRoot '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo -PathType Leaf) {
        $env:CARGO_HOME = Join-Path $ProjectRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $ProjectRoot '.tools\rustup'
        $bin = Split-Path -Parent $localCargo
        if (($env:PATH -split ';') -notcontains $bin) { $env:PATH = $bin + ';' + $env:PATH }
        return $localCargo
    }
    $cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargoCommand) { throw '请安装 Rust stable MSVC 工具链以及 Visual Studio C++ 构建工具。' }
    return $cargoCommand.Source
}
function Get-ExistingItem([string]$Path) {
    try { Get-Item -LiteralPath $Path -Force -ErrorAction Stop }
    catch { if ($_.CategoryInfo.Category -ne [Management.Automation.ErrorCategory]::ObjectNotFound) { throw } }
}
function Assert-NoLinksInPath([string]$Path) {
    $probe = [IO.Path]::GetFullPath($Path)
    while ($probe) {
        $item = Get-ExistingItem $probe
        if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "路径包含文件系统链接，已停止：$probe" }
        $probe = [IO.Path]::GetDirectoryName($probe)
    }
}
function Assert-ChildPath([string]$Path, [string]$Parent) {
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\','/')
    $root = [IO.Path]::GetFullPath($Parent).TrimEnd('\','/')
    if (-not $full.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw '路径必须位于本次任务目录之内。' }
    return $full
}
function Get-TreeFilesNoLinks([string]$Path) {
    Assert-NoLinksInPath $Path
    $root = Get-Item -LiteralPath $Path -Force
    if (-not $root.PSIsContainer) { return $root }
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($root.FullName)
    while ($pending.Count) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "目录包含文件系统链接，已停止：$($item.FullName)" }
            if ($item.PSIsContainer) { $pending.Push($item.FullName) } else { $item }
        }
    }
}
function Assert-NoPython([string]$Root = $ProjectRoot, [switch]$WholeTree) {
    if ($WholeTree) { $sourceFiles = @(Get-TreeFilesNoLinks $Root) }
    else {
        Assert-NoLinksInPath $Root
        $sourceFiles = @(Get-ChildItem -LiteralPath $Root -File -Force)
        foreach ($folder in @('src','tests','scripts','assets','third_party','samples')) {
            $path = Join-Path $Root $folder
            if (Get-ExistingItem $path) { $sourceFiles += @(Get-TreeFilesNoLinks $path) }
        }
    }
    $forbidden = @($sourceFiles | Where-Object { $_.Extension -in '.py','.pyc','.pyo','.pyw','.pyd' -or $_.Name -match '^(python|pypy).*\.(exe|dll)$' })
    if ($forbidden.Count) { throw "发现 $($forbidden.Count) 个禁止的解释器或源码文件。" }
}
function Get-ReactorNugetPackages {
    @(
        [PSCustomObject]@{Name='Microsoft.WindowsAppSDK.Runtime';Version='2.5.1'},
        [PSCustomObject]@{Name='Microsoft.Web.WebView2';Version='1.0.4078.44'}
    )
}
function Test-NugetArchive([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    try {
        $zip=[IO.Compression.ZipFile]::OpenRead($Path)
        try { return ($zip.Entries.Count -gt 0) }
        finally { $zip.Dispose() }
    } catch { return $false }
}
function Ensure-ReactorNugetCache {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) { throw '无法确定 NuGet 缓存目录：LOCALAPPDATA 未设置。' }
    $cache=Join-Path $env:LOCALAPPDATA 'windows-reactor-setup\temp'
    Assert-NoLinksInPath $cache
    New-Item -ItemType Directory -Path $cache -Force | Out-Null
    Assert-NoLinksInPath $cache
    $curl=Join-Path $env:SystemRoot 'System32\curl.exe'
    if (-not (Test-Path -LiteralPath $curl -PathType Leaf)) { throw '系统缺少 curl.exe，无法下载固定 WinUI 运行依赖。' }
    foreach($package in Get-ReactorNugetPackages){
        $name=$package.Name
        $version=$package.Version
        $archive=Join-Path $cache ($name+'.'+$version+'.nupkg')
        Assert-NoLinksInPath $archive
        if (Test-NugetArchive $archive) { continue }
        $old=Get-ExistingItem $archive
        if ($old) { Remove-Item -LiteralPath $archive -Force }
        $partial=Join-Path $cache ('.'+$name+'.'+$version+'.nupkg.download')
        Assert-NoLinksInPath $partial
        $old=Get-ExistingItem $partial
        if ($old) { Remove-Item -LiteralPath $partial -Force }
        $url='https://www.nuget.org/api/v2/package/'+$name+'/'+$version
        Write-Host "正在准备固定运行依赖：$name $version"
        & $curl '--fail' '--location' '--silent' '--show-error' '--retry' '3' '--retry-delay' '1' '--output' $partial $url
        $exitCode=$LASTEXITCODE
        if ($exitCode -ne 0 -or -not (Test-NugetArchive $partial)) {
            if (Get-ExistingItem $partial) { Remove-Item -LiteralPath $partial -Force }
            throw "下载固定运行依赖失败：$name $version；请检查 NuGet 网络连接后重试。"
        }
        Assert-NoLinksInPath $partial
        [IO.File]::Move($partial,$archive)
        if (-not (Test-NugetArchive $archive)) { throw "固定运行依赖下载后校验失败：$name $version。" }
    }
    return $cache
}
function Remove-OwnedDirectory([string]$Path, [string]$Parent) {
    $resolved = Assert-ChildPath -Path $Path -Parent $Parent
    Assert-NoLinksInPath $resolved
    $item = Get-ExistingItem $resolved
    if (-not $item) { return }
    if (-not $item.PSIsContainer) { throw '清理目标不是目录。' }
    # Enumerate each level only after rejecting links; never traverse a link first.
    $null = @(Get-TreeFilesNoLinks $resolved)
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
function Copy-CheckedTree([string]$Source, [string]$Destination) {
    $sourceRoot = [IO.Path]::GetFullPath($Source).TrimEnd('\','/')
    $files = @(Get-TreeFilesNoLinks $sourceRoot)
    Assert-NoLinksInPath $Destination
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    foreach ($file in $files) {
        $relative = $file.FullName.Substring($sourceRoot.Length + 1)
        $target = Assert-ChildPath (Join-Path $Destination $relative) $Destination
        Assert-NoLinksInPath $target
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($target)) -Force | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $target -Force
    }
}
function Get-PortableRuntimeNames {
    $runtimeList = Join-Path $ProjectRoot 'third_party\windows-rs\crates\libs\reactor-setup\assets\runtime.txt'
    $names = @(Get-Content -LiteralPath $runtimeList | ForEach-Object { $_.Trim() } | Where-Object { $_ }) + @('Microsoft.Web.WebView2.Core.dll')
    foreach ($name in $names) { if ($name -notmatch '^[A-Za-z0-9_.-]+$' -or $name -in '.','..') { throw '运行文件清单包含非法路径。' } }
    return $names
}
function Assert-PortableNotRunning([string]$Executable) {
    $target = [IO.Path]::GetFullPath($Executable)
    foreach ($process in Get-Process -Name HarmonicaStudio -ErrorAction SilentlyContinue) {
        $path = $process.Path
        if (-not $path) { throw '无法确认已运行程序的位置，请先保存并关闭口琴工坊后重试。' }
        if ([IO.Path]::GetFullPath($path).Equals($target,[StringComparison]::OrdinalIgnoreCase)) { throw '目标程序正在运行，请先保存并关闭，然后重新打包。' }
    }
}
function Replace-FileAtomically([string]$Source, [string]$Target, [bool]$Existed) {
    Assert-NoLinksInPath $Source
    Assert-NoLinksInPath $Target
    $temporary = Join-Path ([IO.Path]::GetDirectoryName($Target)) ('.hs-install-' + [guid]::NewGuid().ToString('N') + '.tmp')
    try {
        Copy-Item -LiteralPath $Source -Destination $temporary
        if ($Existed) { [IO.File]::Replace($temporary,$Target,[System.Management.Automation.Language.NullString]::Value) }
        else { [IO.File]::Move($temporary,$Target) }
    } finally {
        if (Get-ExistingItem $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}
function Install-PortableTree {
    param([Parameter(Mandatory=$true)][string]$Source,
          [Parameter(Mandatory=$true)][string]$Destination,
          [Parameter(Mandatory=$true)][string]$Rollback,
          [scriptblock]$BeforeReplace,
          [switch]$MigrateLegacy,
          [scriptblock]$BeforeLegacyRemove)
    $sourceRoot = [IO.Path]::GetFullPath($Source).TrimEnd('\','/')
    $destinationRoot = [IO.Path]::GetFullPath($Destination).TrimEnd('\','/')
    $rollbackRoot = [IO.Path]::GetFullPath($Rollback).TrimEnd('\','/')
    if ($sourceRoot -eq $destinationRoot -or $rollbackRoot -eq $destinationRoot -or $rollbackRoot.StartsWith($destinationRoot+'\',[StringComparison]::OrdinalIgnoreCase)) { throw '暂存、安装及回滚目录必须分离。' }
    $files = @(Get-TreeFilesNoLinks $sourceRoot | Sort-Object FullName)
    Assert-NoLinksInPath $destinationRoot
    Assert-NoLinksInPath $rollbackRoot
    New-Item -ItemType Directory -Path $destinationRoot,$rollbackRoot -Force | Out-Null
    $plan = [Collections.Generic.List[object]]::new()
    $preserved = 0
    foreach ($file in $files) {
        $relative = $file.FullName.Substring($sourceRoot.Length + 1)
        $target = Assert-ChildPath (Join-Path $destinationRoot $relative) $destinationRoot
        Assert-NoLinksInPath $target
        $old = Get-ExistingItem $target
        if ($old -and $old.PSIsContainer) { throw "安装文件位置已被目录占用：$relative" }
        if ($old -and ($relative.StartsWith('samples\',[StringComparison]::OrdinalIgnoreCase) -or $relative.StartsWith('data\',[StringComparison]::OrdinalIgnoreCase))) { $preserved++; continue }
        if ($old) {
            # Detect locked/read-only destinations before replacing any file.
            $probe = [IO.File]::Open($target,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
            $probe.Dispose()
        }
        $backup = Assert-ChildPath (Join-Path $rollbackRoot $relative) $rollbackRoot
        $plan.Add([PSCustomObject]@{Source=$file.FullName;Target=$target;Backup=$backup;Existed=($null -ne $old);Relative=$relative})
    }
    $legacy=@()
    if ($MigrateLegacy) {
        . (Join-Path $PSScriptRoot 'portable-migration.ps1')
        $legacy=@(Get-LegacyMigrationPlan $sourceRoot $destinationRoot $rollbackRoot)
        foreach($item in $legacy) {
            if (-not $item.Different) { continue }
            Assert-NoLinksInPath $item.ArchiveTarget
            if (Get-ExistingItem $item.ArchiveTarget) { throw '旧文件备份路径已存在。' }
            $plan.Add([PSCustomObject]@{Source=$item.Target;Target=$item.ArchiveTarget;Backup='';Existed=$false;Relative=$item.ArchiveRelative})
            $preserved++
        }
    }
    $installed = [Collections.Generic.List[object]]::new()
    $removed = [Collections.Generic.List[object]]::new()
    try {
        for ($index=0; $index -lt $plan.Count; $index++) {
            $item = $plan[$index]
            Assert-NoLinksInPath $item.Target
            New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($item.Target)) -Force | Out-Null
            if ($item.Existed) {
                New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($item.Backup)) -Force | Out-Null
                Copy-Item -LiteralPath $item.Target -Destination $item.Backup
            }
            # This optional callback is used only by isolated fault-injection fixtures.
            if ($BeforeReplace) { & $BeforeReplace $index $item }
            Replace-FileAtomically -Source $item.Source -Target $item.Target -Existed $item.Existed
            $installed.Add($item)
        }
        Assert-NoLinksInPath (Join-Path $destinationRoot 'data')
        New-Item -ItemType Directory -Path (Join-Path $destinationRoot 'data') -Force | Out-Null
        foreach($item in $legacy) {
            Assert-NoLinksInPath $item.Target
            Assert-NoLinksInPath $item.Backup
            New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($item.Backup)) -Force | Out-Null
            Copy-Item -LiteralPath $item.Target -Destination $item.Backup
            if ($BeforeLegacyRemove) { & $BeforeLegacyRemove $removed.Count $item }
            Remove-Item -LiteralPath $item.Target -Force
            $removed.Add($item)
        }
        foreach($item in $removed) { Remove-EmptyLegacyParents $item.Target $destinationRoot }
        return [PSCustomObject]@{Installed=$installed.Count;Preserved=$preserved;Migrated=$removed.Count}
    } catch {
        $original = $_.Exception
        $failures = [Collections.Generic.List[string]]::new()
        for ($index=$removed.Count-1; $index -ge 0; $index--) {
            $item=$removed[$index]
            try {
                Assert-NoLinksInPath $item.Target
                New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($item.Target)) -Force | Out-Null
                Replace-FileAtomically -Source $item.Backup -Target $item.Target -Existed ([bool](Get-ExistingItem $item.Target))
            } catch { $failures.Add($item.Relative+'：'+$_.Exception.Message) }
        }
        for ($index=$installed.Count-1; $index -ge 0; $index--) {
            $item = $installed[$index]
            try {
                Assert-NoLinksInPath $item.Target
                if ($item.Existed) { Replace-FileAtomically -Source $item.Backup -Target $item.Target -Existed $true }
                elseif (Get-ExistingItem $item.Target) { Remove-Item -LiteralPath $item.Target -Force }
            } catch { $failures.Add($item.Relative + '：' + $_.Exception.Message) }
        }
        $message = '安装失败：' + $original.Message
        if ($failures.Count) { $message += "`n部分文件未能回滚，原文件备份保留在：$rollbackRoot`n" + ($failures -join "`n") }
        else { $message += '；本次已替换的文件已回滚。' }
        $failure = [InvalidOperationException]::new($message,$original)
        $failure.Data['KeepRollback'] = ($failures.Count -gt 0)
        throw $failure
    }
}
function Invoke-PortableJson([string]$Executable,[ValidateSet('build-info','self-test')][string]$Command,[int]$TimeoutSeconds=60,[string]$Scratch) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.Arguments = $Command
    $info.WorkingDirectory = Split-Path -Parent $Executable
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($Scratch) {
        $info.EnvironmentVariables['HARMONICA_STUDIO_HOME'] = Join-Path $Scratch 'data'
        $info.EnvironmentVariables['TEMP'] = $Scratch
        $info.EnvironmentVariables['TMP'] = $Scratch
    }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    try {
        if (-not $process.Start()) { throw '无法启动成品检查进程。' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds*1000)) {
            $process.Kill()
            if (-not $process.WaitForExit(5000)) { throw "成品 $Command 超时，检查进程尚未退出。" }
            throw "成品 $Command 检查超时。"
        }
        $output = $stdout.GetAwaiter().GetResult()
        $errorText = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "成品 $Command 检查失败，退出码 $($process.ExitCode)：$errorText" }
        try { return $output | ConvertFrom-Json } catch { throw "成品 $Command 未返回有效 JSON。" }
    } finally { $process.Dispose() }
}
function Copy-NugetNotices([string]$PackagePath,[string]$Destination,[string]$ExpectedName,[string]$ExpectedVersion) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    Assert-NoLinksInPath $PackagePath
    Assert-NoLinksInPath $Destination
    $zip = [IO.Compression.ZipFile]::OpenRead($PackagePath)
    try {
        $nuspec = @($zip.Entries | Where-Object { $_.FullName -eq ($ExpectedName+'.nuspec') })
        if ($nuspec.Count -ne 1 -or $nuspec[0].Length -gt 1048576) { throw 'NuGet 元数据缺失或过大。' }
        $stream = $nuspec[0].Open()
        $settings = [Xml.XmlReaderSettings]::new()
        $settings.DtdProcessing = [Xml.DtdProcessing]::Prohibit
        $settings.XmlResolver = $null
        $reader = [Xml.XmlReader]::Create($stream,$settings)
        try { $xml = [Xml.XmlDocument]::new(); $xml.XmlResolver=$null; $xml.Load($reader) }
        finally { $reader.Dispose(); $stream.Dispose() }
        if ($xml.package.metadata.id -ne $ExpectedName -or $xml.package.metadata.version -ne $ExpectedVersion) { throw 'NuGet 名称或版本与固定运行依赖不一致。' }
        $entries = @($zip.Entries | Where-Object { $_.FullName -match '^(?i:LICENSE|LICENCE|NOTICE|ThirdPartyNotices)(\.[A-Za-z0-9]+)?$' -or $_.FullName -eq ($ExpectedName+'.nuspec') })
        if (-not ($entries | Where-Object {$_.FullName -match '^(?i:LICENSE|LICENCE)'})) { throw 'NuGet 缺少顶层许可证。' }
        if (-not ($entries | Where-Object {$_.FullName -match '^(?i:NOTICE|ThirdPartyNotices)'})) { throw 'NuGet 缺少顶层第三方声明。' }
        New-Item -ItemType Directory -Path $Destination -Force | Out-Null
        foreach ($entry in $entries) {
            if ($entry.Length -gt 2097152) { throw 'NuGet 法律声明超过大小限制。' }
            $target = Assert-ChildPath (Join-Path $Destination $entry.FullName) $Destination
            Assert-NoLinksInPath $target
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,$target,$true)
        }
        return [PSCustomObject]@{name=$ExpectedName;version=$ExpectedVersion;files=@($entries | ForEach-Object {$_.FullName});source=('https://www.nuget.org/packages/'+$ExpectedName+'/'+$ExpectedVersion)}
    } finally { $zip.Dispose() }
}
function Assert-CargoPackageVersion([string]$Version) {
    $pattern='^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$'
    $versionMatch=[regex]::Match($Version,$pattern)
    if(-not $versionMatch.Success){throw '目标版本必须为 SemVer，例如 2.0.0 或 2.0.0-alpha.1。'}
    foreach($part in ($versionMatch.Groups['pre'].Value -split '\.')){if($part -match '^\d+$' -and $part.Length -gt 1 -and $part.StartsWith('0')){throw 'SemVer 数字预发布标识不能以零开头。'}}
}
function Get-CargoPackageVersion([string]$ManifestPath) {
    Assert-NoLinksInPath $ManifestPath
    $manifest=[IO.File]::ReadAllText($ManifestPath)
    $package=[regex]::Match($manifest,'(?ms)^\[package\][ \t]*\r?\n.*?(?=^\[|\z)')
    if(-not $package.Success){throw 'Cargo.toml 缺少 package 配置。'}
    $versions=[regex]::Matches($package.Value,'(?m)^version[ \t]*=[ \t]*"(?<value>[^"\r\n]+)"[ \t]*\r?$')
    if($versions.Count -ne 1){throw '无法确定唯一的项目版本声明。'}
    return $versions[0].Groups['value'].Value
}
function Read-BuildVersion([string]$CurrentVersion, [scriptblock]$ReadValue = { Read-Host '输入新版本号（直接回车保持当前版本）' }) {
    Write-Host "当前版本：$CurrentVersion"
    Write-Host '默认不更新版本；例如可输入 2.0.0 或 2.0.0-alpha.2。'
    while($true) {
        $value=([string](& $ReadValue)).Trim()
        if(-not $value -or $value -eq $CurrentVersion){return ''}
        try { Assert-CargoPackageVersion $value; return $value }
        catch { Write-Host ('版本号无效：'+$_.Exception.Message) }
    }
}
function Set-CargoPackageVersion([string]$ManifestPath,[string]$Version) {
    Assert-CargoPackageVersion $Version
    Assert-NoLinksInPath $ManifestPath
    $manifest=[IO.File]::ReadAllText($ManifestPath)
    $package=[regex]::Match($manifest,'(?ms)^\[package\][ \t]*\r?\n.*?(?=^\[|\z)')
    if(-not $package.Success){throw 'Cargo.toml 缺少 package 配置。'}
    $versionPattern='(?m)^version[ \t]*=[ \t]*"[^"\r\n]+"[ \t]*\r?$'
    $versionLine=[regex]::Matches($package.Value,$versionPattern)
    if($versionLine.Count -ne 1){throw '无法确定唯一的项目版本声明。'}
    $line='version = "'+$Version+'"'
    if($versionLine[0].Value.EndsWith("`r")){$line+="`r"}
    $updated=[regex]::Replace($package.Value,$versionPattern,$line)
    $manifest=$manifest.Substring(0,$package.Index)+$updated+$manifest.Substring($package.Index+$package.Length)
    [IO.File]::WriteAllText($ManifestPath,$manifest,[Text.UTF8Encoding]::new($false))
}
