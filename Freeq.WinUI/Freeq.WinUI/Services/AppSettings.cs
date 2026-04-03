using System.Text.Json;

namespace Freeq.WinUI.Services;

/// <summary>
/// Persists user preferences to %LOCALAPPDATA%\Freeq\settings.json.
/// </summary>
public class AppSettings
{
    private static readonly string FilePath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Freeq", "settings.json");

    private SettingsData _data = new();

    public AppSettings()
    {
        Load();
    }

    public HashSet<string> MutedChannels => _data.MutedChannels;

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
            File.WriteAllText(FilePath, JsonSerializer.Serialize(_data, new JsonSerializerOptions { WriteIndented = true }));
        }
        catch (Exception ex)
        {
            OAuthLog.Write($"AppSettings.Save failed: {ex.Message}");
        }
    }

    private void Load()
    {
        if (!File.Exists(FilePath)) return;
        try
        {
            var json = File.ReadAllText(FilePath);
            _data = JsonSerializer.Deserialize<SettingsData>(json) ?? new();
        }
        catch (Exception ex)
        {
            OAuthLog.Write($"AppSettings.Load failed: {ex.Message}");
        }
    }

    private class SettingsData
    {
        public HashSet<string> MutedChannels { get; set; } = new(StringComparer.OrdinalIgnoreCase);
    }
}
