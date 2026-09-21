param([string]$Version='', [string]$DistPath='', [switch]$Console, [switch]$PromptVersion)
trap {
    if ($runLogs -and (Test-Path -LiteralPath $runLogs)) {
        $_ | Format-List * -Force | Out-String -Width 240 | Out-File -LiteralPath (Join-Path $runLogs 'failure.log') -Encoding UTF8
    }
    if ($Console) { Write-Host ("`n打包失败：" + $_.Exception.Message); exit 1 }
    throw $_
}
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'output.ps1')
$cargo=Get-Cargo
$runLogs=New-RunLogDirectory 'build'
$timer=[Diagnostics.Stopwatch]::StartNew()
Write-Host "口琴工坊 · 生成便携版`n"
Write-Host "详细日志：$runLogs`n"
$manifestPath=Join-Path $ProjectRoot 'Cargo.toml'
$currentVersion=Get-CargoPackageVersion $manifestPath
$Version=$Version.Trim()
if($PromptVersion -and -not $Version){$Version=Read-BuildVersion $currentVersion}
Write-BuildStage 1 '检查环境与运行状态'
if (-not $DistPath) { $DistPath=Join-Path $ProjectRoot 'app' }
$DistPath=[IO.Path]::GetFullPath($DistPath).TrimEnd('\','/')
$destination=Join-Path $DistPath 'HarmonicaStudio'
$targetExe=Join-Path $destination 'HarmonicaStudio.exe'
Assert-NoLinksInPath $destination
Assert-PortableNotRunning $targetExe
Assert-PortableNotRunning (Join-Path $destination 'program\HarmonicaStudio.exe')
Assert-NoPython
$verification=Join-Path $ProjectRoot 'verification'
$buildRoot=Join-Path $ProjectRoot 'build'
Assert-NoLinksInPath $verification
Assert-NoLinksInPath $buildRoot
New-Item -ItemType Directory -Path $verification,$buildRoot -Force | Out-Null
$stage=Join-Path $buildRoot ('portable-stage-'+[guid]::NewGuid().ToString('N'))
$portable=Join-Path $stage 'portable'
$rollback=Join-Path $stage 'rollback'
New-Item -ItemType Directory -Path $portable,$rollback -Force | Out-Null
$keepStage=$false
$transcribing=$false
$pushed=$false
$buildFailure=$null
$cleanupFailures=[Collections.Generic.List[string]]::new()
try {
    Start-Transcript -Path (Join-Path $runLogs 'build.log') -Force | Out-Null
    $transcribing=$true
    Push-Location $ProjectRoot
    $pushed=$true
    if ($Version -and $Version -ne $currentVersion) {
        Assert-CargoPackageVersion $Version
        $packageVersion=$Version
        Write-Host "版本号：$currentVersion → $packageVersion"
        Set-CargoPackageVersion -ManifestPath $manifestPath -Version $Version
        Invoke-LoggedCommand -Executable $cargo -Arguments @('update','--offline','-p','harmonica-studio') -LogPath (Join-Path $runLogs 'version.log') -Label '版本同步（源码版本可能已变更，旧便携版仍保留）'
    } else {
        $packageVersion=$currentVersion
        Write-Host "版本号：$packageVersion（保持不变）"
    }
    Write-BuildStage 2 '运行自动检查'
    & (Join-Path $PSScriptRoot 'test.ps1') -LogDirectory $runLogs -NoBanner
    Write-BuildStage 3 '编译正式版本（首次运行可能较久）'
    Invoke-LoggedCommand -Executable $cargo -Arguments @('build','--locked','--release','--features','desktop','--target-dir',(Join-Path $ProjectRoot 'target')) -LogPath (Join-Path $runLogs 'compile.log') -Label '正式版编译'
    Write-BuildStage 4 '收集运行资源与许可证'
    $release=Join-Path $ProjectRoot 'target\release'
    Assert-NoLinksInPath $release
    $program=Join-Path $portable 'program'
    New-Item -ItemType Directory -Path $program -Force | Out-Null
    $rustc=Join-Path (Split-Path -Parent $cargo) 'rustc.exe'
    $launcherBuild=Join-Path $stage 'launcher'
    New-Item -ItemType Directory -Path $launcherBuild -Force | Out-Null
    $launcherExe=Join-Path $launcherBuild 'HarmonicaStudio.exe'
    Invoke-LoggedCommand -Executable $rustc -Arguments @('--edition=2024','--crate-name','harmonica_launcher','-C','opt-level=s','-C','target-feature=+crt-static','-C',('link-arg='+(Join-Path $release 'studio-launcher.res')),(Join-Path $ProjectRoot 'src\launcher.rs'),'-o',$launcherExe) -LogPath (Join-Path $runLogs 'launcher.log') -Label '便携启动器编译'
    Copy-Item -LiteralPath $launcherExe -Destination $portable
    Copy-Item -LiteralPath (Join-Path $release 'HarmonicaStudio.exe') -Destination $program
    [IO.File]::WriteAllText((Join-Path $program 'portable-layout.txt'),'harmonica-studio-portable-v2',[Text.UTF8Encoding]::new($false))
    foreach($name in Get-PortableRuntimeNames) {
        $runtimeFile=Join-Path $release $name
        $item=Get-Item -LiteralPath $runtimeFile
        if($item.PSIsContainer) {Copy-CheckedTree $runtimeFile (Join-Path $program $name)}
        else {Assert-NoLinksInPath $runtimeFile; Copy-Item -LiteralPath $runtimeFile -Destination $program}
    }
    Copy-CheckedTree (Join-Path $ProjectRoot 'samples') (Join-Path $portable 'samples')
    Copy-CheckedTree (Join-Path $ProjectRoot 'assets') (Join-Path $program 'assets')
    Copy-CheckedTree (Join-Path $ProjectRoot 'third_party\AutoHotkey') (Join-Path $program 'third_party\AutoHotkey')
    foreach($name in @('LICENSE','THIRD_PARTY.md','使用说明.txt')) {
        $path=Join-Path $ProjectRoot $name
        Assert-NoLinksInPath $path
        $documentRoot=if($name -eq '使用说明.txt'){$portable}else{$program}
        Copy-Item -LiteralPath $path -Destination $documentRoot
    }
    & (Join-Path $PSScriptRoot 'collect-licenses.ps1') -Destination (Join-Path $program 'third_party\licenses') *>&1 | Out-File -LiteralPath (Join-Path $runLogs 'licenses.log') -Encoding UTF8
    New-Item -ItemType Directory -Path (Join-Path $portable 'data') -Force | Out-Null
    Write-BuildStage 5 '检查成品完整性与功能'
    $smoke=& (Join-Path $PSScriptRoot 'smoke.ps1') -Path $portable -ExpectedVersion $packageVersion
    $smoke | Set-Content -LiteralPath (Join-Path $runLogs 'portable-smoke.json') -Encoding UTF8
    $smoke | Set-Content -LiteralPath (Join-Path $verification 'portable-smoke.json') -Encoding UTF8
    # Recheck after the build: an existing user program may have been opened meanwhile.
    Write-BuildStage 6 '安装便携版并保留已有数据'
    Assert-PortableNotRunning $targetExe
    Assert-PortableNotRunning (Join-Path $destination 'program\HarmonicaStudio.exe')
    $installed=Install-PortableTree -Source $portable -Destination $destination -Rollback $rollback -MigrateLegacy
} catch {
    $buildFailure=$_
    if ($_.Exception.Data['KeepRollback']) { $keepStage=$true }
} finally {
    if($pushed){Pop-Location}
    try {
        if($keepStage){Write-Warning "自动回滚未全部完成，已保留原文件备份：$rollback"}
        else {Remove-OwnedDirectory -Path $stage -Parent $buildRoot}
    } catch {$cleanupFailures.Add('清理暂存目录失败：'+$_.Exception.Message)}
    if($transcribing){try{Stop-Transcript | Out-Null}catch{$cleanupFailures.Add('关闭构建日志失败：'+$_.Exception.Message)}}
}
if($buildFailure){
    if($cleanupFailures.Count){throw [InvalidOperationException]::new(($buildFailure.Exception.Message+"`n"+($cleanupFailures -join "`n")),$buildFailure.Exception)}
    throw $buildFailure
}
if($cleanupFailures.Count){throw ($cleanupFailures -join "`n")}
Complete-BuildProgress
Write-ConsoleProgress -Activity '打包完成' -PercentComplete 100
$summary = @(
    ("打包完成，用时 {0:N1} 秒。" -f $timer.Elapsed.TotalSeconds),
    "版本号：$packageVersion",
    "程序位置：$targetExe",
    "文件处理：更新 $($installed.Installed) 个；保留 $($installed.Preserved) 个已有文件。",
    "详细日志：$runLogs"
)
$summary | Set-Content -LiteralPath (Join-Path $runLogs 'summary.txt') -Encoding UTF8
Write-Host ("`n" + ($summary -join "`n"))
