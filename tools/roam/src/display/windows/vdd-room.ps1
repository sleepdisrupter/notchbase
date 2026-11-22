# Gives the Virtual Display Driver room for more screens: sets how many it makes in its settings
# file and restarts its device, which reads the file as it starts. Roam runs this as
# administrator, hidden, only while none of the driver's screens is in use, and reads what
# happened from room.log in $Folder.
param([Parameter(Mandatory = $true)][string]$Folder, [int]$Monitors = 4)

$ErrorActionPreference = 'Stop'
$Log = Join-Path $Folder 'room.log'
$HardwareId = 'Root\MttVDD'

function Say([string]$Text) { Add-Content -Path $Log -Value $Text }

try {
    # The driver reads its settings from the folder its installer names, else this one.
    $Settings = 'C:\VirtualDisplayDriver'
    $Named = Get-ItemProperty -Path 'HKLM:\SOFTWARE\MikeTheTech\VirtualDisplayDriver' -Name 'VDDPATH' -ErrorAction SilentlyContinue
    if ($Named -and $Named.VDDPATH -and (Test-Path (Join-Path $Named.VDDPATH 'vdd_settings.xml'))) { $Settings = $Named.VDDPATH }
    $File = Join-Path $Settings 'vdd_settings.xml'
    if (-not (Test-Path $File)) { throw "The driver's settings aren't at $File" }
    $Xml = New-Object System.Xml.XmlDocument
    $Xml.PreserveWhitespace = $true
    $Xml.Load($File)
    $Count = $Xml.SelectSingleNode('//monitors/count')
    if (-not $Count) { throw "The driver's settings in $File don't say how many screens it makes" }
    $Had = 0
    [void][int]::TryParse($Count.InnerText.Trim(), [ref]$Had)
    if ($Had -lt $Monitors) {
        $Count.InnerText = "$Monitors"
        $Xml.Save($File)
        Say "Screens in $File`: $Had to $Monitors"
    }

    $Devices = @(Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object { $_.HardwareID -contains $HardwareId })
    if ($Devices.Count -eq 0) { throw 'The display driver has no device' }
    foreach ($Device in $Devices) {
        Say "Restarting $($Device.InstanceId)"
        $Out = & pnputil.exe /restart-device $Device.InstanceId
        Say ($Out -join ' ')
        # Windows before 10 2004 has no /restart-device: turn it off and on again.
        if ($LASTEXITCODE -ne 0) {
            Disable-PnpDevice -InstanceId $Device.InstanceId -Confirm:$false
            Start-Sleep -Seconds 2
            Enable-PnpDevice -InstanceId $Device.InstanceId -Confirm:$false
        }
    }
    Say 'DONE'
    exit 0
} catch {
    Say "FAILED: $($_.Exception.Message)"
    exit 1
}
