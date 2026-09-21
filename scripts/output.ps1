# Console summaries and per-run diagnostic logs. Dot-source common.ps1 first.
function Write-ConsoleProgress {
    param($Id, [string]$Activity, [string]$Status, [int]$PercentComplete = -1, [switch]$Completed)
    # Append-only output avoids Windows PowerShell 5.1 CJK cursor/width bugs.
    if ($Completed) { return }
    if ($PercentComplete -ge 0) {
        $filled = [int][Math]::Floor($PercentComplete / 5)
        Write-Host ('[{0}{1}] {2}%' -f ('#' * $filled), ('-' * (20 - $filled)), $PercentComplete)
    } else {
        Write-Host "  ${Activity}：$Status"
    }
}
function Write-BuildStage([ValidateRange(1,6)][int]$Stage, [string]$Label) {
    Write-Host "[$Stage/6] $Label"
    Write-ConsoleProgress -Id 70 -Activity '口琴工坊 · 生成便携版' -Status "第 $Stage / 6 阶段：$Label" -PercentComplete ([int](100 * ($Stage - 1) / 6))
}
function Complete-BuildProgress {
    Write-ConsoleProgress -Id 70 -Activity '口琴工坊 · 生成便携版' -Completed
}
function New-RunLogDirectory([string]$Name) {
    $root = Join-Path $ProjectRoot 'verification'
    Assert-NoLinksInPath $root
    $path = Join-Path $root ($Name + '-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,6))
    New-Item -ItemType Directory -Path $path -Force | Out-Null
    return $path
}
function ConvertTo-ProcessArgument([string]$Value) {
    # Windows CommandLineToArgvW quoting, including quotes and trailing backslashes.
    return '"' + (($Value -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
}
function Invoke-LoggedCommand(
    [string]$Executable, [string[]]$Arguments, [string]$LogPath, [string]$Label
) {
    Assert-NoLinksInPath $LogPath
    $errorPath = [IO.Path]::ChangeExtension($LogPath, 'stderr.log')
    Assert-NoLinksInPath $errorPath
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.Arguments = ($Arguments | ForEach-Object { ConvertTo-ProcessArgument $_ }) -join ' '
    $info.WorkingDirectory = $ProjectRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables['CARGO_TERM_COLOR'] = 'never'
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $stdout = $null
    $stderr = $null
    try {
        $stdout = [IO.File]::Open($LogPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $stderr = [IO.File]::Open($errorPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        if (-not $process.Start()) { throw "无法启动$Label。" }
        # Drain both streams concurrently so compiler output cannot deadlock a full pipe.
        $outTask = $process.StandardOutput.BaseStream.CopyToAsync($stdout)
        $errTask = $process.StandardError.BaseStream.CopyToAsync($stderr)
        $commandTimer = [Diagnostics.Stopwatch]::StartNew()
        $nextNotice = 0.0
        do {
            if ($commandTimer.Elapsed.TotalSeconds -ge $nextNotice) {
                Write-ConsoleProgress -Id 71 -Activity $Label -Status ('正在运行 · 已用 {0:N0} 秒 · 详细内容写入日志' -f $commandTimer.Elapsed.TotalSeconds) -PercentComplete -1
                $nextNotice = $commandTimer.Elapsed.TotalSeconds + 10
            }
        } while (-not $process.WaitForExit(500))
        $null = $outTask.GetAwaiter().GetResult()
        $null = $errTask.GetAwaiter().GetResult()
        $code = $process.ExitCode
    } finally {
        Write-ConsoleProgress -Id 71 -Activity $Label -Completed
        if ($stdout) { $stdout.Dispose() }
        if ($stderr) { $stderr.Dispose() }
        $process.Dispose()
    }
    if ($code -ne 0) {
        $details = @()
        foreach ($path in @($LogPath, $errorPath)) {
            $details += @(Get-Content -LiteralPath $path -Encoding UTF8 -Tail 8 | Where-Object { $_.Trim() })
        }
        throw "${Label}失败（退出码 $code）。`n$($details -join "`n")`n详细日志：$LogPath`n错误日志：$errorPath"
    }
}
