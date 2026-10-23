# Embedded in easy-eq.exe; environment values are data, never PowerShell source.
$ErrorActionPreference = 'Stop'
$executable = $env:PEACE2_EXECUTABLE
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$name = "Easy EQ ($sid)"
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
function Remove-Task([string] $taskName) {
    foreach ($task in $folder.GetTasks(1)) {
        if ($task.Name -eq $taskName) { $folder.DeleteTask($taskName, 0); break }
    }
}
if ($env:PEACE2_STARTUP -eq 'disable') {
    Remove-Task $name
} else {
    $escaped = [Security.SecurityElement]::Escape($executable)
    $xml = @"
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Keep Easy EQ available at sign-in, unlock and resume. Explicit Quit stops it until the next trigger or launch.</Description></RegistrationInfo>
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled><UserId>$sid</UserId></LogonTrigger>
    <SessionStateChangeTrigger><Enabled>true</Enabled><StateChange>SessionUnlock</StateChange><UserId>$sid</UserId></SessionStateChangeTrigger>
    <EventTrigger><Enabled>true</Enabled><Subscription>&lt;QueryList&gt;&lt;Query Id="0" Path="System"&gt;&lt;Select Path="System"&gt;*[System[Provider[@Name='Microsoft-Windows-Power-Troubleshooter'] and EventID=1]]&lt;/Select&gt;&lt;/Query&gt;&lt;/QueryList&gt;</Subscription><Delay>PT3S</Delay></EventTrigger>
  </Triggers>
  <Principals><Principal id="User"><UserId>$sid</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate><StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand><Enabled>true</Enabled><Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle><WakeToRun>false</WakeToRun><ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>
  </Settings>
  <Actions Context="User"><Exec><Command>$escaped</Command><Arguments>--tray</Arguments></Exec></Actions>
</Task>
"@
    $folder.RegisterTask($name, $xml, 6, $sid, $null, 3, $null) | Out-Null
}
# Migrate the prototype's Run entry only after task configuration succeeds.
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Test-Path -LiteralPath $run) { Remove-ItemProperty -LiteralPath $run -Name 'Peace 2' -ErrorAction SilentlyContinue }
Remove-Task "Peace 2 ($sid)"
