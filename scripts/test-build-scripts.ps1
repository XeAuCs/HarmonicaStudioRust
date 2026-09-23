# Isolated filesystem failure fixtures. Never starts the application or an input player.
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'output.ps1')
$buildRoot=Join-Path $ProjectRoot 'build'
Assert-NoLinksInPath $buildRoot
New-Item -ItemType Directory -Path $buildRoot -Force | Out-Null
$fixtureRoot=Join-Path $buildRoot ('script-tests-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
$results=[Collections.Generic.List[object]]::new()
function Assert-True([bool]$Condition,[string]$Message){if(-not $Condition){throw $Message}}
function Assert-Fails([scriptblock]$Action){try{$null=& $Action}catch{return $_.Exception};throw '预期失败的操作意外成功。'}
function Write-FixtureFile([string]$Path,[string]$Content){New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($Path)) -Force | Out-Null;[IO.File]::WriteAllText($Path,$Content,[Text.UTF8Encoding]::new($false))}
function New-Fixture([string]$Name){$root=Join-Path $fixtureRoot $Name; $value=[PSCustomObject]@{Root=$root;Source=(Join-Path $root 'source');Destination=(Join-Path $root 'destination');Rollback=(Join-Path $root 'rollback')};New-Item -ItemType Directory -Path $value.Source,$value.Destination,$value.Rollback -Force | Out-Null;foreach($name in @('a.dll','b.dll')){Write-FixtureFile (Join-Path $value.Source $name) ('new-'+$name);Write-FixtureFile (Join-Path $value.Destination $name) ('old-'+$name)};return $value}
function Run-Check([string]$Name,[scriptblock]$Action){try{$null=& $Action;$results.Add([PSCustomObject]@{name=$Name;ok=$true});Write-Host "通过：$Name"}catch{$results.Add([PSCustomObject]@{name=$Name;ok=$false;message=$_.Exception.Message});Write-Host "失败：$Name -- $($_.Exception.Message)"}}
function Invoke-FixtureBatch([string]$Path,[string]$WorkingDirectory) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$env:ComSpec
    $info.Arguments='/d /v:off /s /c ""'+$Path+'""'
    $info.WorkingDirectory=$WorkingDirectory
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $info.RedirectStandardInput=$true
    $info.RedirectStandardOutput=$true
    $info.RedirectStandardError=$true
    $info.StandardOutputEncoding=[Text.Encoding]::UTF8
    $info.StandardErrorEncoding=[Text.Encoding]::UTF8
    $process=[Diagnostics.Process]::new()
    $process.StartInfo=$info
    try {
        if(-not $process.Start()){throw '无法启动隔离 CMD 检查。'}
        $process.StandardInput.Close()
        $stdout=$process.StandardOutput.ReadToEndAsync()
        $stderr=$process.StandardError.ReadToEndAsync()
        if(-not $process.WaitForExit(15000)){$process.Kill();$process.WaitForExit();throw '隔离 CMD 检查超时。'}
        return [PSCustomObject]@{ExitCode=$process.ExitCode;Output=$stdout.GetAwaiter().GetResult();Error=$stderr.GetAwaiter().GetResult()}
    } finally {$process.Dispose()}
}
function Remove-FixtureLink([string]$Path){
    $resolved=Assert-ChildPath $Path $fixtureRoot
    $item=Get-Item -LiteralPath $resolved -Force
    if(-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw '夹具链接清理目标不是链接。'}
    # Windows PowerShell 5.1 Remove-Item has a junction bug; this removes only the
    # known fixture link itself, never recursing into its target.
    [IO.Directory]::Delete($resolved)
}
try {
    Run-Check '项目源码单文件不超过 1000 行' {
        $root=Join-Path $fixtureRoot 'line-limit'
        $source=Join-Path $root 'src\example.rs'
        New-Item -ItemType Directory -Path (Split-Path -Parent $source) -Force | Out-Null
        [IO.File]::WriteAllLines($source,[string[]]@((1..1000 | ForEach-Object {'// line'})),[Text.UTF8Encoding]::new($false))
        Assert-SourceLineLimit -Root $root
        [IO.File]::AppendAllText($source,"// one more line`n",[Text.UTF8Encoding]::new($false))
        $error=Assert-Fails {Assert-SourceLineLimit -Root $root}
        Assert-True ($error.Message.Contains('example.rs')) '超长源码未被检查出来。'
    }
    Run-Check '源码目录无曲库时仍建立空便携曲库' {
        $root=Join-Path $fixtureRoot 'missing-samples'
        New-Item -ItemType Directory -Path $root | Out-Null
        $source=Join-Path $root 'samples'
        $destination=Join-Path $root 'portable\samples'
        Copy-OptionalSamples -Source $source -Destination $destination
        Assert-True (Test-Path -LiteralPath $destination -PathType Container) '缺少空便携曲库目录。'
        Assert-True (@(Get-ChildItem -LiteralPath $destination -Force).Count -eq 0) '空曲库被填入了本机数据。'
    }
    Run-Check '版本选择默认不更新且非法输入可重试' {
        $current='2.0.0-alpha.1'
        foreach($inputValue in @('', '   ', $current)) {
            $value=Read-BuildVersion $current { $inputValue }
            Assert-True ([string]::IsNullOrEmpty($value)) '默认或相同版本不应触发更新。'
        }
        $answers=[Collections.Generic.Queue[string]]::new()
        $answers.Enqueue('bad-version')
        $answers.Enqueue('1.0.0-01')
        $answers.Enqueue(' 2.0.0-alpha.2 ')
        $value=Read-BuildVersion $current { $answers.Dequeue() }
        Assert-True ($value -eq '2.0.0-alpha.2' -and $answers.Count -eq 0) '非法输入未重试或合法版本未保留。'
    }
    Run-Check '命令日志保留双输出、中文参数和失败退出码' {
        $root=Join-Path $fixtureRoot '日志 中文 空格'
        New-Item -ItemType Directory -Path $root | Out-Null
        $source=Join-Path $root 'log-fixture.rs'
        Write-FixtureFile $source 'fn main() {
    for (i, arg) in std::env::args().skip(1).enumerate() { println!("ARG{}={}", i, arg); }
    for i in 0..4000 { println!("OUT{}", i); eprintln!("ERR{}", i); }
    if std::env::args().any(|a| a == "fail") { std::process::exit(23); }
}'
        $cargo=Get-Cargo
        $rustc=Join-Path (Split-Path -Parent $cargo) 'rustc.exe'
        $exe=Join-Path $root 'log-fixture.exe'
        & $rustc --crate-name log_fixture $source -o $exe
        Assert-True ($LASTEXITCODE -eq 0) '日志夹具编译失败。'
        $log=Join-Path $root 'output.log'
        $arguments=@('中文 空格','quote"inside','C:\ending\','', 'a&b!')
        $leaked=@(Invoke-LoggedCommand -Executable $exe -Arguments $arguments -LogPath $log -Label '日志夹具')
        Assert-True ($leaked.Count -eq 0) '正常运行仍向控制台泄露详细输出。'
        $text=[IO.File]::ReadAllText($log)
        for($i=0;$i -lt $arguments.Count;$i++) { Assert-True ($text.Contains("ARG${i}=$($arguments[$i])")) '参数经过引号处理后改变。' }
        Assert-True ($text.Contains('OUT3999')) '标准输出被截断。'
        Assert-True ([IO.File]::ReadAllText([IO.Path]::ChangeExtension($log,'stderr.log')).Contains('ERR3999')) '错误输出被截断。'
        $error=Assert-Fails {Invoke-LoggedCommand -Executable $exe -Arguments @('fail') -LogPath (Join-Path $root 'failure.log') -Label '日志夹具'}
        Assert-True ($error.Message.Contains('退出码 23')) '失败退出码被吞掉。'
        Assert-True ($error.Message.Contains('ERR3999') -and $error.Message.Contains('failure.log')) '失败未显示错误摘要和日志路径。'
    }
    Run-Check '便携启动器透传复杂参数、工作目录、双输出和退出码' {
        $root=Join-Path $fixtureRoot '启动器 中文 空格 &!'
        $program=Join-Path $root 'program'
        New-Item -ItemType Directory -Path $program -Force | Out-Null
        $cargo=Get-Cargo
        $rustc=Join-Path (Split-Path -Parent $cargo) 'rustc.exe'
        $launcher=Join-Path $root 'HarmonicaStudio.exe'
        Invoke-LoggedCommand $rustc @('--edition=2024','--crate-name','launcher_fixture',(Join-Path $ProjectRoot 'src\launcher.rs'),'-o',$launcher) (Join-Path $root 'compile.log') '启动器夹具'
        $source=Join-Path $root 'child.rs'
        Write-FixtureFile $source 'fn main() { for (i,a) in std::env::args().skip(1).enumerate() { println!("ARG{}={}",i,a); } println!("CWD={}",std::env::current_dir().unwrap().display()); eprintln!("child-stderr"); std::process::exit(23); }'
        Invoke-LoggedCommand $rustc @('--crate-name','child_fixture',$source,'-o',(Join-Path $program 'HarmonicaStudio.exe')) (Join-Path $root 'child-compile.log') '子程序夹具'
        $log=Join-Path $root 'launch.log'
        $arguments=@('中文 空格','quote"inside','C:\ending\','', 'a&b!')
        $failure=Assert-Fails {Invoke-LoggedCommand $launcher $arguments $log '启动器'}
        Assert-True ($failure.Message.Contains('退出码 23')) '子程序退出码丢失。'
        $output=[IO.File]::ReadAllText($log)
        for($i=0;$i -lt $arguments.Count;$i++){Assert-True ($output.Contains("ARG${i}=$($arguments[$i])")) '启动器改变了参数。'}
        Assert-True ($output.Contains('CWD='+(Get-Location).Path)) '启动器改变了调用者工作目录。'
        Assert-True ([IO.File]::ReadAllText([IO.Path]::ChangeExtension($log,'stderr.log')).Contains('child-stderr')) '子程序错误输出丢失。'
        Remove-Item -LiteralPath (Join-Path $program 'HarmonicaStudio.exe')
        $failure=Assert-Fails {Invoke-LoggedCommand $launcher @('build-info') (Join-Path $root 'missing.log') '缺失程序'}
        Assert-True ($failure.Message.Contains('退出码 1')) '内部程序缺失没有明确失败。'
    }
    Run-Check '旧布局迁移保留修改、未知文件和个人数据' {
        $f=New-Fixture 'legacy-preserve'
        Write-FixtureFile (Join-Path $f.Source 'program\portable-layout.txt') 'harmonica-studio-portable-v2'
        foreach($relative in @('assets\player.ahk','en-us\resources.pri')) {
            Write-FixtureFile (Join-Path $f.Source ('program\'+$relative)) 'official'
            Write-FixtureFile (Join-Path $f.Destination $relative) 'official'
        }
        Write-FixtureFile (Join-Path $f.Destination 'assets\player.ahk') 'personal edit'
        Write-FixtureFile (Join-Path $f.Destination 'assets\my-file.txt') 'keep'
        Write-FixtureFile (Join-Path $f.Destination 'data\settings.json') 'personal'
        foreach($retired in @('assets\phone.svg','HarmonicaStudio.pdb','third_party\licenses\harmonica-studio-2.1.1\LICENSE')){Write-FixtureFile (Join-Path $f.Destination $retired) 'retired artifact'}
        $result=Install-PortableTree $f.Source $f.Destination $f.Rollback -MigrateLegacy
        Assert-True ($result.Migrated -eq 5) '已知旧文件或明确废弃文件没有迁移。'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $f.Destination 'en-us'))) '空语言目录没有清理。'
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'assets\my-file.txt')) -eq 'keep') '未知个人文件被删除。'
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'data\settings.json')) -eq 'personal') '个人数据被改变。'
        $archives=@(Get-TreeFilesNoLinks (Join-Path $f.Destination 'program\previous-layout'))
        Assert-True ($archives.Count -eq 4 -and @($archives | Where-Object {[IO.File]::ReadAllText($_.FullName) -eq 'personal edit'}).Count -eq 1) '有差异的旧文件没有保留原文。'
        $again=Install-PortableTree $f.Source $f.Destination (Join-Path $f.Root 'rollback-again') -MigrateLegacy
        Assert-True ($again.Migrated -eq 0) '重复安装重复迁移。'
    }
    Run-Check '迁移清理中途失败恢复旧布局与旧程序' {
        $f=New-Fixture 'legacy-rollback'
        Write-FixtureFile (Join-Path $f.Source 'program\portable-layout.txt') 'harmonica-studio-portable-v2'
        foreach($name in @('one.txt','two.txt')) {
            Write-FixtureFile (Join-Path $f.Source ('program\assets\'+$name)) 'new'
            Write-FixtureFile (Join-Path $f.Destination ('assets\'+$name)) 'old'
        }
        $failure=Assert-Fails {Install-PortableTree $f.Source $f.Destination $f.Rollback -MigrateLegacy -BeforeLegacyRemove {param($index,$item);if($index -eq 1){throw 'injected migration failure'}}}
        Assert-True ($failure.Message.Contains('injected migration failure') -and -not $failure.Data['KeepRollback']) '迁移故障未完整回滚。'
        foreach($name in @('one.txt','two.txt')){Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination ('assets\'+$name))) -eq 'old') '旧布局文件未恢复。'}
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'a.dll')) -eq 'old-a.dll') '原程序未恢复。'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $f.Destination 'program\portable-layout.txt'))) '新布局标记未撤销。'
    }
    Run-Check 'PowerShell 脚本语法' {
        foreach($file in Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.ps1'){$tokens=$null;$errors=$null;[Management.Automation.Language.Parser]::ParseFile($file.FullName,[ref]$tokens,[ref]$errors)|Out-Null;Assert-True ($errors.Count -eq 0) ('脚本解析失败：'+$file.Name)}
    }
    Run-Check 'CMD 入口使用 UTF-8 无 BOM 和完整 CRLF' {
        foreach($name in @('启动口琴工坊.cmd','打包.cmd')){
            $bytes=[IO.File]::ReadAllBytes((Join-Path $ProjectRoot $name))
            Assert-True (-not ($bytes.Length -ge 3 -and $bytes[0] -eq 0xef -and $bytes[1] -eq 0xbb -and $bytes[2] -eq 0xbf)) ('CMD 不应包含 BOM：'+$name)
            $text=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)
            Assert-True ($text -notmatch '(?<!\r)\n|\r(?!\n)') ('CMD 包含不兼容的换行：'+$name)
            Assert-True ($text.EndsWith([string][char]13+[char]10)) ('CMD 缺少结尾换行：'+$name)
        }
    }
    Run-Check '启动入口缺少成品时显示完整中文并返回失败' {
        $root=Join-Path $fixtureRoot '缺少成品 中文 空格 (入口)&!'
        New-Item -ItemType Directory -Path $root | Out-Null
        $batch=Join-Path $root '启动口琴工坊.cmd'
        Copy-Item -LiteralPath (Join-Path $ProjectRoot '启动口琴工坊.cmd') -Destination $batch
        $result=Invoke-FixtureBatch $batch $fixtureRoot
        Assert-True ($result.ExitCode -eq 1) '缺少程序时没有返回退出码 1。'
        Assert-True ($result.Output.Contains('找不到口琴工坊程序，请先运行“打包.cmd”生成便携版。')) '缺少程序提示被 CMD 误解析或中文损坏。'
        Assert-True ([string]::IsNullOrWhiteSpace($result.Error)) ('CMD 解析错误：'+$result.Error)
    }
    Run-Check '启动入口从其他目录正确启动中文空格路径中的无窗口夹具' {
        $root=Join-Path $fixtureRoot '存在成品 中文 空格 (入口)&!'
        $portable=Join-Path $root 'app\HarmonicaStudio'
        New-Item -ItemType Directory -Path $portable | Out-Null
        $batch=Join-Path $root '启动口琴工坊.cmd'
        Copy-Item -LiteralPath (Join-Path $ProjectRoot '启动口琴工坊.cmd') -Destination $batch
        $source=Join-Path $root 'fixture.rs'
        Write-FixtureFile $source '#![windows_subsystem = "windows"]
fn main() {
    let exe = std::env::current_exe().unwrap();
    let cwd = std::env::current_dir().unwrap();
    std::fs::write(exe.with_extension("launched"), cwd.to_string_lossy().as_bytes()).unwrap();
}'
        $cargo=Get-Cargo
        $rustc=Join-Path (Split-Path -Parent $cargo) 'rustc.exe'
        Assert-True (Test-Path -LiteralPath $rustc -PathType Leaf) 'Rust 工具链缺少 rustc.exe。'
        $exe=Join-Path $portable 'HarmonicaStudio.exe'
        $compilerOutput=@(& $rustc --edition=2024 --crate-name cmd_launch_fixture -C 'target-feature=+crt-static' $source -o $exe 2>&1)
        Assert-True ($LASTEXITCODE -eq 0) ('无窗口夹具编译失败：'+($compilerOutput -join ' '))
        $result=Invoke-FixtureBatch $batch $fixtureRoot
        Assert-True ($result.ExitCode -eq 0) '存在成品时启动入口返回失败。'
        Assert-True ([string]::IsNullOrWhiteSpace($result.Error)) ('启动入口输出错误：'+$result.Error)
        $marker=[IO.Path]::ChangeExtension($exe,'launched')
        $timer=[Diagnostics.Stopwatch]::StartNew()
        while(-not (Test-Path -LiteralPath $marker) -and $timer.ElapsedMilliseconds -lt 10000){Start-Sleep -Milliseconds 20}
        Assert-True (Test-Path -LiteralPath $marker -PathType Leaf) '启动入口没有执行到无窗口夹具。'
        Assert-True ([IO.File]::ReadAllText($marker).Equals($portable,[StringComparison]::OrdinalIgnoreCase)) '应用工作目录没有定位到便携文件夹。'
    }
    Run-Check '打包 CMD 正确定位脚本并保留成功及失败退出码' {
        $root=Join-Path $fixtureRoot '打包 中文 空格 (入口)&!'
        $scripts=Join-Path $root 'scripts'
        New-Item -ItemType Directory -Path $scripts | Out-Null
        $batch=Join-Path $root '打包.cmd'
        Copy-Item -LiteralPath (Join-Path $ProjectRoot '打包.cmd') -Destination $batch
        foreach($expectedExit in @(23,0)){
            $stub='param([switch]$Console,[switch]$PromptVersion); $ErrorActionPreference = ''Stop''; [IO.File]::WriteAllText((Join-Path $PSScriptRoot ''called.txt''),(Get-Location).Path); [IO.File]::WriteAllText((Join-Path $PSScriptRoot ''prompt.txt''),$PromptVersion.IsPresent.ToString()); exit '+$expectedExit
            Write-FixtureFile (Join-Path $scripts 'build.ps1') $stub
            $result=Invoke-FixtureBatch $batch $fixtureRoot
            Assert-True ($result.ExitCode -eq $expectedExit) ('打包退出码丢失，预期 '+$expectedExit+'，实际 '+$result.ExitCode)
            Assert-True ([string]::IsNullOrWhiteSpace($result.Error)) ('打包 CMD 解析失败：'+$result.Error)
            Assert-True ($result.Output.Contains('按任意键关闭窗口...')) '打包结束提示中文损坏或丢失。'
            Assert-True ([IO.File]::ReadAllText((Join-Path $scripts 'prompt.txt')) -eq 'True') '双击打包入口没有启用版本选择。'
            Assert-True ([IO.File]::ReadAllText((Join-Path $scripts 'called.txt')).Equals($root,[StringComparison]::OrdinalIgnoreCase)) '打包脚本未在项目目录执行。'
        }
    }
    Run-Check '安装保留曲库、设置和其他用户文件' {
        $f=New-Fixture 'preserve'
        foreach($relative in @('samples\song.mid','data\preferences.json')){Write-FixtureFile (Join-Path $f.Source $relative) 'new seed';Write-FixtureFile (Join-Path $f.Destination $relative) 'user content'}
        Write-FixtureFile (Join-Path $f.Source 'samples\new.mid') 'new song'
        Write-FixtureFile (Join-Path $f.Destination 'my-score.hstudio') 'personal score'
        $report=Install-PortableTree $f.Source $f.Destination $f.Rollback
        Assert-True ($report.Preserved -eq 2) '已有数据没有计入保留。'
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'a.dll')) -eq 'new-a.dll') '运行文件没有更新。'
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'samples\song.mid')) -eq 'user content') '已有曲库被覆盖。'
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'data\preferences.json')) -eq 'user content') '已有设置被覆盖。'
        Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'my-score.hstudio')) -eq 'personal score') '个人工程被覆盖。'
        Assert-True (Test-Path -LiteralPath (Join-Path $f.Destination 'samples\new.mid')) '新增种子曲目没有安装。'
    }
    Run-Check '已锁定文件在写入前中止安装' {
        $f=New-Fixture 'locked-preflight';$lock=[IO.File]::Open((Join-Path $f.Destination 'b.dll'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)
        try{$null=Assert-Fails {Install-PortableTree $f.Source $f.Destination $f.Rollback};Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination 'a.dll')) -eq 'old-a.dll') '预检查失败仍替换了文件。'}finally{$lock.Dispose()}
    }
    Run-Check '中途失败撤销已完成替换' {
        $f=New-Fixture 'rollback'
        $failure=Assert-Fails {Install-PortableTree $f.Source $f.Destination $f.Rollback -BeforeReplace {param($index,$item);if($index -eq 1){throw 'injected install failure'}}}
        Assert-True ($failure.Message.Contains('injected install failure')) '未执行到中途故障注入。';Assert-True (-not $failure.Data['KeepRollback']) '完整回滚被误报为未完成。'
        foreach($name in @('a.dll','b.dll')){Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination $name)) -eq ('old-'+$name)) '旧运行文件没有恢复。'}
    }
    Run-Check '安装期间新增文件锁不会破坏原文件' {
        $f=New-Fixture 'late-lock';$holder=[PSCustomObject]@{Stream=$null}
        $callback={param($index,$item);if($index -eq 1){$holder.Stream=[IO.File]::Open($item.Target,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)}}.GetNewClosure()
        try{$failure=Assert-Fails {Install-PortableTree $f.Source $f.Destination $f.Rollback -BeforeReplace $callback};Assert-True ($null -ne $holder.Stream) '未执行到安装期间的锁注入。';Assert-True (-not $failure.Data['KeepRollback']) '没有变更的锁文件不应该阻止其他文件回滚。'}finally{if($holder.Stream){$holder.Stream.Dispose()}}
        foreach($name in @('a.dll','b.dll')){Assert-True ([IO.File]::ReadAllText((Join-Path $f.Destination $name)) -eq ('old-'+$name)) '锁定场景损坏了旧文件。'}
    }
    Run-Check '回滚也遇锁时保留原文件备份并报告失败' {
        $f=New-Fixture 'rollback-lock';$holder=[PSCustomObject]@{Stream=$null};$firstTarget=Join-Path $f.Destination 'a.dll'
        $callback={param($index,$item);if($index -eq 1){$holder.Stream=[IO.File]::Open($firstTarget,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None);throw 'failure after successful first replacement'}}.GetNewClosure()
        try{$failure=Assert-Fails {Install-PortableTree $f.Source $f.Destination $f.Rollback -BeforeReplace $callback};Assert-True ($failure.Data['KeepRollback'] -eq $true) '不完整回滚没有保留备份标记。';Assert-True ($failure.Message.Contains('failure after successful first replacement')) '原始失败被回滚异常掩盖。';Assert-True ([IO.File]::ReadAllText((Join-Path $f.Rollback 'a.dll')) -eq 'old-a.dll') '唯一原文件备份丢失。'}finally{if($holder.Stream){$holder.Stream.Dispose()}}
    }
    Run-Check '目标目录链接禁止安装及递归清理' {
        $f=New-Fixture 'target-link';$outside=Join-Path $f.Root 'outside';New-Item -ItemType Directory -Path $outside|Out-Null;Write-FixtureFile (Join-Path $outside 'sentinel.txt') 'keep';$link=Join-Path $f.Root 'linked'
        New-Item -ItemType Junction -Path $link -Target $outside | Out-Null
        try{$null=Assert-Fails {Install-PortableTree $f.Source (Join-Path $link 'app') $f.Rollback};$null=Assert-Fails {Remove-OwnedDirectory $link $f.Root};Assert-True ([IO.File]::ReadAllText((Join-Path $outside 'sentinel.txt')) -eq 'keep') '链接目标被修改。'}finally{Remove-FixtureLink $link}
    }
    Run-Check '遍历源目录前拒绝内部链接' {
        $f=New-Fixture 'source-link';$outside=Join-Path $f.Root 'outside';New-Item -ItemType Directory -Path $outside|Out-Null;Write-FixtureFile (Join-Path $outside 'sentinel.txt') 'keep';$link=Join-Path $f.Source 'linked'
        New-Item -ItemType Junction -Path $link -Target $outside | Out-Null
        try{$null=Assert-Fails {Copy-CheckedTree $f.Source $f.Destination};$null=Assert-Fails {Remove-OwnedDirectory $f.Source $f.Root};Assert-True ([IO.File]::ReadAllText((Join-Path $outside 'sentinel.txt')) -eq 'keep') '源链接目标被修改。'}finally{Remove-FixtureLink $link}
    }
    Run-Check '清理边界拒绝相邻目录' {
        $f=New-Fixture 'boundary';$null=Assert-Fails {Remove-OwnedDirectory $f.Source $f.Destination};Assert-True (Test-Path -LiteralPath $f.Source) '边界外目录被删除。'
    }
    Run-Check 'NuGet 顶层法律声明和版本校验' {
        Add-Type -AssemblyName System.IO.Compression
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $root=Join-Path $fixtureRoot 'nuget';New-Item -ItemType Directory -Path $root|Out-Null;$package=Join-Path $root 'Example.1.2.3.nupkg';$zip=[IO.Compression.ZipFile]::Open($package,[IO.Compression.ZipArchiveMode]::Create)
        try{foreach($entry in @(@('Example.nuspec','<package><metadata><id>Example</id><version>1.2.3</version></metadata></package>'),@('LICENSE.txt','license'),@('NOTICE.txt','notice'),@('../NOTICE-escape.txt','must not extract'))){$stream=$zip.CreateEntry($entry[0]).Open();$writer=[IO.StreamWriter]::new($stream);try{$writer.Write($entry[1])}finally{$writer.Dispose()}}}finally{$zip.Dispose()}
        $target=Join-Path $root 'licenses';$record=Copy-NugetNotices $package $target 'Example' '1.2.3';Assert-True ($record.files.Count -eq 3) '没有准确提取三个顶层文件。';Assert-True (-not (Test-Path -LiteralPath (Join-Path $root 'NOTICE-escape.txt'))) '解压越出了许可证目录。';$null=Assert-Fails {Copy-NugetNotices $package (Join-Path $root 'bad-version') 'Example' '9.9.9'}
    }
    Run-Check '实际 WinUI 与 WebView2 NuGet 许可证完整' {
        $cache=Ensure-ReactorNugetCache
        foreach($package in Get-ReactorNugetPackages){$archive=Join-Path $cache ($package.Name+'.'+$package.Version+'.nupkg');$target=Join-Path $fixtureRoot ('actual-'+$package.Name);$record=Copy-NugetNotices $archive $target $package.Name $package.Version;Assert-True ($record.files.Count -eq 3) '固定运行依赖缺少法律声明。';foreach($file in Get-ChildItem -LiteralPath $target -File){Assert-True ($file.Length -gt 0) '提取的许可证为空。'}}
    }
    Run-Check '版本同步只修改 package 版本并拒绝非法 SemVer' {
        $root=Join-Path $fixtureRoot 'version';New-Item -ItemType Directory -Path $root|Out-Null;$manifest=Join-Path $root 'Cargo.toml'
        $text="[package]`r`nname = `"example`"`r`nversion = `"1.0.0`"`r`n`r`n[dependencies.helper]`r`nversion = `"9.8.7`"`r`n"
        [IO.File]::WriteAllText($manifest,$text)
        Assert-True ((Get-CargoPackageVersion $manifest) -eq '1.0.0') '读到了依赖版本而不是项目版本。'
        Set-CargoPackageVersion $manifest '2.0.0-alpha.1+build.2'
        $updated=[IO.File]::ReadAllText($manifest)
        Assert-True ($updated.Contains('version = "2.0.0-alpha.1+build.2"')) '项目版本未更新。'
        Assert-True ($updated.Contains('version = "9.8.7"')) '错误修改了依赖版本。'
        foreach($version in @('01.0.0','1.0.0-01','1.0.0-..','1.0.0+')){$null=Assert-Fails {Set-CargoPackageVersion $manifest $version};Assert-True ([IO.File]::ReadAllText($manifest) -eq $updated) '非法版本改变了现有清单。'}
    }
    $failed=@($results | Where-Object {-not $_.ok})
    if($failed.Count){throw "构建脚本夹具验证失败：$($failed.Count)/$($results.Count)"}
    Write-Host "构建脚本验证：$($results.Count) 项全部通过。"
} finally {Remove-OwnedDirectory -Path $fixtureRoot -Parent $buildRoot}
