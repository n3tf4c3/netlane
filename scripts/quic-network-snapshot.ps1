# Read-only inventory shared by the opt-in elevated trial and its unelevated launcher.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$RulesPath)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$adapters = @(Get-NetAdapter -Name Ethernet,Wi-Fi | Sort-Object InterfaceGuid)
if ($adapters.Count -ne 2) { throw 'Ethernet/Wi-Fi não identificáveis.' }
$interfaces = @(foreach ($adapter in $adapters) {
    $index = $adapter.ifIndex
    [ordered]@{
        Name = $adapter.Name
        Guid = $adapter.InterfaceGuid.ToString()
        Index = $index
        Status = $adapter.Status.ToString()
        Bindings = @(Get-NetAdapterBinding -Name $adapter.Name -ComponentID ms_tcpip,ms_tcpip6 |
            Sort-Object ComponentID | Select-Object ComponentID,Enabled)
        Addresses = @(Get-NetIPAddress -InterfaceIndex $index | Sort-Object AddressFamily,IPAddress |
            Select-Object AddressFamily,IPAddress,PrefixLength,AddressState)
        DefaultRoutes = @(Get-NetRoute -InterfaceIndex $index |
            Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0','::/0') } |
            Sort-Object AddressFamily,NextHop,RouteMetric | Select-Object AddressFamily,DestinationPrefix,NextHop,RouteMetric)
        Metrics = @(Get-NetIPInterface -InterfaceIndex $index | Sort-Object AddressFamily |
            Select-Object AddressFamily,AutomaticMetric,InterfaceMetric,ConnectionState)
        Dns = @(Get-DnsClientServerAddress -InterfaceIndex $index | Sort-Object AddressFamily |
            Select-Object AddressFamily,ServerAddresses)
    }
})
# Flags are checked separately in C#, so the network inventory can also be compared while flags are enabled.
ConvertTo-Json -InputObject ([ordered]@{
    Interfaces = $interfaces
    RealRulesSha256 = (Get-FileHash -LiteralPath $RulesPath -Algorithm SHA256).Hash
    ServiceProcessIds = @(Get-Process -Name NetLane.Service -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
}) -Depth 12 -Compress
