namespace NetLane.Core.Models;

public static class LocalPath
{
    /// <summary>
    /// Local drive path (<c>C:\…</c>), decided by text before any filesystem access. UNC
    /// (<c>\\host\…</c>, <c>//host/…</c>), device (<c>\\?\</c>, <c>\\.\</c>), relative paths and
    /// alternate data streams are refused, so an elevated caller never opens an SMB connection
    /// while checking existence. Same rule as <c>is_local_path</c> in the Rust port.
    /// </summary>
    public static bool IsLexicallyLocal(string? path) =>
        path is { Length: >= 3 }
        && char.IsAsciiLetter(path[0]) && path[1] == ':' && path[2] is '\\' or '/'
        && path.IndexOf(':', 2) < 0 && !path.Contains('\0');
}
