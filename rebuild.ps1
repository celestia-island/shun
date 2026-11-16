$ErrorActionPreference = 'Stop'
Set-Location 'D:\源代码\工程项目\celestia\_worktree\shun\tui-image-protocols'
cmd /c "git show HEAD:shell/src/tui.rs > tui-head.tmp"
$t = [System.IO.File]::ReadAllText((Join-Path (Get-Location) 'tui-head.tmp'))
$patches = Get-Content 'D:\源代码\工程项目\celestia\_worktree\shun\tui-image-protocols\patches.json' -Raw | ConvertFrom-Json
$miss = New-Object System.Collections.Generic.List[string]
$i = 0
foreach ($p in $patches) {
    $i++
    if ($p.a -eq '') { $t = $t + $p.b; continue }
    if (-not $t.Contains($p.a)) { $miss.Add("pair $i : " + $p.a.Substring(0, [Math]::Min(70, $p.a.Length)).Replace([string][char]10, ' ~ ')); continue }
    $t = $t.Replace($p.a, $p.b)
}
[System.IO.File]::WriteAllText((Join-Path (Get-Location) 'shell\src\tui.rs'), $t)
Write-Output ('pairs applied: ' + ($patches.Count - $miss.Count) + '/' + $patches.Count)
if ($miss.Count) { $miss | ForEach-Object { Write-Output ('MISS: ' + $_) } }
Remove-Item 'tui-head.tmp'
