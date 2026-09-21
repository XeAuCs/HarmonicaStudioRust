param([Parameter(Mandatory=$true)][string]$Destination)
. (Join-Path $PSScriptRoot 'common.ps1')
$cargo=Get-Cargo
Assert-NoLinksInPath $Destination
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
Push-Location $ProjectRoot
try {
    $metadata=& $cargo metadata --locked --offline --format-version 1
    if($LASTEXITCODE -ne 0){throw '无法读取已锁定依赖许可证清单。'}
    $metadata=($metadata -join "`n") | ConvertFrom-Json
    $records=[Collections.Generic.List[object]]::new()
    foreach($package in $metadata.packages){
        $records.Add([PSCustomObject]@{name=$package.name;version=$package.version;license=$package.license;repository=$package.repository})
        $folder=Split-Path -Parent $package.manifest_path
        Assert-NoLinksInPath $folder
        $licenseFiles=@(Get-ChildItem -LiteralPath $folder -File | Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|license-)'})
        if($package.license_file){
            $declared=$package.license_file
            if(-not [IO.Path]::IsPathRooted($declared)){$declared=Join-Path $folder $declared}
            $licenseFiles+=Get-Item -LiteralPath $declared
        }
        if($licenseFiles.Count){
            $target=Assert-ChildPath (Join-Path $Destination ($package.name+'-'+$package.version)) $Destination
            Assert-NoLinksInPath $target
            New-Item -ItemType Directory -Path $target -Force | Out-Null
            foreach($file in ($licenseFiles | Sort-Object FullName -Unique)){Assert-NoLinksInPath $file.FullName;Copy-Item -LiteralPath $file.FullName -Destination $target -Force}
        }
    }
    $records.ToArray() | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Destination 'dependencies.json') -Encoding UTF8
    foreach($name in @('license-mit','license-apache-2.0')) {Copy-Item -LiteralPath (Join-Path $ProjectRoot ('third_party\windows-rs\'+$name)) -Destination $Destination -Force}
    # These are the two fixed packages used by the checked-in reactor-setup.
    # Its extraction drops top-level notices, so read the original NuGet ZIPs.
    $setup=[IO.File]::ReadAllText((Join-Path $ProjectRoot 'third_party\windows-rs\crates\libs\reactor-setup\src\lib.rs'))
    $packages=@(Get-ReactorNugetPackages)
    $cache=Ensure-ReactorNugetCache
    $runtimeRecords=@()
    foreach($package in $packages){
        if(-not $setup.Contains('"'+$package.Name+'"') -or -not $setup.Contains('"'+$package.Version+'"')){throw '固定运行依赖已变化，须先更新许可证清单。'}
        $archive=Join-Path $cache ($package.Name+'.'+$package.Version+'.nupkg')
        $target=Join-Path $Destination ($package.Name+'-'+$package.Version)
        $runtimeRecords+=Copy-NugetNotices -PackagePath $archive -Destination $target -ExpectedName $package.Name -ExpectedVersion $package.Version
    }
    $runtimeRecords | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Destination 'runtime-dependencies.json') -Encoding UTF8
} finally {Pop-Location}

