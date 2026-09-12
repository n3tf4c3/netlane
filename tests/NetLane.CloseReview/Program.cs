using System.ComponentModel;
using System.Collections.Specialized;
using System.IO;
using System.Reflection;
using System.Security.Cryptography;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;
using NetLane.Core.Contracts;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.Network.Control;
using NetLane.UI;
using NetLane.UI.Monitoring;

namespace NetLane.CloseReview;

// Manual review host: displays the compiled production window and its native dialogs.
// The injected adapters, rule file and service are synthetic. No routing service is created.
internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        if (args.Length != 1 || args[0] is not ("--check" or "--review")) return 2;
        var runDirectory = Path.Combine(AppContext.BaseDirectory, "review-runs",
            DateTime.UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(runDirectory);
        try { return Run(runDirectory, args[0] == "--check"); }
        catch (Exception error)
        {
            File.WriteAllText(Path.Combine(runDirectory, "error.txt"), error.ToString());
            return 1;
        }
    }

    private static int Run(string runDirectory, bool checkOnly)
    {
        var application = new Application { ShutdownMode = ShutdownMode.OnMainWindowClose };
        var file = new RoutingPolicyFile(Path.Combine(runDirectory, "rules.json"));
        var revision = file.Save([new()
        {
            ApplicationId = "Aplicativo de teste.exe",
            ExecutablePath = Path.Combine(runDirectory, "Aplicativo de teste.exe"),
            RouteMode = NetworkRouteMode.WiFi,
            InterfaceId = ReviewAdapters.WifiId
        }], null);
        var events = new List<ReviewEvent>();
        var observedRows = new HashSet<PolicyRow>();
        var session = new ReviewSession(file, revision);

        // Use the existing injection seam without changing the reviewed binary or its dialogs.
        var window = (MainWindow?)Activator.CreateInstance(typeof(MainWindow),
            BindingFlags.Instance | BindingFlags.NonPublic, binder: null,
            args: [file, new ReviewAdapters(), null,
                new InterfaceSelectionFile(Path.Combine(runDirectory, "interfaces.json")), session, null, null],
            culture: null) ?? throw new InvalidOperationException("A janela revisada não pôde ser criada.");
        application.MainWindow = window;
        var editor = (PolicyEditor)window.DataContext;
        var editedWhileStopping = false;
        var closed = false;
        var state = checkOnly ? "Checking" : "AwaitingManualClose";
        var uiHash = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(typeof(MainWindow).Assembly.Location)));

        void WriteResult()
        {
            var result = new
            {
                RecorderVersion = 2,
                Mode = checkOnly ? "check" : "review",
                State = state,
                UpdatedAtUtc = DateTimeOffset.UtcNow,
                ProcessId = Environment.ProcessId,
                ReviewedUi = typeof(MainWindow).Assembly.Location,
                ReviewedUiHash = uiHash,
                SyntheticPolicyPath = file.FilePath,
                SyntheticPolicyUnchanged = file.Load().Revision == revision,
                SessionIsSimulated = true,
                session.OwnsRunningProcess,
                session.StopPending,
                session.RemainingSeconds,
                EditedWhileStopping = editedWhileStopping,
                editor.HasChanges,
                RuleEnabled = editor.Rows.Single().Enabled,
                WindowClosed = closed,
                Events = events,
                Limits = "Native window and dialogs with manual input; simulated service/adapters and isolated rules. "
                    + "No WFP, UAC, real service, packet capture or proof of real routing cleanup. "
                    + "User screenshots/report must confirm which dialog response was chosen."
            };
            // A separate temporary file keeps read-only observers from seeing a partial JSON write.
            var temporary = Path.Combine(runDirectory, "review.next.json");
            File.WriteAllText(temporary, JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }));
            File.Move(temporary, Path.Combine(runDirectory, "review.json"), overwrite: true);
        }

        void Record(string name)
        {
            events.Add(new(DateTimeOffset.UtcNow, name, session.StopPending,
                session.OwnsRunningProcess, editor.HasChanges, editor.Rows.Single().Enabled));
            WriteResult();
        }

        void SessionChanged(object? sender, EventArgs args)
        {
            if (session.StopPending)
            {
                state = "AwaitingEditDuringSimulatedStop";
                if (!events.Any(e => e.Name == "StopStarted")) Record("StopStarted");
            }
            else if (!session.OwnsRunningProcess)
            {
                state = editedWhileStopping ? "AwaitingNativeDiscardResponse" : "StoppedWithoutEditDuringWait";
                Record("StopCompleted");
            }
        }

        void RowChanged(object? sender, PropertyChangedEventArgs args)
        {
            // This scenario uses only the enabled toggle; adapter refresh can also notify SelectedRoute.
            if (args.PropertyName != nameof(PolicyRow.Enabled)) return;
            editedWhileStopping |= session.StopPending;
            Record(session.StopPending ? "EditedDuringStop" : "EditedOutsideStop");
        }

        void TrackRows(object? sender, NotifyCollectionChangedEventArgs? args)
        {
            // Reload replaces the row objects. Keep exactly one subscription per current row,
            // and detach discarded rows so later attempts cannot silently lose edit evidence.
            foreach (var row in observedRows) row.PropertyChanged -= RowChanged;
            observedRows.Clear();
            foreach (var row in editor.Rows)
            {
                observedRows.Add(row);
                row.PropertyChanged += RowChanged;
            }
        }

        session.Changed += SessionChanged;
        editor.Rows.CollectionChanged += TrackRows;
        TrackRows(null, null);
        ((ListBox)window.FindName("NavigationList")).SelectedIndex = 1;
        var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        timer.Tick += (_, _) =>
        {
            window.Title = session.StopPending
                ? $"ENSAIO DE FECHAMENTO — faltam {session.RemainingSeconds}s: desative a regra de teste sem salvar"
                : "ENSAIO DE FECHAMENTO — regra fictícia e serviço simulado";
            WriteResult();
        };
        window.Title = "ENSAIO DE FECHAMENTO — regra fictícia e serviço simulado";
        window.Closing += (_, args) => Record(args.Cancel ? "CloseDeferredOrCancelled" : "CloseAllowed");
        window.Closed += (_, _) =>
        {
            closed = true;
            timer.Stop();
            editor.Rows.CollectionChanged -= TrackRows;
            foreach (var row in observedRows) row.PropertyChanged -= RowChanged;
            observedRows.Clear();
            state = checkOnly ? "Prepared" : "WindowClosedAwaitingUserReview";
            Record("WindowClosed");
        };
        Record("PreparedWindow");

        if (checkOnly)
        {
            if (!editor.CanEdit || editor.HasChanges || editor.Rows.Count != 1
                || !editor.Rows.Single().Enabled || file.Load().Revision != revision)
                throw new InvalidOperationException("A preparação do cenário não preservou a referência esperada.");
            for (var attempt = 0; attempt < 2; attempt++)
            {
                var discardedRow = editor.Rows.Single();
                editor.Load();
                var previousEvents = events.Count;
                discardedRow.Enabled = false;
                if (events.Count != previousEvents || editor.HasChanges)
                    throw new InvalidOperationException("O registrador ainda acompanha uma linha descartada.");
                editor.Rows.Single().Enabled = false;
                if (events.Count != previousEvents + 1 || !editor.HasChanges
                    || events[^1].Name != "EditedOutsideStop")
                    throw new InvalidOperationException("A edição após recarregar não foi registrada exatamente uma vez.");
                editor.Load();
                if (editor.HasChanges || !editor.Rows.Single().Enabled || file.Load().Revision != revision)
                    throw new InvalidOperationException("A conferência do registrador alterou o arquivo sintético.");
            }
            Record("RecorderReloadCheckPassed");
            session.EndForPreparationCheck();
            window.Close();
            return 0;
        }

        window.Loaded += (_, _) => { Record("WindowLoaded"); timer.Start(); };
        application.Run(window);
        return 0;
    }
}

internal sealed record ReviewEvent(DateTimeOffset AtUtc, string Name, bool StopPending,
    bool SimulatedSessionActive, bool HasChanges, bool RuleEnabled);

internal sealed class ReviewAdapters : INetworkInterfaceDetector
{
    public const string WifiId = "11111111-1111-1111-1111-111111111111";
    public IReadOnlyList<NetworkAdapter> GetConnectedAdapters() => [new()
    {
        AdapterId = WifiId, Name = "Wi-Fi de teste", InterfaceType = "Wireless80211",
        IpAddress = "192.0.2.10", IsConnected = true, HasGateway = true
    }];
}

internal sealed class ReviewSession(RoutingPolicyFile file, string revision) : IServiceSession
{
    public bool OwnsRunningProcess { get; private set; } = true;
    public bool IsAvailable => false;
    public string? ServiceExecutablePath => null;
    public bool StopPending { get; private set; }
    public int RemainingSeconds { get; private set; }
    public string Status { get; private set; } = "Ensaio de fechamento: sessão simulada, sem roteamento.";
    public RoutingServiceSnapshot? Snapshot => OwnsRunningProcess ? new(DateTimeOffset.UtcNow,
        Environment.ProcessId, file.FilePath, revision, "ManualUiReview", "Attention",
        [new("Aplicativo de teste.exe", false, "Sessão simulada para revisar os diálogos; não aplica roteamento.")]) : null;
    public event EventHandler? Changed;

    public Task StartAsync(bool allowTemporaryRoutePolicies, CancellationToken cancellationToken = default)
        => throw new InvalidOperationException("O ensaio não inicia um serviço real nem pode ser reiniciado nesta janela.");

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        if (StopPending) throw new InvalidOperationException("A parada simulada já está em andamento.");
        if (!OwnsRunningProcess) return;
        StopPending = true;
        try
        {
            for (RemainingSeconds = 15; RemainingSeconds > 0; RemainingSeconds--)
            {
                Status = $"Parada simulada: faltam {RemainingSeconds} segundos. Edite a regra fictícia sem salvar.";
                Changed?.Invoke(this, EventArgs.Empty);
                await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken);
            }
        }
        finally
        {
            StopPending = false;
            Finish();
        }
    }

    public void EndForPreparationCheck() => Finish();

    private void Finish()
    {
        OwnsRunningProcess = false;
        Status = "Sessão simulada encerrada. Nenhuma opção de rede foi alterada.";
        new RoutingStatusFile(file.FilePath).Write(new(DateTimeOffset.UtcNow, Environment.ProcessId,
            file.FilePath, revision, "ManualUiReview", "Stopped", []));
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
