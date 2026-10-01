using System.IO;
using Microsoft.Win32;

namespace NetLane.UI.Startup;

public interface IStartupRegistration
{
    bool IsAvailable { get; }
    bool ReadEnabled();
    void SetEnabled(bool enabled);
}

public sealed class WindowsStartupRegistration : IStartupRegistration
{
    internal const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    internal const string ValueName = "NetLane";
    private readonly RegistryKey _hive;
    private readonly string _runKey;
    private readonly string? _command;
    public bool IsAvailable => _command is not null;

    public WindowsStartupRegistration() : this(Environment.ProcessPath, AppContext.BaseDirectory, Registry.CurrentUser, RunKey) { }

    // Tests use a disposable key, never the user's real Run key.
    internal WindowsStartupRegistration(string? executable, string directory, RegistryKey hive, string runKey)
    {
        _hive = hive; _runKey = runKey;
        if (string.IsNullOrWhiteSpace(executable) || !ApplicationPaths.IsInstalledLayout(directory)
            || !Path.IsPathFullyQualified(executable) || !File.Exists(executable)
            || !string.Equals(Path.GetFileName(executable), "NetLane.UI.exe", StringComparison.OrdinalIgnoreCase)
            || !string.Equals(Path.GetDirectoryName(Path.GetFullPath(executable)), Path.TrimEndingDirectorySeparator(Path.GetFullPath(directory)), StringComparison.OrdinalIgnoreCase)
            || executable.IndexOfAny(['"', '\r', '\n']) >= 0) return;
        var command = $"\"{Path.GetFullPath(executable)}\" --startup";
        if (command.Length <= 260) _command = command;
    }

    public bool ReadEnabled()
    {
        if (!IsAvailable) return false;
        using var key = _hive.OpenSubKey(_runKey);
        return Matches(key?.GetValue(ValueName, null, RegistryValueOptions.DoNotExpandEnvironmentNames));
    }

    public void SetEnabled(bool enabled)
    {
        if (!IsAvailable) throw new InvalidOperationException("Disponível no NetLane instalado.");
        using var key = _hive.CreateSubKey(_runKey, writable: true);
        var current = key.GetValue(ValueName, null, RegistryValueOptions.DoNotExpandEnvironmentNames);
        if (current is not null && !Matches(current))
            throw new InvalidOperationException("Já existe uma inicialização NetLane de outro local. Gerencie essa entrada nas configurações do Windows.");
        if (enabled) key.SetValue(ValueName, _command!, RegistryValueKind.String);
        else key.DeleteValue(ValueName, throwOnMissingValue: false);
        if (ReadEnabled() != enabled) throw new IOException("O Windows não confirmou a preferência de inicialização.");
    }

    private bool Matches(object? value) => value is string command && string.Equals(command, _command, StringComparison.OrdinalIgnoreCase);
}
