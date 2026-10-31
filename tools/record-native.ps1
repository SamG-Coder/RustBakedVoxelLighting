param([string]$Output = 'evidence/native-live.mp4')
$ErrorActionPreference='Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$ready=Join-Path (Get-Location) 'evidence/native-ready.txt'
if(Test-Path -LiteralPath $ready){Remove-Item -LiteralPath $ready}
$env:BVL_SHOWCASE='1'
$env:BVL_RTX='1'
$game=Start-Process target/release/woodland.exe -PassThru -RedirectStandardOutput evidence/native-showcase.log -RedirectStandardError evidence/native-showcase.err
Remove-Item Env:BVL_SHOWCASE
Remove-Item Env:BVL_RTX
$timeout=[DateTime]::UtcNow.AddSeconds(180)
while(!(Test-Path -LiteralPath $ready)){
 if($game.HasExited){throw 'Native renderer exited before ready'}
 if([DateTime]::UtcNow -gt $timeout){throw 'Native startup timed out'}
 Start-Sleep -Milliseconds 200
}
# gdigrab exposes wall-clock input timestamps; keep the first packet timestamp
# in the diagnostic log to align the footage to the shared route's start epoch.
$env:FFREPORT='file=evidence/native-ffmpeg-report.log:level=48'
$captureArgs = '-report -hide_banner -y -debug_ts -f gdigrab -framerate 60 -draw_mouse 0 -i "title=RustBakedVoxelLighting - Native showcase" -t 150 -an -c:v h264_nvenc -preset p5 -cq 20 -pix_fmt yuv420p "' + $Output + '"'
$recorder=Start-Process ffmpeg -ArgumentList $captureArgs -PassThru -WindowStyle Hidden -RedirectStandardError evidence/native-capture.log -RedirectStandardOutput evidence/native-capture.out
$recorder.WaitForExit()
$recorder.Refresh()
Remove-Item Env:FFREPORT
if($null -ne $recorder.ExitCode -and $recorder.ExitCode -ne 0){throw "Capture failed: inspect evidence/native-capture.log"}
