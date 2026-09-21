# Legacy migration is driven by the new package's exact file list, never a wildcard delete.
function Get-LegacyMigrationPlan([string]$Source,[string]$Destination,[string]$Rollback) {
    $program=Join-Path $Source 'program'
    if (-not (Test-Path -LiteralPath (Join-Path $program 'portable-layout.txt') -PathType Leaf)) { throw '新便携布局标记缺失。' }
    $allowed=@(Get-PortableRuntimeNames)+@('assets','third_party','LICENSE','THIRD_PARTY.md')
    $archive='program\previous-layout\'+[guid]::NewGuid().ToString('N')
    $candidates=[Collections.Generic.List[object]]::new()
    foreach($file in Get-TreeFilesNoLinks $program) {
        $relative=$file.FullName.Substring($program.Length+1)
        if (($relative -split '\\')[0] -notin $allowed) { continue }
        $candidates.Add([PSCustomObject]@{Relative=$relative;Reference=$file.FullName})
    }
    # Explicit retired application artifacts: archive their bytes, even if modified.
    foreach($relative in @('assets\phone.svg','assets\settings.svg','HarmonicaStudio.pdb')) {
        if ($relative -notin $candidates.Relative) { $candidates.Add([PSCustomObject]@{Relative=$relative;Reference=$null}) }
    }
    $oldLicenses=Join-Path $Destination 'third_party\licenses'
    Assert-NoLinksInPath $oldLicenses
    if (Test-Path -LiteralPath $oldLicenses -PathType Container) {
        foreach($directory in Get-ChildItem -LiteralPath $oldLicenses -Directory) {
            if ($directory.Name -notmatch '^harmonica-studio-(.+)$') { continue }
            try { Assert-CargoPackageVersion $Matches[1] } catch { continue }
            $relative='third_party\licenses\'+$directory.Name+'\LICENSE'
            if ($relative -notin $candidates.Relative) { $candidates.Add([PSCustomObject]@{Relative=$relative;Reference=$null}) }
        }
    }
    foreach($candidate in $candidates) {
        $relative=$candidate.Relative
        $old=Assert-ChildPath (Join-Path $Destination $relative) $Destination
        Assert-NoLinksInPath $old
        $item=Get-ExistingItem $old
        if (-not $item) { continue }
        if ($item.PSIsContainer) { throw "旧程序文件位置被目录占用：$relative" }
        $probe=[IO.File]::Open($old,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
        $probe.Dispose()
        $different=(-not $candidate.Reference) -or (Get-FileHash -LiteralPath $old -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $candidate.Reference -Algorithm SHA256).Hash
        $archiveRelative=Join-Path $archive $relative
        [PSCustomObject]@{
            Target=$old;Relative=$relative
            Backup=(Assert-ChildPath (Join-Path $Rollback ('.legacy\'+$relative)) $Rollback)
            ArchiveRelative=$archiveRelative
            ArchiveTarget=(Assert-ChildPath (Join-Path $Destination $archiveRelative) $Destination)
            Different=$different
        }
    }
}

function Remove-EmptyLegacyParents([string]$File,[string]$Root) {
    $parent=[IO.Path]::GetDirectoryName($File)
    while ($parent -and -not $parent.Equals($Root,[StringComparison]::OrdinalIgnoreCase)) {
        $parent=Assert-ChildPath $parent $Root
        Assert-NoLinksInPath $parent
        if (-not (Get-ExistingItem $parent)) { break }
        if (@(Get-ChildItem -LiteralPath $parent -Force).Count) { break }
        # Non-recursive: any personal file prevents removal of this directory.
        [IO.Directory]::Delete($parent)
        $parent=[IO.Path]::GetDirectoryName($parent)
    }
}
