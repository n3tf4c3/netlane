namespace NetLane.TrayRoutingReview;

internal static class ReviewSafety
{
    public static void ValidateLimit(int minutes)
    {
        if (minutes is not (5 or 30))
            throw new InvalidDataException("O ensaio exige limite explícito de 5 ou 30 minutos; duração ilimitada não é permitida.");
    }

    public static int HistoryCapacity(int minutes)
    {
        ValidateLimit(minutes);
        // Cover the active limit plus preparation, with room for visibility events as well as one-second samples.
        return Math.Max(1200, (minutes + 15) * 120);
    }

    public static string? StopReason(int minutes, TimeSpan elapsed, bool stopRequested, bool policyChanged)
    {
        ValidateLimit(minutes);
        if (policyChanged) return "Ensaio interrompido por mudança da regra isolada.";
        if (stopRequested) return "Ensaio interrompido por pedido de parada.";
        return elapsed >= TimeSpan.FromMinutes(minutes) ? $"Ensaio interrompido por limite de {minutes} minutos." : null;
    }
}
