using System.Text.Json;

namespace NetLane.QuicProbe;

internal static class Program
{
    private static async Task<int> Main(string[] args)
    {
        ProbeOptions options;
        try { options = ProbeOptions.Parse(args); }
        catch (ArgumentException ex)
        {
            Console.Error.WriteLine(ex.Message);
            return 64;
        }
        if (options.Mode == ProbeMode.Help)
        {
            Console.WriteLine("""
                NetLane.QuicProbe — observador isolado, sem políticas de roteamento.
                --check-support
                  Consulta QUIC/MsQuic sem DNS nem tráfego.
                --concurrent-child --host <nome.dns> --ipv4 <destino>
                  Uso pelo controlador: espera GO em stdin e observa a conexão por 3 segundos.
                --handshake --host <nome.dns> [--ipv4 <destino>] [--timeout-seconds <1..30>]
                  Nova conexão QUIC/IPv4 na porta 443, ALPN h3 e certificado validado.
                  Só handshake: não envia requisição HTTP ou arquivo do usuário.
                  JSON em stdout; nenhum arquivo é escrito pelo probe.
                  Sem bind de origem, proxy, fallback TCP, serviço, WFP ou netsh.
                Sem argumentos: mostra esta ajuda e não acessa a rede.
                """);
            return 0;
        }
        using var stop = new CancellationTokenSource();
        ConsoleCancelEventHandler cancel = (_, e) => { e.Cancel = true; stop.Cancel(); };
        Console.CancelKeyPress += cancel;
        try
        {
            var report = options.Mode == ProbeMode.ConcurrentChild
                ? await ConcurrentProbe.RunAsync(options, Console.In, Console.Out, stop.Token)
                : await ProbeRunner.RunAsync(options, new QuicHandshake(), stop.Token);
            Console.WriteLine(JsonSerializer.Serialize(report, new JsonSerializerOptions { WriteIndented = true }));
            return ProbeRunner.ExitCode(report);
        }
        finally { Console.CancelKeyPress -= cancel; }
    }
}
