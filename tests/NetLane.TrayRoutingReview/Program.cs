using System.IO;
using System.Text.Json;
using NetLane.Core.Models;
using NetLane.Core.Persistence;
using NetLane.QuicRoutingCheck;

namespace NetLane.TrayRoutingReview;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        ReviewEnvironment? environment = null;
        try
        {
            if (args.Length is < 3 or > 4 || args[0] is not ("--prepare" or "--review" or "--measure" or "--verify")) return 2;
            if (NativeTrialPlatform.Hash(args[1]) != args[2]) throw new InvalidDataException("Hash do pedido não confere.");
            var request = JsonSerializer.Deserialize<ReviewRequest>(File.ReadAllText(args[1])) ?? throw new InvalidDataException("Pedido vazio.");
            environment = new(request, args[1], args[2]);
            environment.ValidateFiles(recent: args[0] is "--prepare" or "--review");
            if (args[0] == "--prepare")
            {
                var before = environment.Platform.ReadAndValidateState(false);
                var revision = new RoutingPolicyFile(environment.PolicyPath).Save([new()
                {
                    ApplicationId = "Probe isolado - bandeja", ExecutablePath = request.Trial.ProbePath,
                    InterfaceId = request.Trial.WiFiId.ToString(), RouteMode = NetworkRouteMode.WiFi,
                    Enabled = true, IncludeRelatedExecutables = false
                }], null);
                environment.Platform.Save("prepared.json", new PreparedReview(environment.PolicyPath, revision, before));
                return 0;
            }
            environment.ValidatePolicy(environment.Read<PreparedReview>("prepared.json"));
            if (args[0] == "--review") return ReviewWindow.Run(environment);
            if (args[0] == "--measure" && args.Length == 4)
            {
                ReviewMeasurement.RunAsync(environment, args[3]).GetAwaiter().GetResult();
                return 0;
            }
            if (args[0] == "--verify")
            {
                ReviewMeasurement.VerifyAsync(environment).GetAwaiter().GetResult();
                return 0;
            }
            return 2;
        }
        catch (Exception error)
        {
            if (environment is not null)
                environment.Platform.Save("error-" + Guid.NewGuid().ToString("N") + ".json", new { Mode = args[0], AtUtc = DateTimeOffset.UtcNow, Error = error.ToString() });
            return 1;
        }
    }
}
