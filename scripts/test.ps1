param([switch]$CoreOnly, [string]$LogDirectory='', [switch]$NoBanner)
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'output.ps1')
$cargo = Get-Cargo
if (-not $LogDirectory) { $LogDirectory = New-RunLogDirectory 'test' }
Assert-NoLinksInPath $LogDirectory
New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
if (-not $NoBanner) { Write-Host "口琴工坊 · 自动检查`n" }
Assert-NoPython
Assert-SourceLineLimit
Push-Location $ProjectRoot
try {
    if (-not $CoreOnly) { $null=Ensure-ReactorNugetCache }
    $arguments = @('test','--locked')
    if ($CoreOnly) { $arguments += @('--no-default-features','--target-dir',(Join-Path $ProjectRoot 'target\core-only'),'--lib','--test','core_contracts','--test','application_contracts') }
    else { $arguments += @('--features','desktop','--target-dir',(Join-Path $ProjectRoot 'target')) }
    Write-Host '  正在检查 Rust 代码…'
    $rustLog = Join-Path $LogDirectory 'rust-tests.log'
    Invoke-LoggedCommand -Executable $cargo -Arguments $arguments -LogPath $rustLog -Label 'Rust 测试'
    $passed = 0
    $ignored = 0
    $summaries = 0
    foreach ($line in Get-Content -LiteralPath $rustLog -Encoding UTF8) {
        if ($line -match '^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;') {
            $passed += [int]$Matches[1]
            $ignored += [int]$Matches[3]
            $summaries++
        }
    }
    if (-not $summaries) { throw "未找到 Rust 测试结果，请查看：$rustLog" }
    $note = if ($ignored) { "；另有 $ignored 项跳过" } else { '' }
    Write-Host "  Rust 测试：$passed 项通过$note"
    Write-Host '  正在检查构建与安装保护…'
    & (Join-Path $PSScriptRoot 'test-build-scripts.ps1') *>&1 | Out-File -LiteralPath (Join-Path $LogDirectory 'script-tests.log') -Encoding UTF8
    $summary = Get-Content -LiteralPath (Join-Path $LogDirectory 'script-tests.log') -Encoding UTF8 | Where-Object { $_ -match '^构建脚本验证：' } | Select-Object -Last 1
    if (-not $summary) { throw "未找到构建脚本检查结果，请查看：$LogDirectory" }
    Write-Host ('  ' + $summary)
    if (-not $NoBanner) {
        Write-Host "`n自动检查完成。"
        Write-Host "详细日志：$LogDirectory"
    }
} catch {
    $_ | Format-List * -Force | Out-String -Width 240 | Out-File -LiteralPath (Join-Path $LogDirectory 'test-failure.log') -Encoding UTF8
    Write-Host "`n自动检查未通过。详细日志：$LogDirectory"
    throw
} finally { Pop-Location }

