# Installs the Virtual Display Driver the way its project's own silent install does
# (Community Scripts/silent-install.ps1 in github.com/VirtualDrivers/Virtual-Display-Driver):
# NefCon creates the driver's device. Any earlier device and driver package go first, since the
# driver in the project's 25.5.2 setup never makes a screen. Roam runs this as administrator,
# hidden, and reads what happened from install.log in $Folder. Downloads are pinned by SHA-256.
param([Parameter(Mandatory = $true)][string]$Folder, [int]$Monitors = 1, [string]$Sizes = '')

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Log = Join-Path $Folder 'install.log'

function Say([string]$Text) { Add-Content -Path $Log -Value $Text }

$NefConUrl = 'https://github.com/nefarius/nefcon/releases/download/v1.14.0/nefcon_v1.14.0.zip'
$NefConSha = 'A15557DA24A9EFCA203158DE3B43B0EAF982DB231F0194031F1ED428BC13E669'
# Driver 11.30.4.434, the one the project's maintainers say works.
$DriverUrl = 'https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/download/25.7.23/VirtualDisplayDriver-x86.Driver.Only.zip'
$DriverSha = 'E24210692B442B39AF763536330CE78B423F19342B7A7792C26DE3944E418B3A'
$HardwareId = 'Root\MttVDD'
$Settings = 'C:\VirtualDisplayDriver'

function Fetch([string]$Url, [string]$Sha, [string]$Name) {
    $Path = Join-Path $Folder "$Name.zip"
    Say "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Path -UseBasicParsing
    $Got = (Get-FileHash -Path $Path -Algorithm SHA256).Hash
    if ($Got -ne $Sha) { throw "The $Name download didn't match (SHA-256 $Got)" }
    $Out = Join-Path $Folder $Name
    Expand-Archive -Path $Path -DestinationPath $Out -Force
    return $Out
}

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $NefCon = Join-Path (Fetch $NefConUrl $NefConSha 'nefcon') 'x64\nefconc.exe'
    $Driver = Join-Path (Fetch $DriverUrl $DriverSha 'driver') 'VirtualDisplayDriver'

    # The driver reads its settings from here. Settings already there stay.
    New-Item -ItemType Directory -Path $Settings -Force | Out-Null
    $Kept = Join-Path $Settings 'vdd_settings.xml'
    if (-not (Test-Path $Kept)) { Copy-Item (Join-Path $Driver 'vdd_settings.xml') $Kept }
    # As many screens as Roam may lend at once: each stays off until it's in use.
    try {
        $Xml = New-Object System.Xml.XmlDocument
        $Xml.PreserveWhitespace = $true
        $Xml.Load($Kept)
        $Count = $Xml.SelectSingleNode('//monitors/count')
        $Had = 0
        if ($Count -and [int]::TryParse($Count.InnerText.Trim(), [ref]$Had) -and $Had -lt $Monitors) {
            $Count.InnerText = "$Monitors"
            $Xml.Save($Kept)
            Say "Screens: $Had to $Monitors"
        }
    } catch {
        Say "Couldn't set how many screens the driver makes: $($_.Exception.Message)"
    }
    # And the sizes the lent screens take, so the pointer meets their edges where it would.
    try {
        $Xml = New-Object System.Xml.XmlDocument
        $Xml.PreserveWhitespace = $true
        $Xml.Load($Kept)
        $List = $Xml.SelectSingleNode('//resolutions')
        $Added = 0
        foreach ($Size in ($Sizes -split ',' | Where-Object { $_ -match '^\d+x\d+$' })) {
            if (-not $List) { break }
            $Width, $Height = $Size -split 'x'
            $There = @($List.SelectNodes('resolution') | Where-Object {
                    $W = $_.SelectSingleNode('width'); $H = $_.SelectSingleNode('height')
                    $W -and $H -and $W.InnerText.Trim() -eq $Width -and $H.InnerText.Trim() -eq $Height
                })
            if ($There.Count -gt 0) { continue }
            $Node = $Xml.CreateElement('resolution')
            foreach ($Pair in @(@('width', $Width), @('height', $Height), @('refresh_rate', '60'))) {
                $Child = $Xml.CreateElement($Pair[0])
                $Child.InnerText = $Pair[1]
                [void]$Node.AppendChild($Child)
            }
            [void]$List.AppendChild($Node)
            $Added++
            Say "Size added: $Size"
        }
        if ($Added -gt 0) { $Xml.Save($Kept) }
    } catch {
        Say "Couldn't add the screens' sizes: $($_.Exception.Message)"
    }
    # Roam adds sizes to the settings without administrator rights.
    & icacls.exe $Settings /grant '*S-1-5-32-545:(OI)(CI)M' | Out-Null

    # Trust the driver's publisher, so Windows installs it without asking again.
    Say 'Trusting the driver publisher'
    $Certificates = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2Collection
    $Certificates.Import([IO.File]::ReadAllBytes((Join-Path $Driver 'mttvdd.cat')))
    $Store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'LocalMachine')
    $Store.Open('ReadWrite')
    foreach ($Certificate in $Certificates) { $Store.Add($Certificate) }
    $Store.Close()

    # Every earlier device goes, or it picks up the new driver too and makes a second screen.
    Say 'Removing any earlier virtual display devices and drivers'
    $Devices = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object { $_.HardwareID -contains $HardwareId }
    foreach ($Device in $Devices) {
        $Out = & pnputil.exe /remove-device $Device.InstanceId
        Say "Removed $($Device.InstanceId): $($Out -join ' ')"
        # Windows before 10 2004 has no /remove-device: NefCon removes one device a time.
        if ($LASTEXITCODE -ne 0) {
            & $NefCon --remove-device-node --hardware-id $HardwareId --class-guid '{4d36e968-e325-11ce-bfc1-08002be10318}' | Out-Null
        }
    }
    $Packages = Get-WindowsDriver -Online | Where-Object { $_.OriginalFileName -like '*mttvdd.inf' }
    foreach ($Package in $Packages) {
        $Out = & pnputil.exe /delete-driver $Package.Driver /uninstall /force
        Say "Removed $($Package.Driver) ($($Package.Version))"
    }

    Say 'Creating the display device'
    $Out = & $NefCon install (Join-Path $Driver 'MttVDD.inf') $HardwareId
    Say ($Out -join ' ')
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 3010) { throw "NefCon couldn't install the driver (code $LASTEXITCODE)" }
    Say 'DONE'
    exit 0
} catch {
    Say "FAILED: $($_.Exception.Message)"
    exit 1
}
