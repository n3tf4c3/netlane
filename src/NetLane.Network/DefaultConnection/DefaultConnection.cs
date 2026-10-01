namespace NetLane.Network.DefaultConnection;

public sealed record ConnectionInterface(Guid Id, string Name, string Kind, uint Index, bool Connected,
    bool AutomaticMetric, uint Metric, uint? DefaultRouteMetric, uint? SavedMetric = null);
public sealed record ConnectionRoute(uint Index, string Name, uint InterfaceMetric, uint RouteMetric);
public sealed record ConnectionSnapshot(ConnectionInterface[] Interfaces, ConnectionRoute[] Routes)
{
    public string CurrentLabel
    {
        get
        {
            if (Routes.Length == 0) return "Sem rota padrão IPv4 disponível";
            var minimum = Routes.Min(r => (ulong)r.InterfaceMetric + r.RouteMetric);
            var winners = Routes.Where(r => (ulong)r.InterfaceMetric + r.RouteMetric == minimum).DistinctBy(r => r.Index).ToArray();
            return winners.Length == 1 ? $"Atual: {winners[0].Name}" : "Prioridades iguais — Windows decide";
        }
    }
}
public sealed record ConnectionMetric(Guid Id, bool AutomaticMetric, uint Metric);
public sealed record ConnectionRequest(ConnectionSnapshot Expected, ConnectionMetric[] Changes, Guid? PreferredInterface);
public sealed record ConnectionResult(ConnectionSnapshot Snapshot, bool Success, string Detail, bool RollbackConfirmed = false);

public interface IConnectionSettings
{
    Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default);
    Task WriteAsync(ConnectionMetric metric, CancellationToken token = default);
}

public interface IDefaultConnectionControl
{
    bool IsAvailable { get; }
    Task<ConnectionSnapshot> ReadAsync(CancellationToken token = default);
    Task<ConnectionResult> ApplyAsync(ConnectionRequest request, CancellationToken token = default);
}

public static class ConnectionPriority
{
    public static ConnectionMetric[] Plan(ConnectionSnapshot snapshot, Guid preferred, IEnumerable<Guid>? previouslyManaged = null)
    {
        Validate(snapshot);
        var selected = snapshot.Interfaces.SingleOrDefault(i => i.Id == preferred)
            ?? throw new InvalidOperationException("A conexão escolhida não está disponível.");
        if (!selected.Connected || selected.DefaultRouteMetric is null)
            throw new InvalidOperationException("Conecte a interface escolhida à rede antes de aplicar.");
        var managed = (previouslyManaged ?? []).ToHashSet();
        var affected = snapshot.Interfaces.Where(i => i.Id == preferred || i.DefaultRouteMetric is not null || managed.Contains(i.Id)).ToArray();
        if (affected.Any(i => i.AutomaticMetric ? i.SavedMetric is > 0 : i.SavedMetric != i.Metric))
            throw new InvalidOperationException("A prioridade ativa difere da configuração salva no Windows. Essa configuração foi preservada; confira antes de aplicar.");
        var targetTotal = (ulong)selected.DefaultRouteMetric.Value + 5;
        if (targetTotal + 50 > uint.MaxValue) throw new InvalidOperationException("A prioridade dessa rota não pode ser ajustada.");
        if (snapshot.Routes.Any(r => !affected.Any(i => i.Index == r.Index) && (ulong)r.RouteMetric + r.InterfaceMetric <= targetTotal))
            throw new InvalidOperationException("Outra conexão, como uma VPN, tem prioridade. Ajuste essa conexão antes de escolher o padrão.");
        return affected.Select(i => new ConnectionMetric(i.Id, false, i.Id == preferred ? 5u : (uint)(targetTotal + 50))).ToArray();
    }

    public static void Validate(ConnectionSnapshot snapshot)
    {
        if (snapshot is null || snapshot.Interfaces is null || snapshot.Routes is null || snapshot.Interfaces.Length > 64
            || snapshot.Interfaces.Any(i => i is null || i.Id == Guid.Empty || i.Index == 0 || i.Kind is not ("Cabo" or "Wi-Fi"))
            || snapshot.Interfaces.Select(i => i.Id).Distinct().Count() != snapshot.Interfaces.Length
            || snapshot.Interfaces.Select(i => i.Index).Distinct().Count() != snapshot.Interfaces.Length)
            throw new InvalidDataException("Inventário de conexões inválido.");
    }

    public static bool Matches(ConnectionInterface actual, ConnectionMetric expected) => actual.Id == expected.Id
        && actual.AutomaticMetric == expected.AutomaticMetric
        && (expected.AutomaticMetric ? actual.SavedMetric is null or 0 : actual.Metric == expected.Metric && actual.SavedMetric == expected.Metric);

    public static ConnectionMetric Metric(ConnectionInterface adapter) => new(adapter.Id, adapter.AutomaticMetric, adapter.Metric);
}

public sealed class ConnectionPriorityTransaction(IConnectionSettings settings)
{
    public async Task<ConnectionResult> ApplyAsync(ConnectionRequest request, CancellationToken token = default)
    {
        var before = await settings.ReadAsync(token);
        ConnectionPriority.Validate(before);
        ConnectionPriority.Validate(request.Expected);
        if (request.Changes is null || request.Changes.Length is 0 or > 64 || request.Changes.Any(c => c is null)
            || request.Changes.Select(c => c.Id).Distinct().Count() != request.Changes.Length)
            throw new InvalidDataException("Lista de alterações inválida.");
        foreach (var change in request.Changes)
        {
            var actual = before.Interfaces.SingleOrDefault(i => i.Id == change.Id);
            var expected = request.Expected.Interfaces.SingleOrDefault(i => i.Id == change.Id);
            if (actual is null || expected is null || actual.Index != expected.Index || actual.Kind != expected.Kind
                || actual.Connected != expected.Connected || actual.DefaultRouteMetric != expected.DefaultRouteMetric
                || !ConnectionPriority.Matches(actual, ConnectionPriority.Metric(expected)))
                throw new InvalidOperationException("A configuração mudou. Atualize a leitura e tente novamente.");
            if (!change.AutomaticMetric && change.Metric == 0) throw new InvalidDataException("Prioridade manual inválida.");
        }
        if (request.PreferredInterface is { } selected)
        {
            var expectedPlan = ConnectionPriority.Plan(before, selected, request.Changes.Select(c => c.Id));
            if (!expectedPlan.OrderBy(c => c.Id).SequenceEqual(request.Changes.OrderBy(c => c.Id)))
                throw new InvalidDataException("Alteração não corresponde à conexão escolhida.");
        }
        var touched = new List<ConnectionInterface>();
        try
        {
            // Raise alternatives first; lower the selected interface last.
            foreach (var change in request.Changes.OrderBy(c => c.Id == request.PreferredInterface))
            {
                token.ThrowIfCancellationRequested();
                touched.Add(before.Interfaces.Single(i => i.Id == change.Id));
                await settings.WriteAsync(change, token);
            }
            var after = await settings.ReadAsync(token);
            if (request.Changes.Any(c => !after.Interfaces.Any(i => ConnectionPriority.Matches(i, c))))
                throw new IOException("O Windows não confirmou as prioridades solicitadas.");
            if (request.PreferredInterface is { } preferred)
            {
                var target = after.Interfaces.Single(i => i.Id == preferred);
                var best = after.Routes.OrderBy(r => (ulong)r.RouteMetric + r.InterfaceMetric).FirstOrDefault();
                if (!target.Connected || best is null || best.Index != target.Index
                    || after.Routes.Any(r => r.Index != target.Index && (ulong)r.RouteMetric + r.InterfaceMetric <= (ulong)best.RouteMetric + best.InterfaceMetric))
                    throw new IOException("A conexão escolhida não se tornou a rota padrão IPv4.");
            }
            return new(after, true, request.PreferredInterface is null ? "Configuração anterior restaurada." : "Conexão padrão IPv4 aplicada. A escolha permanece após fechar o NetLane.");
        }
        catch (Exception ex)
        {
            var errors = new List<string>();
            using var cleanup = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            foreach (var original in touched.AsEnumerable().Reverse())
            {
                try { await settings.WriteAsync(ConnectionPriority.Metric(original), cleanup.Token); }
                catch (Exception restoreError) { errors.Add(restoreError.Message); }
            }
            ConnectionSnapshot after;
            try { after = await settings.ReadAsync(cleanup.Token); }
            catch { after = new([], []); errors.Add("Não foi possível conferir a restauração."); }
            var restored = errors.Count == 0 && touched.All(i => after.Interfaces.Any(a => ConnectionPriority.Matches(a, ConnectionPriority.Metric(i))));
            return new(after, false, ex.Message + (restored ? " Alteração revertida." : " Restauração incompleta: " + string.Join(" | ", errors)), restored);
        }
    }
}
