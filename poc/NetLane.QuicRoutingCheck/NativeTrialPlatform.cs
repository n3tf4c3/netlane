using System.Diagnostics;
using System.Net;
using System.Net.NetworkInformation;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Network;
using NetLane.Network.Control;

namespace NetLane.QuicRoutingCheck;

internal sealed record TrialRequest(string RepositoryPath, string ProbePath, string OutputDirectory,
    string SnapshotScriptPath, string SnapshotScriptSha256, string RulesPath, string ReferenceNetworkJson,
    Guid EthernetId, Guid WiFiId, string Host, Dictionary<string, string> ProbeHashes, DateTimeOffset CreatedAtUtc,
    bool Concurrent = false, string? PeerProbePath = null);

internal sealed partial class NativeTrialPlatform(TrialRequest request, bool requireAdministrator) : IConcurrentTrialPlatform
{
    public IRoutePolicySettings Settings { get; } = new WindowsRoutePolicySettings();

    public TrialState ReadAndValidateState(bool expectEnabledFlags)
    {
        var prerequisites = WindowsRoutingPrerequisites.Check();
        if (!prerequisites.ApiAvailable || (requireAdministrator && !prerequisites.IsAdministrator))
            throw new InvalidOperationException(prerequisites.Summary);
        if (prerequisites.Ipv4RoutePolicies != expectEnabledFlags || prerequisites.Ipv6RoutePolicies != expectEnabledFlags)
            throw new InvalidOperationException("Flags routepolicies não correspondem à fase; nenhuma correção presumida.");
        VerifyFiles();
        if (!Hash(request.SnapshotScriptPath).Equals(request.SnapshotScriptSha256, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("O coletor de referência mudou.");
        using var otherControllers = new ProcessList(Process.GetProcessesByName("NetLane.QuicRoutingCheck"));
        if (otherControllers.Items.Any(process => process.Id != Environment.ProcessId))
            throw new InvalidOperationException("Outro controlador QUIC está em execução.");

        var powershell = Path.Combine(Environment.SystemDirectory, "WindowsPowerShell", "v1.0", "powershell.exe");
        var start = new ProcessStartInfo(powershell)
        {
            UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8
        };
        foreach (var arg in new[] { "-NoProfile", "-NonInteractive", "-File", request.SnapshotScriptPath, "-RulesPath", request.RulesPath })
            start.ArgumentList.Add(arg);
        using var snapshot = Process.Start(start) ?? throw new IOException("Coletor não iniciou.");
        var output = snapshot.StandardOutput.ReadToEndAsync();
        var error = snapshot.StandardError.ReadToEndAsync();
        if (!snapshot.WaitForExit(20000))
        {
            snapshot.Kill(); snapshot.WaitForExit(); // Only this read-only collector, never a service/controller.
            throw new TimeoutException("Coletor somente leitura excedeu 20 segundos.");
        }
        Task.WhenAll(output, error).GetAwaiter().GetResult();
        if (snapshot.ExitCode != 0 || !string.IsNullOrWhiteSpace(error.Result))
            throw new IOException("Coletor somente leitura falhou: " + error.Result);
        var network = output.Result.Trim();
        if (network != request.ReferenceNetworkJson)
            throw new InvalidOperationException("Rede/regras/serviço mudaram desde a referência. Ensaio interrompido, sem sobrescrever configurações.");
        using var parsed = JsonDocument.Parse(network);
        var root = parsed.RootElement;
        if (root.GetProperty("ServiceProcessIds").GetArrayLength() != 0)
            throw new InvalidOperationException("Serviço real em execução, fora do escopo.");
        var adapters = root.GetProperty("Interfaces").EnumerateArray().ToArray();
        if (adapters.Length != 2) throw new InvalidOperationException("Referência de interfaces ambígua.");
        var ethernet = ReadAdapter(request.EthernetId, NetworkRouteMode.Ethernet, adapters);
        var wifi = ReadAdapter(request.WiFiId, NetworkRouteMode.WiFi, adapters);
        if (ethernet.Id == wifi.Id || ethernet.Address == wifi.Address)
            throw new InvalidOperationException("O ensaio exige interfaces e IPv4 locais distintos.");
        return new(network, ethernet, wifi);
    }

    private static TrialAdapter ReadAdapter(Guid id, NetworkRouteMode mode, JsonElement[] adapters)
    {
        var item = adapters.Single(adapter => Guid.Parse(adapter.GetProperty("Guid").GetString()!) == id);
        var native = NetworkInterface.GetAllNetworkInterfaces().Single(adapter => Guid.Parse(adapter.Id) == id);
        if (native.OperationalStatus != OperationalStatus.Up || item.GetProperty("Status").GetString() != "Up" ||
            native.NetworkInterfaceType != (mode == NetworkRouteMode.Ethernet ? NetworkInterfaceType.Ethernet : NetworkInterfaceType.Wireless80211))
            throw new InvalidOperationException("Interface indisponível ou com tipo inesperado.");
        var bindings = item.GetProperty("Bindings").EnumerateArray().ToArray();
        if (!bindings.Single(binding => binding.GetProperty("ComponentID").GetString() == "ms_tcpip").GetProperty("Enabled").GetBoolean() ||
            bindings.Single(binding => binding.GetProperty("ComponentID").GetString() == "ms_tcpip6").GetProperty("Enabled").GetBoolean() ||
            native.Supports(NetworkInterfaceComponent.IPv6))
            throw new InvalidOperationException("Escopo exige IPv4 habilitado e IPv6 desabilitado nas duas placas.");
        var addresses = item.GetProperty("Addresses").EnumerateArray()
            .Select(address => IPAddress.Parse(address.GetProperty("IPAddress").GetString()!)).ToArray();
        if (addresses.Length != 1 || addresses[0].AddressFamily != System.Net.Sockets.AddressFamily.InterNetwork || IPAddress.IsLoopback(addresses[0]))
            throw new InvalidOperationException("Cada placa deve ter um IPv4 inequívoco e nenhum IPv6 neste ensaio.");
        if (!item.GetProperty("DefaultRoutes").EnumerateArray().Any(route => route.GetProperty("DestinationPrefix").GetString() == "0.0.0.0/0"))
            throw new InvalidOperationException("Interface sem rota padrão IPv4.");
        return new(id, native.Name, mode, addresses[0].ToString());
    }

    public async Task<TrialProbe> ProbeAsync(string stage, string? destination, CancellationToken cancellationToken)
    {
        VerifyFiles();
        var start = new ProcessStartInfo(request.ProbePath)
        { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
        foreach (var arg in new[] { "--handshake", "--host", request.Host, "--timeout-seconds", "15" }) start.ArgumentList.Add(arg);
        if (destination is not null) { start.ArgumentList.Add("--ipv4"); start.ArgumentList.Add(destination); }
        cancellationToken.ThrowIfCancellationRequested();
        using var process = Process.Start(start) ?? throw new IOException("Probe não iniciou.");
        var output = process.StandardOutput.ReadToEndAsync();
        var error = process.StandardError.ReadToEndAsync();
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        deadline.CancelAfter(TimeSpan.FromSeconds(30));
        try { await process.WaitForExitAsync(deadline.Token); }
        catch
        {
            if (!process.HasExited) process.Kill(); // Only the probe; controller stays alive to restore its policy/flags.
            await process.WaitForExitAsync(CancellationToken.None);
            await Task.WhenAll(output, error);
            throw;
        }
        await Task.WhenAll(output, error);
        Save($"raw-{stage}.json", new { ProcessId = process.Id, ExitCode = process.ExitCode, StandardError = error.Result, StandardOutput = output.Result });
        VerifyFiles();
        return ParseProbe(output.Result, process.Id, process.ExitCode, request.ProbePath);
    }

    internal static TrialProbe ParseProbe(string json, int pid, int exitCode, string expectedPath, string expectedMode = "Handshake")
    {
        using var parsed = JsonDocument.Parse(json);
        var root = parsed.RootElement;
        if (exitCode != 0 || root.GetProperty("SchemaVersion").GetInt32() != 1 || root.GetProperty("ProcessId").GetInt32() != pid ||
            !string.Equals(root.GetProperty("ProcessPath").GetString(), expectedPath, StringComparison.OrdinalIgnoreCase) ||
            root.GetProperty("Mode").GetString() != expectedMode || root.GetProperty("Status").GetString() != "HandshakeObserved" ||
            !root.GetProperty("Supported").GetBoolean() || !root.GetProperty("ConnectionDisposed").GetBoolean() ||
            !root.GetProperty("NetworkAttempted").GetBoolean() || root.GetProperty("RoutingChangedByProbe").GetBoolean() ||
            root.GetProperty("ManualSourceBinding").GetBoolean() || root.GetProperty("TcpFallback").GetBoolean() ||
            root.GetProperty("HttpResponseVerified").GetBoolean())
            throw new InvalidDataException("Probe não confirmou handshake isolado, processo e descarte. Consulte o recibo bruto.");
        var handshake = root.GetProperty("Handshake");
        var local = IPAddress.Parse(handshake.GetProperty("LocalAddress").GetString()!);
        var remote = IPAddress.Parse(handshake.GetProperty("RemoteAddress").GetString()!);
        var interfaces = root.GetProperty("ObservedInterfaces");
        if (handshake.GetProperty("Alpn").GetString() != "h3" || handshake.GetProperty("RemotePort").GetInt32() != 443 ||
            handshake.GetProperty("LocalPort").GetInt32() is <= 0 or > 65535 ||
            local.AddressFamily != System.Net.Sockets.AddressFamily.InterNetwork || IPAddress.IsLoopback(local) || local.Equals(IPAddress.Any) ||
            remote.AddressFamily != System.Net.Sockets.AddressFamily.InterNetwork || root.GetProperty("TargetIpv4").GetString() != remote.ToString() ||
            interfaces.GetArrayLength() != 1 || interfaces[0].GetProperty("Status").GetString() != "Up" ||
            !interfaces[0].GetProperty("Ipv4Addresses").EnumerateArray().Any(address => address.GetString() == local.ToString()))
            throw new InvalidDataException("QUIC, IPv4, destino ou associação de interface não confirmados.");
        return new(pid, local.ToString(), remote.ToString(), Guid.Parse(interfaces[0].GetProperty("Id").GetString()!));
    }

    public ITrialSession OpenSession() => new NativeSession(request.ProbePath);

    public void Save(string name, object value)
    {
        if (Path.GetFileName(name) != name) throw new ArgumentException("Nome de recibo inválido.");
        using var stream = new FileStream(Path.Combine(request.OutputDirectory, name), FileMode.CreateNew, FileAccess.Write, FileShare.Read);
        JsonSerializer.Serialize(stream, value, new JsonSerializerOptions { WriteIndented = true });
        stream.Flush(true);
    }

    private void VerifyFiles()
    {
        foreach (var path in request.Concurrent ? new[] { request.ProbePath, request.PeerProbePath! } : new[] { request.ProbePath })
            foreach (var (file, expectedHash) in request.ProbeHashes)
                if (!Hash(Path.Combine(Path.GetDirectoryName(path)!, file)).Equals(expectedHash, StringComparison.OrdinalIgnoreCase))
                    throw new InvalidOperationException("Binário do probe mudou; ensaio interrompido.");
    }

    internal static string Hash(string path)
    {
        using var stream = File.OpenRead(path);
        return Convert.ToHexString(SHA256.HashData(stream));
    }

    private sealed class NativeSession : ITrialSession
    {
        private readonly WfpRoutingEngine _engine = new();
        private readonly ApplicationIdentity _application;
        public NativeSession(string probePath) => _application = new() { Name = "NetLane isolated QUIC probe", ExecutablePath = probePath };
        public RoutingApplyResult Apply(TrialAdapter adapter) => _engine.ApplyRule(_application, new NetworkRule
        { ApplicationId = _application.Name, InterfaceId = adapter.Id.ToString(), RouteMode = adapter.Mode, Enabled = true });
        public void RemoveAll()
        {
            _engine.RemoveAllRules();
            if (_engine.GetAppliedRules().Count != 0) throw new IOException("Políticas da sessão não foram removidas.");
        }
        public void Dispose() => _engine.Dispose();
    }

    private sealed class ProcessList(Process[] items) : IDisposable
    {
        public Process[] Items { get; } = items;
        public void Dispose() { foreach (var process in Items) process.Dispose(); }
    }
}
