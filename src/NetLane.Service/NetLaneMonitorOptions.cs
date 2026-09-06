namespace NetLane.Service;

public sealed class NetLaneMonitorOptions
{
    public const int DefaultPollIntervalSeconds = 5;
    public const int DefaultTopApplications = 8;

    public int PollIntervalSeconds { get; set; } = DefaultPollIntervalSeconds;
    public int TopApplicationsToLog { get; set; } = DefaultTopApplications;
    public bool IncludeInactiveInterfacesInSnapshot { get; set; } = false;
}
