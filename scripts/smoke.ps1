param([string]$Path = (Join-Path $PSScriptRoot '..\app\HarmonicaStudio'), [string]$ExpectedVersion='')
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-NoLinksInPath $Path
$portable = (Resolve-Path -LiteralPath $Path).Path
$exe = Join-Path $portable 'HarmonicaStudio.exe'
$required = @('HarmonicaStudio.exe','program\HarmonicaStudio.exe','program\portable-layout.txt','program\third_party\AutoHotkey\AutoHotkey64.exe','program\assets\player.ahk','program\assets\remote.html','program\assets\remote.css','program\assets\remote.js','program\third_party\licenses\dependencies.json','program\third_party\licenses\runtime-dependencies.json') + @(Get-PortableRuntimeNames | ForEach-Object {'program\'+$_})
foreach ($file in $required) {
    if (-not (Get-ExistingItem (Join-Path $portable $file))) { throw "便携版缺少运行文件：$file" }
}
Assert-NoPython -Root $portable -WholeTree
$buildRoot = Join-Path $ProjectRoot 'build'
Assert-NoLinksInPath $buildRoot
New-Item -ItemType Directory -Path $buildRoot -Force | Out-Null
$scratch = Join-Path $buildRoot ('portable-smoke-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $buildInfo = Invoke-PortableJson -Executable $exe -Command build-info -TimeoutSeconds 20 -Scratch $scratch
    if ($buildInfo.desktop_enabled -isnot [bool] -or $buildInfo.desktop_enabled -ne $true) { throw '此成品未编译 WinUI 3 桌面功能。' }
    if ($ExpectedVersion -and $buildInfo.version -ne $ExpectedVersion) {throw '成品版本与请求版本不一致。'}
    if (-not ([IO.Path]::GetFullPath($buildInfo.executable)).Equals((Join-Path $portable 'program\HarmonicaStudio.exe'),[StringComparison]::OrdinalIgnoreCase)) { throw '启动器未执行内部主程序。' }
    $launcherVersion=(Get-Item -LiteralPath $exe).VersionInfo.ProductVersion
    if ($launcherVersion -ne $buildInfo.version) { throw '启动器资源版本与主程序不一致。' }
    foreach($check in @(@('application_root',$portable),@('resource_root',(Join-Path $portable 'program')),@('data_root',(Join-Path $scratch 'data')),@('default_library_root',(Join-Path $portable 'samples')))) {
        if (-not ([string]$buildInfo.($check[0])).Equals($check[1],[StringComparison]::OrdinalIgnoreCase)) { throw ('便携路径检查失败：'+$check[0]) }
    }
    $result = Invoke-PortableJson -Executable $exe -Command self-test -TimeoutSeconds 60 -Scratch $scratch
    foreach($flag in @('ok','desktop_enabled','project_roundtrip','export_consistency','controller_save','ahk_no_input_validation')) {
        if($result.$flag -isnot [bool] -or $result.$flag -ne $true){throw "成品自测没有通过：$flag"}
    }
    if($result.python_required -isnot [bool] -or $result.python_required -ne $false){throw '成品自测未确认不需要外部解释器。'}
    if($result.version -ne $buildInfo.version){throw '成品编译信息和自测版本不一致。'}
    $result | ConvertTo-Json -Depth 10
} finally {Remove-OwnedDirectory -Path $scratch -Parent $buildRoot}

