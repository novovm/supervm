# Capability probe only. New ephemeral loopback sockets; never touches a node
# socket, identity, wire codec, firewall, or service. Not a throughput benchmark.
# Microsoft UDP_SEND_MSG_SIZE contract:
# https://learn.microsoft.com/en-us/windows/win32/winsock/ipproto-udp-socket-options
# Local SDK 10.0.26100.0 shared/ws2ipdef.h defines UDP_SEND_MSG_SIZE = 2.
$ErrorActionPreference = 'Stop'
$sender = [System.Net.Sockets.Socket]::new([System.Net.Sockets.AddressFamily]::InterNetwork, [System.Net.Sockets.SocketType]::Dgram, [System.Net.Sockets.ProtocolType]::Udp)
$receiver = [System.Net.Sockets.Socket]::new([System.Net.Sockets.AddressFamily]::InterNetwork, [System.Net.Sockets.SocketType]::Dgram, [System.Net.Sockets.ProtocolType]::Udp)
try {
    $receiver.Bind([System.Net.IPEndPoint]::new([System.Net.IPAddress]::Loopback, 0))
    $receiver.ReceiveTimeout = 2000
    $sender.Bind([System.Net.IPEndPoint]::new([System.Net.IPAddress]::Loopback, 0))
    $sender.SetSocketOption([System.Net.Sockets.SocketOptionLevel]17, [System.Net.Sockets.SocketOptionName]2, 768)
    $payload = [byte[]]::new(768 + 768 + 123)
    for ($index = 0; $index -lt $payload.Length; $index++) { $payload[$index] = [byte]($index % 251) }
    $sent = $sender.SendTo($payload, $receiver.LocalEndPoint)
    if ($sent -ne $payload.Length) { throw 'Incomplete send acceptance.' }
    $lengths = @()
    $offset = 0
    foreach ($expected in @(768, 768, 123)) {
        $buffer = [byte[]]::new(4096)
        $remote = [System.Net.EndPoint][System.Net.IPEndPoint]::new([System.Net.IPAddress]::Any, 0)
        $received = $receiver.ReceiveFrom($buffer, [ref]$remote)
        if ($received -ne $expected -or -not $remote.Equals($sender.LocalEndPoint)) { throw 'Wrong datagram boundary or origin.' }
        for ($index = 0; $index -lt $received; $index++) {
            if ($buffer[$index] -ne $payload[$offset + $index]) { throw 'Payload changed.' }
        }
        $offset += $received
        $lengths += $received
    }
    $sender.SetSocketOption([System.Net.Sockets.SocketOptionLevel]17, [System.Net.Sockets.SocketOptionName]2, 0)
    [ordered]@{ passed=$true; scope='ephemeral IPv4 loopback segmentation capability; not NIC offload, NovoRUDP acceptance, or TPS'; sent_bytes=$sent; received_datagram_lengths=$lengths; exact_bytes_and_origin=$true } | ConvertTo-Json
} finally {
    $sender.Dispose()
    $receiver.Dispose()
}
