# Console summaries and per-run diagnostic logs. Dot-source common.ps1 first.
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
        $process.WaitForExit()
        $null = $outTask.GetAwaiter().GetResult()
        $null = $errTask.GetAwaiter().GetResult()
        $code = $process.ExitCode
    } finally {
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
