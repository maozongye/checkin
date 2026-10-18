# Windows：创建计划任务，每天 09:10（请将系统时区设为「中国」或自行调整）
# 用法（管理员 PowerShell 可选）：
#   .\scripts\install_schtasks.ps1
#   .\scripts\install_schtasks.ps1 -At "09:30"
#   .\scripts\install_schtasks.ps1 -TaskName "CheckinSuiteDaily"

param(
    [string]$At = "09:10",
    [string]$TaskName = "CheckinSuite-WorkBuddy-Trae",
    [string]$Python = "python"
)

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Script = Join-Path $Root "scripts\checkin_once.py"
$Accounts = Join-Path $Root "accounts.json"
$Config = Join-Path $Root "config.json"
$LogDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

$Arg = "`"$Script`" --platform all --accounts `"$Accounts`" --config `"$Config`""
$Action = New-ScheduledTaskAction -Execute $Python -Argument $Arg -WorkingDirectory $Root
$Trigger = New-ScheduledTaskTrigger -Daily -At $At
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable

Register-ScheduledTask -TaskName $TaskName -Action $Action -Trigger $Trigger -Settings $Settings -Force | Out-Null

Write-Host "已创建计划任务: $TaskName"
Write-Host "  时间: 每天 $At"
Write-Host "  工作目录: $Root"
Write-Host "查看: Get-ScheduledTask -TaskName $TaskName"
