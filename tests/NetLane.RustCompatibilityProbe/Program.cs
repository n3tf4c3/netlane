using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using NetLane.Core.Persistence;

try
{
    if (args is ["--compare", var left, var right])
    {
        if (!JsonNode.DeepEquals(JsonNode.Parse(File.ReadAllText(left)), JsonNode.Parse(File.ReadAllText(right))))
            throw new InvalidDataException("Resultados .NET/Rust divergentes.");
        Console.WriteLine("Equivalent");
        return 0;
    }
    if (args is ["--validate", var path])
    {
        var options = new JsonSerializerOptions(JsonSerializerDefaults.Web)
        { Converters = { new JsonStringEnumConverter() } };
        var snapshot = new RoutingPolicyFile(path).Load();
        Console.WriteLine(JsonSerializer.Serialize(snapshot.Policies, options));
        return 0;
    }
    throw new ArgumentException("Use --validate <fixture> ou --compare <resultado .NET> <resultado Rust>.");
}
catch (Exception error)
{
    Console.Error.WriteLine(error.Message);
    return 1;
}
